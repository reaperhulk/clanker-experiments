// SPDX-License-Identifier: LGPL-3.0-or-later
//! All-intra H.264 encoder for the formats and profiles rusty_h264-encoder
//! does not cover: 4:0:0 (High), 10 bits (High 10), 4:2:2 (High 4:2:2),
//! 4:4:4 (High 4:4:4 Predictive) and lossless coding (transform bypass,
//! High 4:4:4 Predictive), the formats libheif's x264 plugin encodes.
//!
//! One IDR picture per call, one slice, CABAC. Macroblocks are Intra4x4,
//! Intra8x8 or Intra16x16, chosen by rate/distortion with CABAC rate
//! estimates; the deblocking filter is left to the decoder.

#![allow(clippy::needless_range_loop, clippy::type_complexity)]

mod cabac;
mod init;
mod intra;
mod transform;

use crate::avc_encoder::{BitWriter, VuiSignal, escape, level, write_vui};
use cabac::{Contexts, Counter, Encoder, Sink};
use intra::Edge;
use transform::{Steps, ZIGZAG4, ZIGZAG8};

/// One picture: `planes` are tightly packed samples (`width` x `height`
/// luma; chroma subsampled by `chroma` 0-3 = 4:0:0, 4:2:0, 4:2:2, 4:4:4).
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    pub chroma: u8,
    pub bit_depth: u8,
    pub planes: [&'a [u16]; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// QPY (-QpBdOffsetY..=51); ignored when `lossless`.
    pub qp: i32,
    pub lossless: bool,
    pub transform_8x8: bool,
    /// Deblocking filter in the decoder (turned off only for exact
    /// reconstruction tests).
    pub deblocking: bool,
    pub vui: VuiSignal,
}

/// The profile x264 chooses for these streams (intra only, no scaling lists).
pub fn profile(chroma: u8, bit_depth: u8, lossless: bool) -> u8 {
    if lossless || chroma == 3 {
        244
    } else if chroma == 2 {
        122
    } else if bit_depth > 8 {
        110
    } else {
        100
    }
}

// ---------------------------------------------------------------------------
// Frame buffers and per-macroblock state

struct Plane {
    width: usize,
    height: usize,
    data: Vec<i32>,
}

impl Plane {
    fn at(&self, x: usize, y: usize) -> i32 {
        self.data[y * self.width + x]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Kind {
    #[default]
    I4x4,
    I8x8,
    I16x16,
}

/// What later macroblocks need to know about a coded one.
#[derive(Clone, Copy, Default)]
struct MbInfo {
    kind: Kind,
    /// Intra4x4PredMode per luma4x4BlkIdx; for Intra8x8 each 8x8 mode is
    /// repeated over its four blocks.
    modes: [u8; 16],
    chroma_mode: u8,
    cbp_luma: u8,
    cbp_chroma: u8,
    /// Per colour plane: Intra16x16 DC (luma-like planes) or chroma DC.
    cbf_dc: [bool; 3],
    /// Per colour plane and luma4x4BlkIdx (luma-like; 8x8 transform blocks
    /// repeat their flag) or chroma4x4BlkIdx (4:2:0 / 4:2:2 chroma AC).
    cbf: [[bool; 16]; 3],
}

impl MbInfo {
    fn t8(&self) -> bool {
        self.kind == Kind::I8x8
    }
}

/// luma4x4BlkIdx of the 4x4 block at (x, y) (samples, within the MB).
fn blk4(x: usize, y: usize) -> usize {
    8 * (y / 8) + 4 * (x / 8) + 2 * ((y % 8) / 4) + (x % 8) / 4
}

/// Position of luma4x4BlkIdx `b` (6.4.3).
fn blk4_pos(b: usize) -> (usize, usize) {
    (
        8 * ((b / 4) % 2) + 4 * (b % 2),
        8 * (b / 8) + 4 * ((b % 4) / 2),
    )
}

/// Levels of a coded macroblock, in syntax order where it matters.
#[derive(Clone)]
struct MbCode {
    kind: Kind,
    /// Intra4x4 (16) or Intra8x8 (first 4) prediction modes.
    modes: [u8; 16],
    i16_mode: u8,
    chroma_mode: u8,
    cbp_luma: u8,
    cbp_chroma: u8,
    /// Per plane: Intra16x16 DC levels (16, scan order) or chroma DC (4/8).
    dc: [[i32; 16]; 3],
    /// Per plane, per 4x4 block (luma4x4BlkIdx or chroma4x4BlkIdx): 16
    /// levels in scan order (position 0 unused for AC-only blocks).
    ac: Box<[[[i32; 16]; 16]; 3]>,
    /// Per plane, per 8x8 block: 64 levels in scan order.
    ac8: Box<[[[i32; 64]; 4]; 3]>,
}

impl MbCode {
    fn new() -> Self {
        MbCode {
            kind: Kind::I4x4,
            modes: [0; 16],
            i16_mode: 0,
            chroma_mode: 0,
            cbp_luma: 0,
            cbp_chroma: 0,
            dc: [[0; 16]; 3],
            ac: Box::new([[[0; 16]; 16]; 3]),
            ac8: Box::new([[[0; 64]; 4]; 3]),
        }
    }
}

// ---------------------------------------------------------------------------
// Encoder state

struct Enc<'a> {
    chroma: u8,
    bit_depth: u8,
    lossless: bool,
    transform_8x8: bool,
    /// QP'Y and QP'C.
    qp_luma: i32,
    qp_chroma: i32,
    steps_luma: Steps,
    steps_chroma: Steps,
    lambda: f64,
    mbw: usize,
    mbh: usize,
    src: [Plane; 3],
    rec: [Plane; 3],
    info: Vec<MbInfo>,
    contexts: Contexts,
    _p: std::marker::PhantomData<&'a ()>,
}

/// 8.5.8 QPC as a function of qPI.
fn chroma_qp(qpi: i32) -> i32 {
    const TABLE: [i32; 22] = [
        29, 30, 31, 32, 32, 33, 34, 34, 35, 35, 36, 36, 37, 37, 37, 38, 38, 38, 39, 39, 39, 39,
    ];
    if qpi < 30 {
        qpi
    } else {
        TABLE[(qpi - 30) as usize]
    }
}

impl Enc<'_> {
    /// Planes coded like luma: 1, or 3 in 4:4:4.
    fn luma_planes(&self) -> usize {
        if self.chroma == 3 { 3 } else { 1 }
    }
    /// MbWidthC, MbHeightC for 4:2:0 / 4:2:2.
    fn chroma_size(&self) -> (usize, usize) {
        (8, if self.chroma == 2 { 16 } else { 8 })
    }
    fn max(&self) -> i32 {
        (1 << self.bit_depth) - 1
    }
    fn neighbour(&self, mx: usize, my: usize, dx: isize, dy: isize) -> Option<&MbInfo> {
        let (x, y) = (mx as isize + dx, my as isize + dy);
        if x < 0 || y < 0 || x >= self.mbw as isize || y >= self.mbh as isize {
            return None;
        }
        // Only already coded macroblocks (raster order, one slice).
        let idx = y as usize * self.mbw + x as usize;
        (idx < my * self.mbw + mx).then(|| &self.info[idx])
    }

    /// Edge samples of an n x n luma-like block at (bx, by) within MB
    /// (mx, my), for a block of the given decoding position.
    #[allow(clippy::too_many_arguments)]
    fn edge_nxn(&self, plane: usize, mx: usize, my: usize, bx: usize, by: usize, n: usize) -> Edge {
        let rec = &self.rec[plane];
        let (x0, y0) = (mx * 16 + bx, my * 16 + by);
        let has_left = bx > 0 || mx > 0;
        let has_top = by > 0 || my > 0;
        let has_corner = match (bx > 0, by > 0) {
            (true, true) => true,
            (false, true) => mx > 0,
            (true, false) => my > 0,
            (false, false) => mx > 0 && my > 0,
        };
        let has_right = if by == 0 {
            if bx + n < 16 {
                my > 0
            } else {
                my > 0 && mx + 1 < self.mbw
            }
        } else if bx + n >= 16 {
            false
        } else if n == 4 {
            blk4(bx + 4, by - 1) < blk4(bx, by)
        } else {
            // Intra8x8: block 2's top-right is block 1; block 3's is to the right.
            by == 8 && bx == 0
        };
        let mut e = Edge {
            top: [0; 32],
            left: [0; 16],
            corner: 0,
            has_top,
            has_left,
            has_corner,
        };
        if has_top {
            for i in 0..n {
                e.top[i] = rec.at(x0 + i, y0 - 1);
            }
            for i in n..2 * n {
                e.top[i] = if has_right {
                    rec.at(x0 + i, y0 - 1)
                } else {
                    e.top[n - 1]
                };
            }
        }
        if has_left {
            for i in 0..n {
                e.left[i] = rec.at(x0 - 1, y0 + i);
            }
        }
        if has_corner {
            e.corner = rec.at(x0 - 1, y0 - 1);
        }
        e
    }

    /// Edge of a whole-MB prediction (16x16, or chroma w x h) of `plane`.
    fn edge_mb(&self, plane: usize, mx: usize, my: usize, w: usize, h: usize) -> Edge {
        let rec = &self.rec[plane];
        let (x0, y0) = (mx * w, my * h);
        let mut e = Edge {
            top: [0; 32],
            left: [0; 16],
            corner: 0,
            has_top: my > 0,
            has_left: mx > 0,
            has_corner: mx > 0 && my > 0,
        };
        if e.has_top {
            for i in 0..w {
                e.top[i] = rec.at(x0 + i, y0 - 1);
            }
        }
        if e.has_left {
            for i in 0..h {
                e.left[i] = rec.at(x0 - 1, y0 + i);
            }
        }
        if e.has_corner {
            e.corner = rec.at(x0 - 1, y0 - 1);
        }
        e
    }

    /// predIntraNxNPredMode (8.3.1.1 / 8.3.2.1) for the block at (bx, by),
    /// given the modes chosen so far in this MB.
    fn predicted_mode(
        &self,
        mx: usize,
        my: usize,
        bx: usize,
        by: usize,
        eight: bool,
        cur: &[u8; 16],
    ) -> u8 {
        let mode_at = |x: isize, y: isize| -> Option<u8> {
            // None: neighbour unavailable (dcPredModePredictedFlag).
            let (dx, dy) = (x.div_euclid(16), y.div_euclid(16));
            let (rx, ry) = (x.rem_euclid(16) as usize, y.rem_euclid(16) as usize);
            if dx == 0 && dy == 0 {
                return Some(cur[blk4(rx, ry)]);
            }
            let n = self.neighbour(mx, my, dx, dy)?;
            Some(match n.kind {
                Kind::I16x16 => 2,
                Kind::I8x8 => n.modes[blk4(rx, ry)],
                Kind::I4x4 => {
                    if eight {
                        // Intra4x4PredMode[luma8x8BlkIdxN * 4 + n], n = 1 for A, 2 for B.
                        let b8 = blk4(rx, ry) / 4;
                        n.modes[b8 * 4 + if dx != 0 { 1 } else { 2 }]
                    } else {
                        n.modes[blk4(rx, ry)]
                    }
                }
            })
        };
        let a = mode_at(bx as isize - 1, by as isize);
        let b = mode_at(bx as isize, by as isize - 1);
        match (a, b) {
            (Some(a), Some(b)) => a.min(b),
            _ => 2,
        }
    }
}

// ---------------------------------------------------------------------------
// Syntax writing

const CBF_CAT: [usize; 14] = [0, 4, 8, 12, 16, 0, 0, 4, 8, 4, 0, 4, 8, 8];
const MAP_CAT: [usize; 14] = [0, 15, 29, 44, 47, 0, 0, 15, 29, 0, 0, 15, 29, 0];
const ABS_CAT: [usize; 14] = [0, 10, 20, 30, 39, 0, 0, 10, 20, 0, 0, 10, 20, 0];

fn cbf_offset(cat: usize) -> usize {
    match cat {
        0..=4 => 85,
        6..=8 => 460,
        10..=12 => 472,
        _ => 1012,
    }
}
fn sig_offset(cat: usize) -> usize {
    match cat {
        0..=4 => 105,
        5 => 402,
        6..=8 => 484,
        9 => 660,
        10..=12 => 528,
        _ => 718,
    }
}
fn last_offset(cat: usize) -> usize {
    match cat {
        0..=4 => 166,
        5 => 417,
        6..=8 => 572,
        9 => 690,
        10..=12 => 616,
        _ => 748,
    }
}
fn abs_offset(cat: usize) -> usize {
    match cat {
        0..=4 => 227,
        5 => 426,
        6..=8 => 952,
        9 => 708,
        10..=12 => 982,
        _ => 766,
    }
}

/// ctxBlockCat of a luma-like plane: base category (0 DC, 1 AC, 2 4x4,
/// 5 8x8) mapped to Cb (6-9) or Cr (10-13).
fn cat_for(plane: usize, base: usize) -> usize {
    match (plane, base) {
        (0, b) => b,
        (p, 5) => 5 + 4 * p,
        (p, b) => b + 6 + 4 * (p - 1),
    }
}

/// residual_block_cabac for `levels` (scan order, maxNumCoeff entries).
fn write_block<S: Sink>(
    s: &mut S,
    cat: usize,
    levels: &[i32],
    cbf_inc: Option<usize>,
    num_c8x8: usize,
) {
    let coded = levels.iter().any(|&v| v != 0);
    if let Some(inc) = cbf_inc {
        s.decision(cbf_offset(cat) + CBF_CAT[cat] + inc, coded);
    }
    if !coded {
        return;
    }
    let n = levels.len();
    let last = levels.iter().rposition(|&v| v != 0).unwrap();
    let eight = matches!(cat, 5 | 9 | 13);
    let inc = |i: usize, last_flag: bool| -> usize {
        if cat == 3 {
            (i / num_c8x8).min(2)
        } else if eight {
            usize::from(if last_flag {
                rusty_h264_common::cabac_tables::LAST8X8[i]
            } else {
                rusty_h264_common::cabac_tables::SIG8X8[i]
            })
        } else {
            i
        }
    };
    for (i, &v) in levels.iter().enumerate().take(n - 1) {
        let sig = v != 0;
        s.decision(sig_offset(cat) + MAP_CAT[cat] + inc(i, false), sig);
        if sig {
            s.decision(last_offset(cat) + MAP_CAT[cat] + inc(i, true), i == last);
            if i == last {
                break;
            }
        }
    }
    let base = abs_offset(cat) + ABS_CAT[cat];
    let (mut gt1, mut eq1) = (0usize, 0usize);
    let max_gt1 = if cat == 3 { 3 } else { 4 };
    for &v in levels[..=last].iter().rev() {
        if v == 0 {
            continue;
        }
        let a = v.unsigned_abs() - 1;
        let first = if gt1 != 0 { 0 } else { (1 + eq1).min(4) };
        s.decision(base + first, a > 0);
        if a > 0 {
            let ctx = base + 5 + gt1.min(max_gt1);
            let prefix = a.min(14);
            for _ in 1..prefix {
                s.decision(ctx, true);
            }
            if prefix < 14 {
                s.decision(ctx, false);
            } else {
                // UEG0 suffix.
                let mut v = a - 14;
                let mut k = 0;
                while v >= (1 << k) {
                    s.bypass(true);
                    v -= 1 << k;
                    k += 1;
                }
                s.bypass(false);
                for b in (0..k).rev() {
                    s.bypass(v >> b & 1 != 0);
                }
            }
            gt1 += 1;
        } else {
            eq1 += 1;
        }
        s.bypass(v < 0);
    }
}

/// The MbInfo a coded macroblock leaves behind.
fn info_of(e: &Enc, code: &MbCode) -> MbInfo {
    let mut info = MbInfo {
        kind: code.kind,
        chroma_mode: code.chroma_mode,
        cbp_luma: code.cbp_luma,
        cbp_chroma: code.cbp_chroma,
        ..Default::default()
    };
    match code.kind {
        Kind::I4x4 => info.modes = code.modes,
        Kind::I8x8 => {
            for b in 0..16 {
                info.modes[b] = code.modes[b / 4];
            }
        }
        Kind::I16x16 => info.modes = [2; 16],
    }
    let nz = |v: &[i32]| v.iter().any(|&x| x != 0);
    for p in 0..e.luma_planes() {
        info.cbf_dc[p] = code.kind == Kind::I16x16 && nz(&code.dc[p]);
        for b in 0..16 {
            if code.cbp_luma >> (b / 4) & 1 == 0 {
                continue;
            }
            info.cbf[p][b] = match code.kind {
                // Not coded outside 4:4:4: inferred to be 1.
                Kind::I8x8 => e.chroma != 3 || nz(&code.ac8[p][b / 4]),
                Kind::I16x16 => nz(&code.ac[p][b][1..]),
                Kind::I4x4 => nz(&code.ac[p][b]),
            };
        }
    }
    if matches!(e.chroma, 1 | 2) {
        let blocks = if e.chroma == 2 { 8 } else { 4 };
        for p in 1..3 {
            info.cbf_dc[p] = code.cbp_chroma != 0 && nz(&code.dc[p][..blocks]);
            for b in 0..blocks {
                info.cbf[p][b] = code.cbp_chroma == 2 && nz(&code.ac[p][b][1..]);
            }
        }
    }
    info
}

/// Writes one macroblock (everything but end_of_slice_flag).
fn write_mb<S: Sink>(s: &mut S, e: &Enc, mx: usize, my: usize, code: &MbCode) {
    let cur = info_of(e, code);
    let left = e.neighbour(mx, my, -1, 0);
    let top = e.neighbour(mx, my, 0, -1);
    // mb_type (I slice).
    let cond = |n: Option<&MbInfo>| usize::from(n.is_some_and(|n| n.kind == Kind::I16x16));
    let nxn = code.kind != Kind::I16x16;
    s.decision(3 + cond(left) + cond(top), !nxn);
    if !nxn {
        s.terminate(false);
        s.decision(3 + 3, code.cbp_luma != 0);
        s.decision(3 + 4, code.cbp_chroma != 0);
        if code.cbp_chroma != 0 {
            s.decision(3 + 5, code.cbp_chroma == 2);
        }
        s.decision(3 + 6, code.i16_mode >> 1 != 0);
        s.decision(3 + 7, code.i16_mode & 1 != 0);
    }
    if nxn {
        if e.transform_8x8 {
            let c = |n: Option<&MbInfo>| usize::from(n.is_some_and(|n| n.t8()));
            s.decision(399 + c(left) + c(top), code.kind == Kind::I8x8);
        }
        let eight = code.kind == Kind::I8x8;
        let mut modes = [0u8; 16];
        let count = if eight { 4 } else { 16 };
        for i in 0..count {
            let (bx, by) = if eight {
                (8 * (i % 2), 8 * (i / 2))
            } else {
                blk4_pos(i)
            };
            let pred = e.predicted_mode(mx, my, bx, by, eight, &modes);
            let mode = code.modes[i];
            if eight {
                for b in 0..4 {
                    modes[i * 4 + b] = mode;
                }
            } else {
                modes[i] = mode;
            }
            s.decision(68, mode == pred);
            if mode != pred {
                let rem = if mode < pred { mode } else { mode - 1 };
                for b in 0..3 {
                    s.decision(69, rem >> b & 1 != 0);
                }
            }
        }
    }
    if matches!(e.chroma, 1 | 2) {
        let c = |n: Option<&MbInfo>| usize::from(n.is_some_and(|n| n.chroma_mode != 0));
        let m = code.chroma_mode;
        s.decision(64 + c(left) + c(top), m > 0);
        if m > 0 {
            s.decision(64 + 3, m > 1);
            if m > 1 {
                s.decision(64 + 3, m > 2);
            }
        }
    }
    if nxn {
        for b8 in 0..4usize {
            let (x, y) = (b8 % 2, b8 / 2);
            // condTermFlagN = 0 when unavailable or its bit is set.
            let bit = |n: Option<&MbInfo>, b: usize| {
                usize::from(n.is_some_and(|n| n.cbp_luma >> b & 1 == 0))
            };
            let a = if x == 1 {
                usize::from(code.cbp_luma >> (b8 - 1) & 1 == 0)
            } else {
                bit(left, b8 + 1)
            };
            let b = if y == 1 {
                usize::from(code.cbp_luma >> (b8 - 2) & 1 == 0)
            } else {
                bit(top, b8 + 2)
            };
            s.decision(73 + a + 2 * b, code.cbp_luma >> b8 & 1 != 0);
        }
        if matches!(e.chroma, 1 | 2) {
            let c = |n: Option<&MbInfo>, two: bool| {
                usize::from(n.is_some_and(|n| {
                    if two {
                        n.cbp_chroma == 2
                    } else {
                        n.cbp_chroma != 0
                    }
                }))
            };
            s.decision(
                77 + c(left, false) + 2 * c(top, false),
                code.cbp_chroma != 0,
            );
            if code.cbp_chroma != 0 {
                s.decision(
                    77 + 4 + c(left, true) + 2 * c(top, true),
                    code.cbp_chroma == 2,
                );
            }
        }
    }
    if !nxn || code.cbp_luma != 0 || code.cbp_chroma != 0 {
        // mb_qp_delta = 0; the previous MB never has a nonzero delta.
        s.decision(60, false);
    }
    // Residual.
    let num_c8x8 = if e.chroma == 2 { 2 } else { 1 };
    for p in 0..e.luma_planes() {
        let cbf_luma = |bx: isize, by: isize, cat_base: usize| -> usize {
            let (dx, dy) = (bx.div_euclid(16), by.div_euclid(16));
            let (rx, ry) = (bx.rem_euclid(16) as usize, by.rem_euclid(16) as usize);
            let n = if dx == 0 && dy == 0 {
                Some(&cur)
            } else {
                e.neighbour(mx, my, dx, dy)
            };
            let Some(n) = n else { return 1 };
            let b = blk4(rx, ry);
            match cat_base {
                0 => usize::from(n.kind == Kind::I16x16 && n.cbf_dc[p]),
                5 => usize::from(n.t8() && n.cbp_luma >> (b / 4) & 1 != 0 && n.cbf[p][b]),
                _ => usize::from(n.cbp_luma >> (b / 4) & 1 != 0 && n.cbf[p][b]),
            }
        };
        match code.kind {
            Kind::I16x16 => {
                let inc = cbf_luma(-1, 0, 0) + 2 * cbf_luma(0, -1, 0);
                write_block(s, cat_for(p, 0), &code.dc[p], Some(inc), num_c8x8);
                if code.cbp_luma != 0 {
                    for b in 0..16 {
                        let (bx, by) = blk4_pos(b);
                        let (bx, by) = (bx as isize, by as isize);
                        let inc = cbf_luma(bx - 1, by, 1) + 2 * cbf_luma(bx, by - 1, 1);
                        write_block(s, cat_for(p, 1), &code.ac[p][b][1..], Some(inc), num_c8x8);
                    }
                }
            }
            Kind::I4x4 => {
                for b in 0..16 {
                    if code.cbp_luma >> (b / 4) & 1 == 0 {
                        continue;
                    }
                    let (bx, by) = blk4_pos(b);
                    let (bx, by) = (bx as isize, by as isize);
                    let inc = cbf_luma(bx - 1, by, 2) + 2 * cbf_luma(bx, by - 1, 2);
                    write_block(s, cat_for(p, 2), &code.ac[p][b], Some(inc), num_c8x8);
                }
            }
            Kind::I8x8 => {
                for b8 in 0..4 {
                    if code.cbp_luma >> b8 & 1 == 0 {
                        continue;
                    }
                    let (bx, by) = (8 * (b8 % 2) as isize, 8 * (b8 / 2) as isize);
                    let inc = (e.chroma == 3)
                        .then(|| cbf_luma(bx - 1, by, 5) + 2 * cbf_luma(bx, by - 1, 5));
                    write_block(s, cat_for(p, 5), &code.ac8[p][b8], inc, num_c8x8);
                }
            }
        }
    }
    if matches!(e.chroma, 1 | 2) && code.cbp_chroma != 0 {
        let (cw, ch) = e.chroma_size();
        let blocks = cw * ch / 16;
        for p in 1..3 {
            let c = |n: Option<&MbInfo>| match n {
                None => 1,
                Some(n) => usize::from(n.cbp_chroma != 0 && n.cbf_dc[p]),
            };
            let inc = c(left) + 2 * c(top);
            write_block(s, 3, &code.dc[p][..blocks], Some(inc), num_c8x8);
        }
        if code.cbp_chroma == 2 {
            for p in 1..3 {
                for b in 0..blocks {
                    let (bx, by) = (4 * (b % 2) as isize, 4 * (b / 2) as isize);
                    let flag = |x: isize, y: isize| -> usize {
                        let (dx, dy) = (x.div_euclid(cw as isize), y.div_euclid(ch as isize));
                        let (rx, ry) = (
                            x.rem_euclid(cw as isize) as usize,
                            y.rem_euclid(ch as isize) as usize,
                        );
                        let n = if dx == 0 && dy == 0 {
                            Some(&cur)
                        } else {
                            e.neighbour(mx, my, dx, dy)
                        };
                        match n {
                            None => 1,
                            Some(n) => {
                                usize::from(n.cbp_chroma == 2 && n.cbf[p][(ry / 4) * 2 + rx / 4])
                            }
                        }
                    };
                    let inc = flag(bx - 1, by) + 2 * flag(bx, by - 1);
                    write_block(s, 4, &code.ac[p][b][1..], Some(inc), num_c8x8);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Residual coding of candidates

/// Quantizes one transform coefficient (deadzone quantizer, intra rounding).
#[inline]
fn quant(w: i32, step: f64) -> i32 {
    let l = (f64::from(w.abs()) / step + 1.0 / 3.0).floor() as i32;
    if w < 0 { -l } else { l }
}

/// 8.5.15: lossless Intra prediction with vertical (`v`) or horizontal
/// residual DPCM: the levels the encoder codes for residual `r`.
fn dpcm(r: &mut [i32], w: usize, h: usize, vertical: bool) {
    if vertical {
        for y in (1..h).rev() {
            for x in 0..w {
                r[y * w + x] -= r[(y - 1) * w + x];
            }
        }
    } else {
        for y in 0..h {
            for x in (1..w).rev() {
                r[y * w + x] -= r[y * w + x - 1];
            }
        }
    }
}

struct Coded4 {
    levels: [i32; 16],
    rec: [i32; 16],
}

impl Enc<'_> {
    fn steps(&self, plane: usize) -> &Steps {
        if plane == 0 {
            &self.steps_luma
        } else {
            &self.steps_chroma
        }
    }
    fn qp_of(&self, plane: usize) -> i32 {
        if plane == 0 {
            self.qp_luma
        } else {
            self.qp_chroma
        }
    }

    /// A 4x4 block with its own DC: prediction `pred`, source `src`.
    /// `bypass_dpcm`: lossless DPCM direction (Some(vertical)).
    fn code4(
        &self,
        plane: usize,
        src: &[i32; 16],
        pred: &[i32; 16],
        dpcm_dir: Option<bool>,
    ) -> Coded4 {
        let mut r: [i32; 16] = std::array::from_fn(|k| src[k] - pred[k]);
        if self.lossless {
            let orig = r;
            if let Some(v) = dpcm_dir {
                dpcm(&mut r, 4, 4, v);
            }
            return Coded4 {
                levels: std::array::from_fn(|i| r[ZIGZAG4[i]]),
                rec: std::array::from_fn(|k| pred[k] + orig[k]),
            };
        }
        let w = transform::forward4(&r);
        let steps = self.steps(plane);
        let mut c = [0i32; 16];
        let mut levels = [0i32; 16];
        for i in 0..16 {
            let k = ZIGZAG4[i];
            c[k] = quant(w[k], steps.ac4[k]);
            levels[i] = c[k];
        }
        let res = if levels.iter().any(|&v| v != 0) {
            transform::inverse4(&transform::scale4(&c, self.qp_of(plane), None))
        } else {
            [0; 16]
        };
        let max = self.max();
        Coded4 {
            levels,
            rec: std::array::from_fn(|k| (pred[k] + res[k]).clamp(0, max)),
        }
    }

    fn code8(
        &self,
        plane: usize,
        src: &[i32; 64],
        pred: &[i32; 64],
        dpcm_dir: Option<bool>,
    ) -> ([i32; 64], [i32; 64]) {
        let mut r: [i32; 64] = std::array::from_fn(|k| src[k] - pred[k]);
        if self.lossless {
            let orig = r;
            if let Some(v) = dpcm_dir {
                dpcm(&mut r, 8, 8, v);
            }
            return (
                std::array::from_fn(|i| r[ZIGZAG8[i]]),
                std::array::from_fn(|k| pred[k] + orig[k]),
            );
        }
        let w = transform::forward8(&r);
        let steps = self.steps(plane);
        let mut c = [0i32; 64];
        let mut levels = [0i32; 64];
        for i in 0..64 {
            let k = ZIGZAG8[i];
            c[k] = quant(w[k], steps.ac8[k]);
            levels[i] = c[k];
        }
        let res = if levels.iter().any(|&v| v != 0) {
            transform::inverse8(&transform::scale8(&c, self.qp_of(plane)))
        } else {
            [0; 64]
        };
        let max = self.max();
        (
            levels,
            std::array::from_fn(|k| (pred[k] + res[k]).clamp(0, max)),
        )
    }

    /// A block of 4x4 transform blocks with a separately transformed DC
    /// (Intra16x16: 16x16, `rows` = 4; chroma: w x h, 2x2 or 4x2 DC).
    /// Returns (DC levels in syntax order, AC levels per block, recon).
    #[allow(clippy::too_many_arguments)]
    fn code_dc_blocks(
        &self,
        plane: usize,
        w: usize,
        h: usize,
        src: &[i32],
        pred: &[i32],
        dpcm_dir: Option<bool>,
        luma: bool,
    ) -> (Vec<i32>, Vec<[i32; 16]>, Vec<i32>) {
        let (bw, bh) = (w / 4, h / 4);
        let nblocks = bw * bh;
        // Block order: luma4x4BlkIdx for 16x16, raster for chroma.
        let block_pos = |b: usize| {
            if luma {
                blk4_pos(b)
            } else {
                (4 * (b % 2), 4 * (b / 2))
            }
        };
        let mut r: Vec<i32> = (0..w * h).map(|k| src[k] - pred[k]).collect();
        let orig = r.clone();
        if let (true, Some(v)) = (self.lossless, dpcm_dir) {
            dpcm(&mut r, w, h, v);
        }
        let block_of = |r: &[i32], b: usize| -> [i32; 16] {
            let (x0, y0) = block_pos(b);
            std::array::from_fn(|k| r[(y0 + k / 4) * w + x0 + k % 4])
        };
        // DC matrix (rows x cols of blocks) in raster, and the syntax order.
        let dc_order: Vec<(usize, usize)> = if luma {
            (0..16).map(|i| (ZIGZAG4[i] / 4, ZIGZAG4[i] % 4)).collect()
        } else if bh == 2 {
            vec![(0, 0), (0, 1), (1, 0), (1, 1)]
        } else {
            vec![
                (0, 0),
                (1, 0),
                (0, 1),
                (2, 0),
                (3, 0),
                (1, 1),
                (2, 1),
                (3, 1),
            ]
        };
        let raster_block = |row: usize, col: usize| -> usize {
            if luma {
                blk4(col * 4, row * 4)
            } else {
                row * 2 + col
            }
        };
        let max = self.max();
        if self.lossless {
            let mut ac = vec![[0; 16]; nblocks];
            let mut dc = vec![0; nblocks];
            for b in 0..nblocks {
                let blk = block_of(&r, b);
                for i in 0..16 {
                    ac[b][i] = blk[ZIGZAG4[i]];
                }
            }
            for (i, &(row, col)) in dc_order.iter().enumerate() {
                dc[i] = ac[raster_block(row, col)][0];
            }
            let rec = (0..w * h).map(|k| pred[k] + orig[k]).collect();
            return (dc, ac, rec);
        }
        let steps = self.steps(plane);
        let qp = self.qp_of(plane);
        let mut coeffs: Vec<[i32; 16]> = (0..nblocks)
            .map(|b| transform::forward4(&block_of(&r, b)))
            .collect();
        let dc_raster: Vec<i32> = (0..bh * bw)
            .map(|k| coeffs[raster_block(k / bw, k % bw)][0])
            .collect();
        let f = transform::forward_dc(&dc_raster, bh, bw);
        let dc_step = if luma { steps.dc_luma } else { steps.dc_chroma };
        let dc_levels_raster: Vec<i32> = f.iter().map(|&v| quant(v, dc_step)).collect();
        let dc: Vec<i32> = dc_order
            .iter()
            .map(|&(row, col)| dc_levels_raster[row * bw + col])
            .collect();
        let dc_values: Vec<i32> = if luma {
            transform::luma_dc(&dc_levels_raster.clone().try_into().unwrap(), qp).to_vec()
        } else {
            transform::chroma_dc(&dc_levels_raster, qp, bh)
        };
        let mut ac = vec![[0; 16]; nblocks];
        let mut rec = vec![0; w * h];
        for b in 0..nblocks {
            let mut c = [0i32; 16];
            for i in 1..16 {
                let k = ZIGZAG4[i];
                c[k] = quant(coeffs[b][k], steps.ac4[k]);
                ac[b][i] = c[k];
            }
            coeffs[b] = c;
        }
        for row in 0..bh {
            for col in 0..bw {
                let b = raster_block(row, col);
                let dcv = dc_values[row * bw + col];
                let res = transform::inverse4(&transform::scale4(&coeffs[b], qp, Some(dcv)));
                let (x0, y0) = block_pos(b);
                for k in 0..16 {
                    let at = (y0 + k / 4) * w + x0 + k % 4;
                    rec[at] = (pred[at] + res[k]).clamp(0, max);
                }
            }
        }
        (dc, ac, rec)
    }
}

fn sse(a: &[i32], b: &[i32]) -> u64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| ((x - y) * (x - y)) as u64)
        .sum()
}

/// Sum of absolute Hadamard-transformed differences of a 4x4 block.
fn satd4(d: &[i32; 16]) -> i32 {
    let mut t = [0i32; 16];
    for i in 0..4 {
        let r = &d[i * 4..i * 4 + 4];
        let (a, b, c, e) = (r[0] + r[3], r[1] + r[2], r[1] - r[2], r[0] - r[3]);
        t[i * 4] = a + b;
        t[i * 4 + 1] = e + c;
        t[i * 4 + 2] = a - b;
        t[i * 4 + 3] = e - c;
    }
    let mut s = 0;
    for j in 0..4 {
        let c = |i: usize| t[i * 4 + j];
        let (a, b, cc, e) = (c(0) + c(3), c(1) + c(2), c(1) - c(2), c(0) - c(3));
        s += (a + b).abs() + (e + cc).abs() + (a - b).abs() + (e - cc).abs();
    }
    s / 2
}

fn satd(src: &[i32], pred: &[i32], w: usize, h: usize) -> i32 {
    let mut total = 0;
    for by in (0..h).step_by(4) {
        for bx in (0..w).step_by(4) {
            let d = std::array::from_fn(|k| {
                let at = (by + k / 4) * w + bx + k % 4;
                src[at] - pred[at]
            });
            total += satd4(&d);
        }
    }
    total
}

/// The chosen chroma prediction and its coded blocks (4:2:0 / 4:2:2).
struct ChromaResult {
    mode: u8,
    cbp: u8,
    rec: [Vec<i32>; 2],
    dc: [Vec<i32>; 2],
    ac: [Vec<[i32; 16]>; 2],
}

impl ChromaResult {
    fn apply(&self, code: &mut MbCode) {
        code.chroma_mode = self.mode;
        code.cbp_chroma = self.cbp;
        for c in 0..2 {
            code.dc[c + 1][..self.dc[c].len()].copy_from_slice(&self.dc[c]);
            for (b, ac) in self.ac[c].iter().enumerate() {
                code.ac[c + 1][b] = *ac;
            }
        }
    }
}

/// A coded candidate: levels and the reconstruction of every plane's MB.
struct Candidate {
    code: MbCode,
    rec: [Vec<i32>; 3],
    distortion: u64,
}

impl Enc<'_> {
    fn block(&self, plane: &Plane, x0: usize, y0: usize, w: usize, h: usize) -> Vec<i32> {
        let mut v = Vec::with_capacity(w * h);
        for y in 0..h {
            v.extend_from_slice(&plane.data[(y0 + y) * plane.width + x0..][..w]);
        }
        v
    }
    fn put(&mut self, plane: usize, x0: usize, y0: usize, w: usize, data: &[i32]) {
        let p = &mut self.rec[plane];
        for (y, row) in data.chunks(w).enumerate() {
            p.data[(y0 + y) * p.width + x0..][..w].copy_from_slice(row);
        }
    }

    /// Rate in 1/256 bits of the whole macroblock with the current contexts.
    fn rate(&self, mx: usize, my: usize, code: &MbCode) -> u32 {
        let mut c = Counter {
            contexts: self.contexts.clone(),
            bits: 0,
        };
        write_mb(&mut c, self, mx, my, code);
        c.bits
    }

    fn cost(&self, mx: usize, my: usize, cand: &Candidate) -> f64 {
        cand.distortion as f64 + self.lambda * f64::from(self.rate(mx, my, &cand.code)) / 256.0
    }

    /// Chroma (4:2:0 / 4:2:2) mode decision and coding.
    fn code_chroma(&mut self, mx: usize, my: usize) -> ChromaResult {
        let (cw, ch) = self.chroma_size();
        let edges = [
            self.edge_mb(1, mx, my, cw, ch),
            self.edge_mb(2, mx, my, cw, ch),
        ];
        let src = [
            self.block(&self.src[1], mx * cw, my * ch, cw, ch),
            self.block(&self.src[2], mx * cw, my * ch, cw, ch),
        ];
        let mut best: Option<(f64, ChromaResult)> = None;
        for mode in 0..4u8 {
            if !intra::chroma_mode_available(mode, &edges[0]) {
                continue;
            }
            let dir = match mode {
                1 => Some(false),
                2 => Some(true),
                _ => None,
            };
            let mut result = ChromaResult {
                mode,
                cbp: 0,
                rec: [vec![], vec![]],
                dc: [vec![], vec![]],
                ac: [vec![], vec![]],
            };
            let mut dist = 0;
            for c in 0..2 {
                let mut pred = vec![0; cw * ch];
                intra::predict_chroma(mode, cw, ch, &edges[c], self.bit_depth, &mut pred);
                let (dc, ac, rec) = self.code_dc_blocks(c + 1, cw, ch, &src[c], &pred, dir, false);
                dist += sse(&src[c], &rec);
                result.dc[c] = dc;
                result.ac[c] = ac;
                result.rec[c] = rec;
            }
            let has_ac = result
                .ac
                .iter()
                .flatten()
                .any(|b| b[1..].iter().any(|&v| v != 0));
            let has_dc = result.dc.iter().flatten().any(|&v| v != 0);
            result.cbp = if has_ac { 2 } else { u8::from(has_dc) };
            // Rate: the chroma syntax of an otherwise empty Intra16x16 MB.
            let mut code = MbCode::new();
            code.kind = Kind::I16x16;
            result.apply(&mut code);
            let j = dist as f64 + self.lambda * f64::from(self.rate(mx, my, &code)) / 256.0;
            if best.as_ref().is_none_or(|b| j < b.0) {
                best = Some((j, result));
            }
        }
        best.unwrap().1
    }

    /// Intra16x16 candidate for every luma-like plane with the best mode.
    fn candidate16(&mut self, mx: usize, my: usize, base: &MbCode) -> Option<Candidate> {
        let planes = self.luma_planes();
        let src: Vec<Vec<i32>> = (0..planes)
            .map(|p| self.block(&self.src[p], mx * 16, my * 16, 16, 16))
            .collect();
        let edges: Vec<Edge> = (0..planes)
            .map(|p| self.edge_mb(p, mx, my, 16, 16))
            .collect();
        // Preselect by SATD, then code the two best.
        let mut scored = vec![];
        for mode in 0..4u8 {
            if !intra::i16_mode_available(mode, &edges[0]) {
                continue;
            }
            let mut s = 0;
            let mut preds = vec![];
            for p in 0..planes {
                let mut pred = [0i32; 256];
                intra::predict16(mode, &edges[p], self.bit_depth, &mut pred);
                s += satd(&src[p], &pred, 16, 16);
                preds.push(pred);
            }
            scored.push((s, mode, preds));
        }
        scored.sort_by_key(|s| s.0);
        let mut best: Option<(f64, Candidate)> = None;
        for (_, mode, preds) in scored.into_iter().take(if self.lossless { 4 } else { 2 }) {
            let dir = match mode {
                0 => Some(true),
                1 => Some(false),
                _ => None,
            };
            let mut code = base.clone();
            code.kind = Kind::I16x16;
            code.i16_mode = mode;
            let mut rec: [Vec<i32>; 3] = [vec![], vec![], vec![]];
            let mut dist = 0;
            let mut any_ac = false;
            for p in 0..planes {
                let (dc, ac, r) = self.code_dc_blocks(p, 16, 16, &src[p], &preds[p], dir, true);
                code.dc[p].copy_from_slice(&dc);
                for b in 0..16 {
                    code.ac[p][b] = ac[b];
                    any_ac |= ac[b][1..].iter().any(|&v| v != 0);
                }
                dist += sse(&src[p], &r);
                rec[p] = r;
            }
            code.cbp_luma = if any_ac { 15 } else { 0 };
            let cand = Candidate {
                code,
                rec,
                distortion: dist,
            };
            let j = self.cost(mx, my, &cand);
            if best.as_ref().is_none_or(|b| j < b.0) {
                best = Some((j, cand));
            }
        }
        best.map(|b| b.1)
    }

    /// Intra4x4 (n = 4) or Intra8x8 (n = 8) candidate: per-block decisions
    /// in decoding order, reconstructing into `rec` as it goes.
    fn candidate_nxn(&mut self, mx: usize, my: usize, n: usize, base: &MbCode) -> Candidate {
        let planes = self.luma_planes();
        let mut code = base.clone();
        code.kind = if n == 4 { Kind::I4x4 } else { Kind::I8x8 };
        let count = if n == 4 { 16 } else { 4 };
        let mut chosen = [0u8; 16];
        let mut dist = 0;
        let sad_lambda = self.lambda.sqrt();
        for i in 0..count {
            let (bx, by) = if n == 4 {
                blk4_pos(i)
            } else {
                (8 * (i % 2), 8 * (i / 2))
            };
            let pred_mode = self.predicted_mode(mx, my, bx, by, n == 8, &chosen);
            let (x0, y0) = (mx * 16 + bx, my * 16 + by);
            let src: Vec<Vec<i32>> = (0..planes)
                .map(|p| self.block(&self.src[p], x0, y0, n, n))
                .collect();
            let edges: Vec<Edge> = (0..planes)
                .map(|p| {
                    let e = self.edge_nxn(p, mx, my, bx, by, n);
                    if n == 8 { intra::filter8(&e) } else { e }
                })
                .collect();
            let mut scored = vec![];
            for mode in 0..9u8 {
                if !intra::nxn_mode_available(mode, &edges[0]) {
                    continue;
                }
                let mut s = 0.0;
                let mut preds = vec![];
                for p in 0..planes {
                    let mut pred = vec![0i32; n * n];
                    intra::predict_nxn(mode, n, &edges[p], self.bit_depth, &mut pred);
                    s += f64::from(satd(&src[p], &pred, n, n));
                    preds.push(pred);
                }
                let bits = if mode == pred_mode { 1.0 } else { 4.0 };
                scored.push((s + sad_lambda * bits, mode, preds));
            }
            scored.sort_by(|a, b| a.0.total_cmp(&b.0));
            let keep = if self.lossless { 9 } else { 3 };
            let mut best: Option<(f64, u8, Vec<Vec<i32>>, Vec<Vec<i32>>, u64)> = None;
            for (_, mode, preds) in scored.into_iter().take(keep) {
                let dir = match mode {
                    0 => Some(true),
                    1 => Some(false),
                    _ => None,
                };
                let mut levels = vec![];
                let mut recs = vec![];
                let mut d = 0;
                let mut c = Counter {
                    contexts: self.contexts.clone(),
                    bits: if mode == pred_mode { 256 } else { 4 * 256 },
                };
                for p in 0..planes {
                    let (lv, rec) = if n == 4 {
                        let coded = self.code4(
                            p,
                            &src[p][..].try_into().unwrap(),
                            &preds[p][..].try_into().unwrap(),
                            dir,
                        );
                        (coded.levels.to_vec(), coded.rec.to_vec())
                    } else {
                        let (lv, rec) = self.code8(
                            p,
                            &src[p][..].try_into().unwrap(),
                            &preds[p][..].try_into().unwrap(),
                            dir,
                        );
                        (lv.to_vec(), rec.to_vec())
                    };
                    let cat = cat_for(p, if n == 4 { 2 } else { 5 });
                    let cbf = (n == 4 || self.chroma == 3).then_some(0);
                    write_block(&mut c, cat, &lv, cbf, 1);
                    d += sse(&src[p], &rec);
                    levels.push(lv);
                    recs.push(rec);
                }
                let j = d as f64 + self.lambda * f64::from(c.bits) / 256.0;
                if best.as_ref().is_none_or(|b| j < b.0) {
                    best = Some((j, mode, levels, recs, d));
                }
            }
            let (_, mode, levels, recs, d) = best.unwrap();
            dist += d;
            code.modes[i] = mode;
            if n == 4 {
                chosen[i] = mode;
            } else {
                for b in 0..4 {
                    chosen[i * 4 + b] = mode;
                }
            }
            for p in 0..planes {
                if n == 4 {
                    code.ac[p][i].copy_from_slice(&levels[p]);
                } else {
                    code.ac8[p][i].copy_from_slice(&levels[p]);
                }
                self.put(p, x0, y0, n, &recs[p]);
            }
        }
        let mut cbp = 0u8;
        for b8 in 0..4 {
            let nz = (0..planes).any(|p| {
                if n == 4 {
                    (0..4).any(|k| code.ac[p][b8 * 4 + k].iter().any(|&v| v != 0))
                } else {
                    code.ac8[p][b8].iter().any(|&v| v != 0)
                }
            });
            if nz {
                cbp |= 1 << b8;
            }
        }
        code.cbp_luma = cbp;
        let rec: [Vec<i32>; 3] = std::array::from_fn(|p| {
            if p < planes {
                self.block(&self.rec[p], mx * 16, my * 16, 16, 16)
            } else {
                vec![]
            }
        });
        Candidate {
            code,
            rec,
            distortion: dist,
        }
    }

    fn encode_mb<S: Sink>(&mut self, sink: &mut S, mx: usize, my: usize) {
        let mut base = MbCode::new();
        let mut chroma_rec = None;
        if matches!(self.chroma, 1 | 2) {
            let result = self.code_chroma(mx, my);
            result.apply(&mut base);
            chroma_rec = Some(result.rec);
        }
        let mut cands = vec![];
        if let Some(c) = self.candidate16(mx, my, &base) {
            cands.push(c);
        }
        cands.push(self.candidate_nxn(mx, my, 4, &base));
        if self.transform_8x8 {
            cands.push(self.candidate_nxn(mx, my, 8, &base));
        }
        let best = cands
            .into_iter()
            .map(|c| (self.cost(mx, my, &c), c))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap()
            .1;
        for p in 0..self.luma_planes() {
            self.put(p, mx * 16, my * 16, 16, &best.rec[p]);
        }
        if let Some(recs) = chroma_rec {
            let (cw, ch) = self.chroma_size();
            for c in 0..2 {
                self.put(c + 1, mx * cw, my * ch, cw, &recs[c]);
            }
        }
        write_mb(sink, self, mx, my, &best.code);
        let info = info_of(self, &best.code);
        self.info[my * self.mbw + mx] = info;
    }
}

// ---------------------------------------------------------------------------
// Parameter sets and slice

fn nal(kind: u8, rbsp: &[u8]) -> Vec<u8> {
    let mut out = vec![0x60 | kind];
    out.extend(escape(rbsp));
    out
}

fn trailing(mut w: BitWriter) -> Vec<u8> {
    w.bit(true);
    while !w.bits.is_multiple_of(8) {
        w.bit(false);
    }
    w.bytes
}

/// Encodes one intra picture into SPS, PPS and IDR slice NAL units
/// (without start codes, with emulation prevention).
pub fn encode(picture: &Picture, settings: &Settings) -> Result<Vec<Vec<u8>>, String> {
    encode_with_reconstruction(picture, settings).map(|(units, _)| units)
}

/// `encode`, also returning the reconstructed planes before deblocking
/// (cropped to the picture), which equal the decoder's output when the
/// deblocking filter is off.
pub fn encode_with_reconstruction(
    picture: &Picture,
    settings: &Settings,
) -> Result<(Vec<Vec<u8>>, [Vec<u16>; 3]), String> {
    let chroma = picture.chroma;
    let bd = picture.bit_depth;
    if !(8..=10).contains(&bd) || chroma > 3 {
        return Err("unsupported format".into());
    }
    let (w, h) = (picture.width as usize, picture.height as usize);
    if w == 0 || h == 0 {
        return Err("empty picture".into());
    }
    let (sx, sy) = match chroma {
        1 => (2, 2),
        2 => (2, 1),
        _ => (1, 1),
    };
    let (mbw, mbh) = (w.div_ceil(16), h.div_ceil(16));
    let qp_offset = 6 * i32::from(bd - 8);
    let qp = if settings.lossless {
        -qp_offset
    } else {
        settings.qp.clamp(-qp_offset, 51)
    };
    let pad = |data: &[u16], pw: usize, ph: usize, fw: usize, fh: usize| -> Result<Plane, String> {
        if data.len() < pw * ph {
            return Err("plane too small".into());
        }
        let mut out = vec![0i32; fw * fh];
        for y in 0..fh {
            for x in 0..fw {
                out[y * fw + x] = i32::from(data[y.min(ph - 1) * pw + x.min(pw - 1)]);
            }
        }
        Ok(Plane {
            width: fw,
            height: fh,
            data: out,
        })
    };
    let (cw, ch) = (w.div_ceil(sx), h.div_ceil(sy));
    let (fcw, fch) = (mbw * 16 / sx, mbh * 16 / sy);
    let src = [
        pad(picture.planes[0], w, h, mbw * 16, mbh * 16)?,
        if chroma == 0 {
            Plane {
                width: 0,
                height: 0,
                data: vec![],
            }
        } else {
            pad(picture.planes[1], cw, ch, fcw, fch)?
        },
        if chroma == 0 {
            Plane {
                width: 0,
                height: 0,
                data: vec![],
            }
        } else {
            pad(picture.planes[2], cw, ch, fcw, fch)?
        },
    ];
    let rec = std::array::from_fn(|p| Plane {
        width: src[p].width,
        height: src[p].height,
        data: vec![0; src[p].data.len()],
    });
    let qp_luma = qp + qp_offset;
    let qp_chroma = chroma_qp((qp).clamp(-qp_offset, 51)) + qp_offset;
    let lambda = 0.85 * 2f64.powf(f64::from(qp - 12) / 3.0) * 4f64.powi(i32::from(bd - 8));
    let mut enc = Enc {
        chroma,
        bit_depth: bd,
        lossless: settings.lossless,
        transform_8x8: settings.transform_8x8,
        qp_luma,
        qp_chroma,
        steps_luma: Steps::new(qp_luma.max(0), 2),
        steps_chroma: Steps::new(qp_chroma.max(0), if chroma == 2 { 4 } else { 2 }),
        lambda: if settings.lossless { 1.0 } else { lambda },
        mbw,
        mbh,
        src,
        rec,
        info: vec![MbInfo::default(); mbw * mbh],
        contexts: Contexts::new(qp),
        _p: std::marker::PhantomData,
    };

    // SPS
    let profile_idc = profile(chroma, bd, settings.lossless);
    let (level_idc, _, mv_range) = level(picture.width, picture.height, 1);
    let mut s = BitWriter::default();
    s.bits(u32::from(profile_idc), 8);
    s.bits(0, 8); // constraint flags, as x264 writes them for these profiles
    s.bits(u32::from(level_idc), 8);
    s.ue(0); // seq_parameter_set_id
    s.ue(u32::from(chroma));
    if chroma == 3 {
        s.bit(false); // separate_colour_plane_flag
    }
    s.ue(u32::from(bd - 8));
    s.ue(u32::from(bd - 8));
    s.bit(settings.lossless); // qpprime_y_zero_transform_bypass_flag
    s.bit(false); // seq_scaling_matrix_present_flag
    s.ue(0); // log2_max_frame_num_minus4
    s.ue(2); // pic_order_cnt_type
    s.ue(1); // max_num_ref_frames
    s.bit(false); // gaps_in_frame_num_value_allowed_flag
    s.ue(mbw as u32 - 1);
    s.ue(mbh as u32 - 1);
    s.bit(true); // frame_mbs_only_flag
    s.bit(true); // direct_8x8_inference_flag
    let (unit_x, unit_y) = match chroma {
        1 => (2, 2),
        2 => (2, 1),
        _ => (1, 1),
    };
    let (crop_r, crop_b) = ((mbw * 16 - w) / unit_x, (mbh * 16 - h) / unit_y);
    let crop = crop_r != 0 || crop_b != 0;
    s.bit(crop);
    if crop {
        s.ue(0);
        s.ue(crop_r as u32);
        s.ue(0);
        s.ue(crop_b as u32);
    }
    s.bit(true); // vui_parameters_present_flag
    write_vui(&mut s, &settings.vui, mv_range);
    let sps = nal(7, &trailing(s));

    // PPS
    let mut p = BitWriter::default();
    p.ue(0);
    p.ue(0);
    p.bit(true); // entropy_coding_mode_flag
    p.bit(false); // bottom_field_pic_order_in_frame_present_flag
    p.ue(0); // num_slice_groups_minus1
    p.ue(0);
    p.ue(0);
    p.bit(false); // weighted_pred_flag
    p.bits(0, 2); // weighted_bipred_idc
    p.se(qp - 26); // pic_init_qp_minus26
    p.se(0); // pic_init_qs_minus26
    p.se(0); // chroma_qp_index_offset
    p.bit(true); // deblocking_filter_control_present_flag
    p.bit(false); // constrained_intra_pred_flag
    p.bit(false); // redundant_pic_cnt_present_flag
    p.bit(settings.transform_8x8);
    p.bit(false); // pic_scaling_matrix_present_flag
    p.se(0); // second_chroma_qp_index_offset
    let pps = nal(8, &trailing(p));

    // Slice header (IDR, I slice).
    let mut sh = BitWriter::default();
    sh.ue(0); // first_mb_in_slice
    sh.ue(7); // slice_type: I (all slices)
    sh.ue(0); // pic_parameter_set_id
    sh.bits(0, 4); // frame_num
    sh.ue(0); // idr_pic_id
    sh.bit(false); // no_output_of_prior_pics_flag
    sh.bit(false); // long_term_reference_flag
    sh.se(0); // slice_qp_delta
    sh.ue(if settings.deblocking { 0 } else { 1 }); // disable_deblocking_filter_idc
    if settings.deblocking {
        sh.se(0);
        sh.se(0);
    }
    while !sh.bits.is_multiple_of(8) {
        sh.bit(true); // cabac_alignment_one_bit
    }
    let bits = sh.bits as u32;
    let mut sink = Encoder::new(enc.contexts.clone(), sh.bytes, bits);
    for my in 0..mbh {
        for mx in 0..mbw {
            enc.contexts = sink.contexts.clone();
            enc.encode_mb(&mut sink, mx, my);
            sink.terminate(mx + 1 == mbw && my + 1 == mbh);
        }
    }
    let slice = nal(5, &sink.finish());
    let crop = |p: &Plane, pw: usize, ph: usize| -> Vec<u16> {
        let mut out = Vec::with_capacity(pw * ph);
        for y in 0..ph {
            out.extend(p.data[y * p.width..][..pw].iter().map(|&v| v as u16));
        }
        out
    };
    let recon = [
        crop(&enc.rec[0], w, h),
        if chroma == 0 {
            vec![]
        } else {
            crop(&enc.rec[1], cw, ch)
        },
        if chroma == 0 {
            vec![]
        } else {
            crop(&enc.rec[2], cw, ch)
        },
    ];
    Ok((vec![sps, pps, slice], recon))
}
