// SPDX-License-Identifier: LGPL-3.0-or-later
//! Adaptive loop filter decisions for the encoder: least-squares luma
//! filters per class (merged greedily), chroma filters per component, and
//! per-CTU choices among no filter, the signalled filters and the 16 fixed
//! filter sets, judged by running the decoder's own filter.
use super::alf;
use super::bits::{BitWriter, ceil_log2};
use super::pic::{AlfCtu, Picture, Plane};
use super::ps::{AlfParam, PicHeader, Pps, SliceHeader, Sps};

/// Luma taps (dx, dy) of each coefficient's sample pair (the other sample
/// is mirrored), in coefficient order.
const LUMA_TAPS: [(i32, i32); 12] = [
    (0, 3),
    (1, 2),
    (0, 2),
    (-1, 2),
    (2, 1),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-2, 1),
    (3, 0),
    (2, 0),
    (1, 0),
];
const CHROMA_TAPS: [(i32, i32); 6] = [(0, 2), (1, 1), (0, 1), (-1, 1), (2, 0), (1, 0)];

/// Normal equations of one filter: sum of d d^T, sum of d e and sum of e^2,
/// with d the tap-pair differences and e the source minus reconstruction.
#[derive(Clone)]
struct Stats {
    n: usize,
    a: Vec<f64>,
    b: Vec<f64>,
    e: f64,
}

impl Stats {
    fn new(n: usize) -> Self {
        Self {
            n,
            a: vec![0.0; n * n],
            b: vec![0.0; n],
            e: 0.0,
        }
    }
    fn add(&mut self, o: &Stats) {
        for (x, y) in self.a.iter_mut().zip(&o.a) {
            *x += y;
        }
        for (x, y) in self.b.iter_mut().zip(&o.b) {
            *x += y;
        }
        self.e += o.e;
    }
    fn accumulate(&mut self, d: &[f64], e: f64) {
        let n = self.n;
        for i in 0..n {
            if d[i] == 0.0 {
                continue;
            }
            for j in 0..n {
                self.a[i * n + j] += d[i] * d[j];
            }
            self.b[i] += d[i] * e;
        }
        self.e += e * e;
    }
    /// Squared error with integer coefficients in units of 1/128.
    fn error(&self, c: &[i32]) -> f64 {
        let n = self.n;
        let x: Vec<f64> = c.iter().map(|&v| f64::from(v) / 128.0).collect();
        let mut err = self.e;
        for i in 0..n {
            err -= 2.0 * x[i] * self.b[i];
            for j in 0..n {
                err += x[i] * self.a[i * n + j] * x[j];
            }
        }
        err
    }
    /// The least-squares filter, quantized to 1/128 and refined by
    /// coordinate descent.
    fn solve(&self) -> Vec<i32> {
        let n = self.n;
        let mut m = self.a.clone();
        let mut v = self.b.clone();
        for i in 0..n {
            m[i * n + i] += 1e-3 + m[i * n + i] * 1e-6;
        }
        // Gaussian elimination with partial pivoting.
        for col in 0..n {
            let piv = (col..n)
                .max_by(|&x, &y| m[x * n + col].abs().total_cmp(&m[y * n + col].abs()))
                .unwrap_or(col);
            if m[piv * n + col].abs() < 1e-12 {
                continue;
            }
            for k in 0..n {
                m.swap(col * n + k, piv * n + k);
            }
            v.swap(col, piv);
            for row in 0..n {
                if row == col {
                    continue;
                }
                let f = m[row * n + col] / m[col * n + col];
                if f == 0.0 {
                    continue;
                }
                for k in col..n {
                    m[row * n + k] -= f * m[col * n + k];
                }
                v[row] -= f * v[col];
            }
        }
        let mut c: Vec<i32> = (0..n)
            .map(|i| {
                let d = m[i * n + i];
                let x = if d.abs() < 1e-12 { 0.0 } else { v[i] / d };
                ((x * 128.0).round() as i32).clamp(-128, 127)
            })
            .collect();
        let mut best = self.error(&c);
        for _ in 0..2 {
            for i in 0..n {
                for delta in [-1, 1] {
                    let old = c[i];
                    c[i] = (old + delta).clamp(-128, 127);
                    let e = self.error(&c);
                    if e < best {
                        best = e;
                    } else {
                        c[i] = old;
                    }
                }
            }
        }
        c
    }
}

fn uvlc_bits(v: u32) -> u32 {
    2 * (32 - (v + 1).leading_zeros()) - 1
}

fn coeff_bits(c: &[i32]) -> u32 {
    c.iter()
        .map(|&v| uvlc_bits(v.unsigned_abs()) + u32::from(v != 0))
        .sum()
}

/// The encoder's ALF choice for a picture.
pub(super) struct AlfDecision {
    /// The APS (id 0) when any CTU uses signalled filters.
    pub aps: Option<AlfParam>,
    pub luma_aps: bool,
    pub chroma: [bool; 2],
    pub ctus: Vec<AlfCtu>,
}

impl AlfDecision {
    pub fn enabled(&self) -> bool {
        self.ctus.iter().any(|c| c.enable[0]) || self.chroma.iter().any(|&c| c)
    }
}

fn gather(p: &Plane, x: i32, y: i32) -> i32 {
    let x = x.clamp(0, p.width as i32 - 1);
    let y = y.clamp(0, p.height as i32 - 1);
    i32::from(p.at(x, y))
}

fn luma_stats(
    rec: &Plane,
    src: &Plane,
    classes: &[(usize, usize)],
    mask: &[bool],
    ctu_log2: u32,
) -> Vec<Stats> {
    let mut st = vec![Stats::new(12); 25];
    let (w, h) = (rec.width as i32, rec.height as i32);
    let bw = (w as usize).div_ceil(4);
    let wc = (w as usize).div_ceil(1 << ctu_log2);
    let mut d = [0f64; 12];
    for y in 0..h {
        for x in 0..w {
            let ctu = (y as usize >> ctu_log2) * wc + (x as usize >> ctu_log2);
            if !mask[ctu] {
                continue;
            }
            let (class, t) = classes[(y as usize / 4) * bw + x as usize / 4];
            let map = alf::transpose_map(t);
            let cur = gather(rec, x, y);
            for (i, &(dx, dy)) in LUMA_TAPS.iter().enumerate() {
                let s = gather(rec, x + dx, y + dy) + gather(rec, x - dx, y - dy) - 2 * cur;
                d[map[i]] = f64::from(s);
            }
            let e = f64::from(i32::from(src.at(x, y)) - cur);
            st[class].accumulate(&d, e);
        }
    }
    st
}

fn chroma_stats(rec: &Plane, src: &Plane, mask: &[bool], ctu_log2: u32, sx: u32, sy: u32) -> Stats {
    let mut st = Stats::new(6);
    let (w, h) = (rec.width as i32, rec.height as i32);
    let wc = ((w as usize) << sx).div_ceil(1 << ctu_log2);
    let mut d = [0f64; 6];
    for y in 0..h {
        for x in 0..w {
            let ctu = (((y as usize) << sy) >> ctu_log2) * wc + (((x as usize) << sx) >> ctu_log2);
            if !mask[ctu] {
                continue;
            }
            let cur = gather(rec, x, y);
            for (i, &(dx, dy)) in CHROMA_TAPS.iter().enumerate() {
                d[i] =
                    f64::from(gather(rec, x + dx, y + dy) + gather(rec, x - dx, y - dy) - 2 * cur);
            }
            st.accumulate(&d, f64::from(i32::from(src.at(x, y)) - cur));
        }
    }
    st
}

/// Luma filters for the 25 classes: the filter count and class mapping
/// minimizing error plus rate, by greedy merging.
fn design_luma(st: &[Stats], lambda: f64) -> (Vec<Vec<i32>>, [u8; 25]) {
    let mut groups: Vec<(Vec<usize>, Stats)> = (0..25).map(|k| (vec![k], st[k].clone())).collect();
    let mut best: Option<(f64, Vec<Vec<i32>>, [u8; 25])> = None;
    loop {
        let filters: Vec<Vec<i32>> = groups.iter().map(|g| g.1.solve()).collect();
        let err: f64 = groups.iter().zip(&filters).map(|(g, c)| g.1.error(c)).sum();
        let n = groups.len() as u32;
        let bits = uvlc_bits(n - 1)
            + if n > 1 { 25 * ceil_log2(n) } else { 0 }
            + filters.iter().map(|c| coeff_bits(c)).sum::<u32>();
        let cost = err + lambda * f64::from(bits);
        if best.as_ref().is_none_or(|b| cost < b.0) {
            let mut idx = [0u8; 25];
            for (gi, g) in groups.iter().enumerate() {
                for &k in &g.0 {
                    idx[k] = gi as u8;
                }
            }
            best = Some((cost, filters.clone(), idx));
        }
        if groups.len() == 1 {
            break;
        }
        // Merge the pair whose union loses least.
        let mut pick = (f64::MAX, 0, 1);
        for i in 0..groups.len() {
            for j in i + 1..groups.len() {
                let mut u = groups[i].1.clone();
                u.add(&groups[j].1);
                let c = u.solve();
                let loss =
                    u.error(&c) - groups[i].1.error(&filters[i]) - groups[j].1.error(&filters[j]);
                if loss < pick.0 {
                    pick = (loss, i, j);
                }
            }
        }
        let (_, i, j) = pick;
        let (members, stats) = groups.remove(j);
        groups[i].0.extend(members);
        groups[i].1.add(&stats);
    }
    let (_, filters, idx) = best.unwrap_or_default();
    (filters, idx)
}

fn build_aps(luma: Option<&(Vec<Vec<i32>>, [u8; 25])>, chroma: Option<&[Vec<i32>]>) -> AlfParam {
    let mut p = AlfParam {
        luma_coeff: vec![0; 25 * 13],
        luma_clip: vec![0; 25 * 13],
        chroma_coeff: vec![0; 8 * 7],
        chroma_clip: vec![0; 8 * 7],
        ..Default::default()
    };
    if let Some((filters, idx)) = luma {
        p.new_filter[0] = true;
        p.num_luma_filters = filters.len() as u32;
        p.coeff_delta_idx = *idx;
        for (f, c) in filters.iter().enumerate() {
            for (j, &v) in c.iter().enumerate() {
                p.luma_coeff[f * 13 + j] = v as i16;
            }
            p.luma_coeff[f * 13 + 12] = 1 << 6;
        }
    }
    if let Some(alts) = chroma {
        p.new_filter[1] = true;
        p.num_alt_chroma = alts.len() as u32;
        for (a, c) in alts.iter().enumerate() {
            for (j, &v) in c.iter().enumerate() {
                p.chroma_coeff[a * 7 + j] = v as i16;
            }
            p.chroma_coeff[a * 7 + 6] = 1 << 6;
        }
    }
    p
}

/// The APS RBSP (aps_params_type ALF, id 0).
pub(super) fn write_aps(p: &AlfParam, chroma_present: bool) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.write(0, 3); // aps_params_type: ALF_APS
    w.write(0, 5); // aps_adaptation_parameter_set_id
    w.flag(chroma_present); // aps_chroma_present_flag
    w.flag(p.new_filter[0]); // alf_luma_filter_signal_flag
    if chroma_present {
        w.flag(p.new_filter[1]); // alf_chroma_filter_signal_flag
        w.flag(false); // alf_cc_cb_filter_signal_flag
        w.flag(false); // alf_cc_cr_filter_signal_flag
    }
    let coeffs = |w: &mut BitWriter, c: &[i16]| {
        for &v in c {
            w.uvlc(u32::from(v.unsigned_abs())); // alf_*_coeff_abs
            if v != 0 {
                w.flag(v < 0); // alf_*_coeff_sign
            }
        }
    };
    if p.new_filter[0] {
        w.flag(false); // alf_luma_clip_flag
        w.uvlc(p.num_luma_filters - 1); // alf_luma_num_filters_signalled_minus1
        if p.num_luma_filters > 1 {
            let len = ceil_log2(p.num_luma_filters);
            for &i in &p.coeff_delta_idx {
                w.write(u32::from(i), len); // alf_luma_coeff_delta_idx
            }
        }
        for f in 0..p.num_luma_filters as usize {
            coeffs(&mut w, &p.luma_coeff[f * 13..f * 13 + 12]);
        }
    }
    if p.new_filter[1] {
        w.flag(false); // alf_chroma_clip_flag
        w.uvlc(p.num_alt_chroma - 1); // alf_chroma_num_alt_filters_minus1
        for a in 0..p.num_alt_chroma as usize {
            coeffs(&mut w, &p.chroma_coeff[a * 7..a * 7 + 6]);
        }
    }
    w.flag(false); // aps_extension_flag
    w.trailing_bits();
    w.data
}

fn sse(a: &Plane, b: &Plane, x0: i32, y0: i32, w: i32, h: i32) -> f64 {
    let mut s = 0f64;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let d = f64::from(a.at(x, y)) - f64::from(b.at(x, y));
            s += d * d;
        }
    }
    s
}

/// Per-CTU weighted squared error of each component.
fn ctu_errors(pic: &Picture, src: &[Plane]) -> Vec<[f64; 3]> {
    let ctu = 1i32 << pic.ctu_log2;
    let wc = pic.width_ctus as i32;
    (0..pic.ctus.len() as i32)
        .map(|a| {
            let (x0, y0) = ((a % wc) * ctu, (a / wc) * ctu);
            let (w, h) = (ctu.min(pic.width - x0), ctu.min(pic.height - y0));
            let mut e = [0f64; 3];
            for c in 0..pic.fmt.num_comp() {
                let (sx, sy) = pic.fmt.scale(c);
                e[c] = sse(
                    &pic.planes[c],
                    &src[c],
                    x0 >> sx,
                    y0 >> sy,
                    w >> sx,
                    h >> sy,
                );
            }
            e
        })
        .collect()
}

/// Chooses ALF for the SAO-filtered picture `input`.
#[allow(clippy::too_many_arguments)]
pub(super) fn decide(
    input: &Picture,
    src: &[Plane],
    sps: &Sps,
    pps: &Pps,
    ph: &PicHeader,
    sh: &SliceHeader,
    lambda: f64,
    weight: [f64; 3],
) -> AlfDecision {
    let n = input.ctus.len();
    let nc = input.fmt.num_comp();
    let base = ctu_errors(input, src);
    let classes = alf::luma_classes(&input.planes[0], input.ctu_log2, input.bit_depth);
    let mut mask = vec![true; n];
    let mut decision = AlfDecision {
        aps: None,
        luma_aps: false,
        chroma: [false; 2],
        ctus: vec![AlfCtu::default(); n],
    };
    let mut best_total = 0f64;
    let mut fixed: Vec<Vec<[f64; 3]>> = Vec::new();
    // Two passes: filters from every CTU, then from the CTUs that used them.
    for _ in 0..2 {
        let luma = design_luma(
            &luma_stats(&input.planes[0], &src[0], &classes, &mask, input.ctu_log2),
            lambda,
        );
        let chroma: Vec<Vec<i32>> = (1..nc)
            .map(|c| {
                let (sx, sy) = input.fmt.scale(c);
                chroma_stats(&input.planes[c], &src[c], &mask, input.ctu_log2, sx, sy).solve()
            })
            .collect();
        let aps = build_aps(Some(&luma), (nc > 1).then_some(chroma.as_slice()));
        let aps_bits = (write_aps(&aps, nc > 1).len() * 8) as f64;
        let mut apss: [Option<AlfParam>; 8] = Default::default();
        apss[0] = Some(aps.clone());
        let mut hdr = sh.clone();
        hdr.alf_enabled = [true, nc > 1, nc > 1];
        hdr.alf_aps_ids_luma = vec![0];
        hdr.alf_aps_id_chroma = 0;
        // Errors with each luma filter choice (16 = the APS filters) and
        // with the chroma filters.
        // The fixed sets do not depend on the pass.
        let mut errs: Vec<Vec<[f64; 3]>> = std::mem::take(&mut fixed);
        for idx in errs.len() as u16..17 {
            let mut p = input.clone();
            for c in p.ctus.iter_mut() {
                c.alf = AlfCtu {
                    enable: [true, idx == 16 && nc > 1, idx == 16 && nc > 1],
                    filter_idx: idx,
                    alt: [0, 1],
                    cc: [0, 0],
                };
            }
            alf::alf(&mut p, sps, pps, ph, std::slice::from_ref(&hdr), &apss);
            errs.push(ctu_errors(&p, src));
        }
        fixed = errs[..16].to_vec();
        let mut ctus = vec![AlfCtu::default(); n];
        let mut total = -lambda * aps_bits;
        let mut uses_aps = false;
        let mut chroma_used = [false; 2];
        for a in 0..n {
            // Luma: off (1 bin), APS (2 bins), fixed set (2 + 4 bins).
            let mut best = (lambda, None);
            for idx in 0..17u16 {
                let bins = if idx == 16 { 2.0 } else { 6.0 };
                let cost = (errs[idx as usize][a][0] - base[a][0]) * weight[0] + lambda * bins;
                if cost < best.0 {
                    best = (cost, Some(idx));
                }
            }
            if let Some(idx) = best.1 {
                ctus[a].enable[0] = true;
                ctus[a].filter_idx = idx;
                uses_aps |= idx == 16;
                total += lambda - best.0;
            }
            for c in 1..nc {
                let cost = (errs[16][a][c] - base[a][c]) * weight[c] + lambda * 2.0;
                if cost < lambda {
                    ctus[a].enable[c] = true;
                    ctus[a].alt[c - 1] = (c - 1) as u8;
                    chroma_used[c - 1] = true;
                    total += lambda - cost;
                }
            }
        }
        if total <= best_total {
            break;
        }
        best_total = total;
        let mut aps = aps;
        if !uses_aps {
            aps.new_filter[0] = false;
            aps.num_luma_filters = 0;
        }
        let any_chroma = chroma_used.iter().any(|&c| c);
        if !any_chroma {
            aps.new_filter[1] = false;
            aps.num_alt_chroma = 0;
        }
        decision = AlfDecision {
            aps: (uses_aps || any_chroma).then_some(aps),
            luma_aps: uses_aps,
            chroma: chroma_used,
            ctus: ctus.clone(),
        };
        for (m, c) in mask.iter_mut().zip(&ctus) {
            *m = c.enable[0] && c.filter_idx == 16;
        }
        if !mask.iter().any(|&m| m) {
            break;
        }
    }
    decision
}
