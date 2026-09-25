// SPDX-License-Identifier: LGPL-3.0-or-later
//! Coding tree unit syntax (H.266 clauses 7.3.11), following vvdec's
//! `CABACReader` and `Partitioner`, reconstructing each CU as it is parsed.
use super::Error;
use super::cabac::Cabac;
use super::ctx;
use super::mv::{IMV_HPEL, Mv};
use super::pic::*;
use super::ps::{AlfParam, I_SLICE, PicHeader, Pps, SliceHeader, Sps};
use super::recon;
use super::tables::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Split {
    None,
    Quad,
    Horz,
    Vert,
    TriH,
    TriV,
    CtuLevel,
    MaxTr,
    IspHorz,
    IspVert,
    /// Sub-block transform tiling, vvdec's `SBT_VER_HALF_POS0_SPLIT` + n.
    Sbt(u8),
}

impl Split {
    fn code(self) -> u32 {
        match self {
            Split::None => 0,
            Split::Quad => 1,
            Split::Horz => 2,
            Split::Vert => 3,
            Split::TriH => 4,
            Split::TriV => 5,
            _ => 6,
        }
    }
    fn is_cu(self) -> bool {
        matches!(
            self,
            Split::Quad | Split::Horz | Split::Vert | Split::TriH | Split::TriV
        )
    }
}

#[derive(Clone)]
struct Level {
    split: Split,
    parts: Vec<UnitArea>,
    idx: usize,
    cu_above: Option<u32>,
    cu_left: Option<u32>,
    qg_enable: bool,
    qg_chroma_enable: bool,
}

pub struct Partitioner {
    stack: Vec<Level>,
    pub ch_type: usize,
    pub tree: Tree,
    pub mode_type: ModeType,
    pub depth: u32,
    pub tr_depth: u32,
    pub mt_depth: u32,
    pub qt_depth: u32,
    pub subdiv: u32,
    pub implicit_bt_depth: u32,
    pub dual: bool,
    min_bt: u32,
    min_tt: u32,
    max_btd: u32,
    max_bt: u32,
    max_tt: u32,
    min_qt: u32,
    pub max_tr: u32,
    slice: u32,
    tile: u32,
    qp_subdiv: u32,
    cqp_subdiv: u32,
}

/// A slice's reference pictures and derived inter-prediction state.
pub struct SliceInter {
    pub refs: [Vec<std::rc::Rc<super::dpb::RefPic>>; 2],
    pub info: super::dpb::SliceRefs,
    /// The collocated picture when temporal MVP is enabled.
    pub col: Option<std::rc::Rc<super::dpb::RefPic>>,
    /// NoBackwardPredFlag (vvdec's `getCheckLDC`).
    pub check_ldc: bool,
    /// Symmetric MVD availability and reference indices.
    pub bidir: bool,
    pub sym_ref: [i8; 2],
}

pub struct SliceInfo<'a> {
    pub sps: &'a Sps,
    pub pps: &'a Pps,
    pub ph: &'a PicHeader,
    pub sh: &'a SliceHeader,
    pub slice_idx: u32,
    /// ALF APS per id, as referenced by this slice.
    pub alf_aps: &'a [Option<AlfParam>; 8],
    pub lmcs: Option<&'a recon::Lmcs>,
    pub scaling: Option<&'a recon::ScalingMatrices>,
    pub inter: &'a SliceInter,
}

impl<'a> SliceInfo<'a> {
    pub fn is_intra(&self) -> bool {
        self.sh.slice_type == I_SLICE
    }
    pub fn dual_tree(&self) -> bool {
        self.is_intra() && self.sps.dual_tree
    }
}

impl Partitioner {
    pub fn new(pic: &Picture, si: &SliceInfo, ctu: UnitArea, ch_type: usize, tile: u32) -> Self {
        let sps = si.sps;
        let dual = si.dual_tree();
        let val_idx = if si.is_intra() {
            if !dual { 0 } else { ch_type << 1 }
        } else {
            1
        };
        let min_cb = 1u32 << sps.log2_min_cb_size;
        let (max_btd, max_bt, max_tt, min_qt) = if si.ph.split_cons_override {
            (
                si.ph.max_mtt_depth[val_idx],
                si.ph.max_bt[val_idx],
                si.ph.max_tt[val_idx],
                si.ph.min_qt[val_idx],
            )
        } else {
            (
                sps.max_mtt_depth[val_idx],
                sps.max_bt[val_idx],
                sps.max_tt[val_idx],
                sps.min_qt[val_idx],
            )
        };
        let intra = si.is_intra();
        let mut p = Self {
            stack: Vec::with_capacity(16),
            ch_type,
            tree: Tree::D,
            mode_type: ModeType::All,
            depth: 0,
            tr_depth: 0,
            mt_depth: 0,
            qt_depth: 0,
            subdiv: 0,
            implicit_bt_depth: 0,
            dual,
            min_bt: min_cb,
            min_tt: min_cb,
            max_btd,
            max_bt,
            max_tt,
            min_qt,
            max_tr: 1 << sps.log2_max_tb_size,
            slice: si.slice_idx,
            tile,
            qp_subdiv: si.ph.cu_qp_delta_subdiv[if intra { 0 } else { 1 }],
            cqp_subdiv: si.ph.cu_chroma_qp_offset_subdiv[if intra { 0 } else { 1 }],
        };
        p.stack.push(Level {
            split: Split::CtuLevel,
            parts: vec![ctu],
            idx: 0,
            cu_above: None,
            cu_left: None,
            qg_enable: true,
            qg_chroma_enable: true,
        });
        p.set_neighbors(pic, si.sps.entropy_coding_sync);
        p
    }

    pub fn area(&self) -> &UnitArea {
        let l = self.stack.last().unwrap();
        &l.parts[l.idx]
    }
    fn level(&self) -> &Level {
        self.stack.last().unwrap()
    }
    pub fn qg_enable(&self) -> bool {
        self.level().qg_enable
    }
    pub fn qg_chroma_enable(&self) -> bool {
        self.level().qg_chroma_enable
    }
    pub fn cu_left(&self) -> Option<u32> {
        self.level().cu_left
    }
    pub fn cu_above(&self) -> Option<u32> {
        self.level().cu_above
    }
    pub fn part_idx(&self) -> usize {
        self.level().idx
    }
    pub fn is_sep_tree(&self) -> bool {
        self.tree != Tree::D || self.dual
    }

    fn set_neighbors(&mut self, pic: &Picture, wpp: bool) {
        let ch = if self.tree == Tree::C {
            1
        } else {
            self.ch_type
        };
        let b = self.area().blk[ch];
        let above =
            pic.get_cu_restricted_pos(b.x, b.y - 1, b.x, b.y, self.slice, self.tile, ch, wpp);
        let left =
            pic.get_cu_restricted_pos(b.x - 1, b.y, b.x, b.y, self.slice, self.tile, ch, wpp);
        let l = self.stack.last_mut().unwrap();
        l.cu_above = above;
        l.cu_left = left;
    }

    pub fn update_neighbors(&mut self, pic: &Picture, wpp: bool) {
        self.set_neighbors(pic, wpp);
    }

    pub fn split_series(&self) -> u32 {
        let mut series = 0;
        let mut depth = 0;
        for level in &self.stack {
            if level.split == Split::CtuLevel {
                continue;
            }
            series += level.split.code() << (depth * 3);
            depth += 1;
            if depth >= 3 {
                break;
            }
        }
        series
    }

    fn cu_sub_partitions(area: &UnitArea, split: Split) -> Vec<UnitArea> {
        let n = match split {
            Split::Quad => 4,
            Split::Horz | Split::Vert => 2,
            _ => 3,
        };
        let mut out = vec![*area; n];
        for (i, sub) in out.iter_mut().enumerate() {
            for b in sub.blk.iter_mut() {
                if !b.valid() {
                    continue;
                }
                match split {
                    Split::Quad => {
                        b.h >>= 1;
                        b.w >>= 1;
                        if i >= 2 {
                            b.y += b.h;
                        }
                        if i & 1 != 0 {
                            b.x += b.w;
                        }
                    }
                    Split::Horz => {
                        b.h >>= 1;
                        if i == 1 {
                            b.y += b.h;
                        }
                    }
                    Split::Vert => {
                        b.w >>= 1;
                        if i == 1 {
                            b.x += b.w;
                        }
                    }
                    Split::TriH => {
                        b.h >>= 1;
                        if (i + 1) & 1 != 0 {
                            b.h >>= 1;
                        }
                        if i == 1 {
                            b.y += b.h / 2;
                        }
                        if i == 2 {
                            b.y += 3 * b.h;
                        }
                    }
                    Split::TriV => {
                        b.w >>= 1;
                        if (i + 1) & 1 != 0 {
                            b.w >>= 1;
                        }
                        if i == 1 {
                            b.x += b.w / 2;
                        }
                        if i == 2 {
                            b.x += 3 * b.w;
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    fn max_tu_tiling(area: &UnitArea, max_tr: i32) -> Vec<UnitArea> {
        let size = area.blk[0];
        // The luma block may be invalid for chroma-only units; vvdec uses
        // lumaSize() of the unit area (zero), which yields a single tile.
        let tiles_h = 1.max(size.w / max_tr);
        let tiles_v = 1.max(size.h / max_tr);
        let n = tiles_h * tiles_v;
        let log2_h = 31 - (tiles_h as u32).leading_zeros();
        const W1: [usize; 4] = [0, 1, 2, 3];
        const W2: [usize; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
        const W4: [usize; 16] = [0, 1, 4, 5, 2, 3, 6, 7, 8, 9, 12, 13, 10, 11, 14, 15];
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n as usize {
            let zid = match log2_h {
                0 => W1[i],
                1 => W2[i],
                _ => W4[i],
            } as i32;
            let y = zid >> log2_h;
            let x = zid & ((1 << log2_h) - 1);
            let mut t = *area;
            for b in t.blk.iter_mut() {
                if !b.valid() {
                    continue;
                }
                b.w /= tiles_h;
                b.h /= tiles_v;
                b.x += b.w * x;
                b.y += b.h * y;
            }
            out.push(t);
        }
        out
    }

    fn isp_partitions(area: &UnitArea, split: Split, dual: bool, chroma: bool) -> Vec<UnitArea> {
        let (w, h) = (area.blk[0].w, area.blk[0].h);
        let dim = isp_split_dim(w, h, split == Split::IspHorz);
        let n = if split == Split::IspHorz {
            h / dim
        } else {
            w / dim
        };
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let mut s = *area;
            if split == Split::IspHorz {
                s.blk[0].h = dim;
                s.blk[0].y = area.blk[0].y + i * dim;
            } else {
                s.blk[0].w = dim;
                s.blk[0].x = area.blk[0].x + i * dim;
            }
            out.push(s);
        }
        let without = if !chroma {
            0
        } else if dual {
            n
        } else {
            n - 1
        };
        for s in out.iter_mut().take(without as usize) {
            s.blk[1] = Area::default();
            s.blk[2] = Area::default();
        }
        out
    }

    /// vvdec's `PartitionerImpl::getSbtTuTiling`.
    fn sbt_tiling(area: &UnitArea, t: u8) -> Vec<UnitArea> {
        let mut out = Vec::with_capacity(2);
        for i in 0..2i32 {
            let (wf, xf, hf, yf) = if t >= 4 {
                if t >= 6 {
                    let pos0 = t == 6;
                    let hf = if (i == 0 && pos0) || (i == 1 && !pos0) {
                        1
                    } else {
                        3
                    };
                    let yf = if i == 0 {
                        0
                    } else if pos0 {
                        1
                    } else {
                        3
                    };
                    (4, 0, hf, yf)
                } else {
                    let pos0 = t == 4;
                    let wf = if (i == 0 && pos0) || (i == 1 && !pos0) {
                        1
                    } else {
                        3
                    };
                    let xf = if i == 0 {
                        0
                    } else if pos0 {
                        1
                    } else {
                        3
                    };
                    (wf, xf, 4, 0)
                }
            } else if t >= 2 {
                (4, 0, 2, if i == 0 { 0 } else { 2 })
            } else {
                (2, if i == 0 { 0 } else { 2 }, 4, 0)
            };
            let mut tile = *area;
            for b in tile.blk.iter_mut() {
                if !b.valid() {
                    continue;
                }
                b.x += (b.w * xf) >> 2;
                b.y += (b.h * yf) >> 2;
                b.w = (b.w * wf) >> 2;
                b.h = (b.h * hf) >> 2;
            }
            out.push(tile);
        }
        out
    }

    pub fn split(&mut self, split: Split, pic: &Picture) {
        let area = *self.area();
        let pic_area = Area::new(0, 0, pic.width, pic.height);
        let br = (
            area.blk[0].x + area.blk[0].w - 1,
            area.blk[0].y + area.blk[0].h - 1,
        );
        let implicit = !pic_area.contains(br.0, br.1);
        let mut qg = self.qg_enable();
        let mut qgc = self.qg_chroma_enable();
        let parts = match split {
            Split::MaxTr => Self::max_tu_tiling(&area, self.max_tr as i32),
            Split::IspHorz | Split::IspVert => {
                Self::isp_partitions(&area, split, self.dual, pic.fmt.chroma != 0)
            }
            Split::Sbt(t) => Self::sbt_tiling(&area, t),
            _ => Self::cu_sub_partitions(&area, split),
        };
        let (last_above, last_left) = (self.level().cu_above, self.level().cu_left);
        match split {
            Split::Quad => {
                self.tr_depth = 0;
                self.mt_depth = 0;
                self.qt_depth += 1;
                self.subdiv += 1;
            }
            Split::Horz | Split::Vert => {
                self.tr_depth = 0;
                if implicit {
                    self.implicit_bt_depth += 1;
                }
                self.mt_depth += 1;
            }
            Split::TriH | Split::TriV => {
                self.tr_depth = 0;
                self.mt_depth += 1;
                self.subdiv += 1;
            }
            _ => self.tr_depth += 1,
        }
        self.depth += 1;
        self.subdiv += 1;
        qg &= self.subdiv <= self.qp_subdiv;
        qgc &= self.subdiv <= self.cqp_subdiv;
        self.stack.push(Level {
            split,
            parts,
            idx: 0,
            cu_above: last_above,
            cu_left: last_left,
            qg_enable: qg,
            qg_chroma_enable: qgc,
        });
    }

    pub fn next_part(&mut self, pic: &Picture, wpp: bool) -> bool {
        let l = self.stack.last_mut().unwrap();
        l.idx += 1;
        if l.idx < l.parts.len() {
            let split = l.split;
            let idx = l.idx;
            if split.is_cu() {
                self.set_neighbors(pic, wpp);
            }
            if split == Split::TriH || split == Split::TriV {
                if idx == 1 {
                    self.subdiv -= 1;
                } else {
                    self.subdiv += 1;
                }
            }
            true
        } else {
            false
        }
    }

    pub fn exit_split(&mut self, pic: &Picture) {
        let l = self.stack.pop().unwrap();
        let pic_area = Area::new(0, 0, pic.width, pic.height);
        let area = *self.area();
        let br = (
            area.blk[0].x + area.blk[0].w - 1,
            area.blk[0].y + area.blk[0].h - 1,
        );
        let implicit = !pic_area.contains(br.0, br.1);
        self.depth -= 1;
        self.subdiv -= 1;
        match l.split {
            Split::Horz | Split::Vert | Split::TriH | Split::TriV => {
                self.mt_depth -= 1;
                if implicit {
                    self.implicit_bt_depth -= 1;
                }
                if (l.split == Split::TriH || l.split == Split::TriV) && l.idx != 1 {
                    self.subdiv -= 1;
                }
            }
            Split::MaxTr | Split::IspHorz | Split::IspVert | Split::Sbt(_) => self.tr_depth -= 1,
            _ => {
                self.qt_depth -= 1;
                self.subdiv -= 1;
            }
        }
    }

    /// vvdec's `Partitioner::canSplit` (CU splits).
    pub fn can_split(&self, pic: &Picture) -> [bool; 6] {
        // [no, qt, bh, bv, th, tv]
        let mut can = [true; 6];
        let mut can_btt = self.mt_depth < self.max_btd + self.implicit_bt_depth;
        let area = self.area().blk[0];
        let area_c = if self.ch_type == 1 {
            Some(self.area().blk[1])
        } else {
            None
        };
        let level = self.level();
        if self.dual && (area.w > 64 || area.h > 64) {
            return [false, true, false, false, false, false];
        }
        if self.tree == Tree::C {
            return [true, false, false, false, false, false];
        }
        let last = level.split;
        let tr_in = area.x + area.w <= pic.width;
        let bl_in = area.y + area.h <= pic.height;
        let implicit = !bl_in || !tr_in;
        if last != Split::CtuLevel && last != Split::Quad {
            can[1] = false;
        }
        if area.w as u32 <= self.min_qt {
            can[1] = false;
        }
        if let Some(c) = area_c
            && c.w <= 4
        {
            can[1] = false;
        }
        if implicit {
            let bt_allowed = area.w as u32 <= self.max_bt
                && area.h as u32 <= self.max_bt
                && area.w <= 64
                && area.h <= 64
                && can_btt;
            can[0] = false;
            can[4] = false;
            can[5] = false;
            can[1] |= !bt_allowed;
            can[2] = bt_allowed && !bl_in && (tr_in || !can[1]);
            can[3] = bt_allowed && bl_in && !tr_in;
            can[3] &= area_c.is_none_or(|c| c.w > 4);
            can[1] |= !can[2] && !can[3];
            return can;
        }
        let (w, h) = (area.w as u32, area.h as u32);
        can_btt &= w > self.min_bt || h > self.min_bt || w > self.min_tt || h > self.min_tt;
        can_btt &= (w <= self.max_bt && h <= self.max_bt) || (w <= self.max_tt && h <= self.max_tt);
        if !can_btt {
            can[2] = false;
            can[3] = false;
            can[4] = false;
            can[5] = false;
            return can;
        }
        let allow_bt = self.mode_type != ModeType::Inter || w * h != 32;
        let allow_tt = self.mode_type != ModeType::Inter || w * h != 64;
        if w > self.max_bt || h > self.max_bt || !allow_bt {
            can[2] = false;
            can[3] = false;
        } else {
            if (last == Split::TriH || last == Split::TriV) && level.idx == 1 {
                let parallel = if last == Split::TriH {
                    Split::Horz
                } else {
                    Split::Vert
                };
                can[2] = parallel != Split::Horz;
                can[3] = parallel != Split::Vert;
            }
            can[2] &= h > self.min_bt && h <= self.max_bt;
            can[2] &= w <= 64 || h > 64;
            can[3] &= w > self.min_bt && w <= self.max_bt;
            can[3] &= w > 64 || h <= 64;
        }
        if w > self.max_tt || h > self.max_tt || !allow_tt || !(w <= 64 && h <= 64) {
            can[4] = false;
            can[5] = false;
            if !can[2] && !can[3] {
                return can;
            }
        } else {
            can[4] &= h > 2 * self.min_tt;
            can[5] &= w > 2 * self.min_tt;
        }
        if let Some(c) = area_c {
            let ca = c.w * c.h;
            can[2] &= ca > 16;
            can[4] &= ca > 32;
            can[3] &= ca > 16 && c.w > 4;
            can[5] &= ca > 32 && c.w > 8;
        }
        can
    }
}

pub fn isp_split_dim(w: i32, h: i32, hor: bool) -> i32 {
    let (split, non_split) = if hor { (h, w) } else { (w, h) };
    let min_samples = 16;
    let factor = if non_split < min_samples {
        min_samples >> (31 - (non_split as u32).leading_zeros())
    } else {
        1
    };
    if (split >> 2) < factor {
        factor
    } else {
        split >> 2
    }
}

/// `CU::canUseISPSplit`: 0 = none, 1 = horizontal only, 2 = vertical only, 4 = both.
fn can_use_isp(w: i32, h: i32, max_tr: i32) -> u8 {
    let log2 = |v: i32| 31 - (v as u32).leading_zeros() as i32;
    let not_enough = log2(w) + log2(h) <= 4;
    let too_large = w > max_tr || h > max_tr;
    let wcan = if !too_large && !not_enough { 4 } else { 2 };
    let hcan = if !too_large && !not_enough { 0 } else { 2 };
    (wcan >> hcan) as u8
}

#[derive(Clone, Copy, Default)]
pub struct CuCtx {
    pub dqp_coded: bool,
    pub cqp_adj_coded: bool,
    pub qg_start: bool,
    pub lfnst_last_scan_pos: bool,
    pub qp: i32,
    pub violates_lfnst: [bool; 2],
    pub violates_mts: bool,
    pub mts_last_scan_pos: bool,
}

pub struct CtuDecoder<'a, 's, 'b> {
    pub cabac: Cabac<'b>,
    pub si: &'a SliceInfo<'s>,
    pub pic: &'a mut Picture,
    pub chroma_qp_adj: u8,
    pub ctu_addr: u32,
    pub tile: u32,
    pub wpp: bool,
}

fn log2(v: i32) -> i32 {
    31 - (v as u32).leading_zeros() as i32
}

impl<'a, 's, 'b> CtuDecoder<'a, 's, 'b> {
    fn bin(&mut self, ctx: usize) -> u32 {
        self.cabac.decode_bin(ctx)
    }
    fn ep(&mut self) -> u32 {
        self.cabac.decode_bypass()
    }

    pub fn coding_tree_unit(
        &mut self,
        ctu_area: UnitArea,
        qps: &mut [i32; 2],
    ) -> Result<(), Error> {
        let mut cu_ctx = CuCtx {
            qp: qps[0],
            ..Default::default()
        };
        let mut part = Partitioner::new(self.pic, self.si, ctu_area, 0, self.tile);
        self.sao()?;
        self.read_alf(&part)?;
        if part.dual && self.pic.fmt.chroma != 0 {
            let mut cu_ctx_c = CuCtx {
                qp: qps[1],
                ..Default::default()
            };
            let mut part_c = Partitioner::new(self.pic, self.si, ctu_area, 1, self.tile);
            self.dt_implicit_qt_split(&mut part, &mut cu_ctx, &mut part_c, &mut cu_ctx_c)?;
            qps[0] = cu_ctx.qp;
            qps[1] = cu_ctx_c.qp;
        } else {
            self.coding_tree(&mut part, &mut cu_ctx)?;
            qps[0] = cu_ctx.qp;
        }
        Ok(())
    }

    fn dt_implicit_qt_split(
        &mut self,
        pl: &mut Partitioner,
        cl: &mut CuCtx,
        pc: &mut Partitioner,
        cc: &mut CuCtx,
    ) -> Result<(), Error> {
        if pl.area().blk[0].w > 64 {
            if self.si.pps.cu_qp_delta && pl.qg_enable() {
                cl.qg_start = true;
                cl.dqp_coded = false;
                cc.qg_start = true;
                cc.dqp_coded = false;
            }
            if self.si.sh.cu_chroma_qp_offset_enabled && pl.qg_chroma_enable() {
                cl.cqp_adj_coded = false;
                cc.cqp_adj_coded = false;
                self.chroma_qp_adj = 0;
            }
            pl.split(Split::Quad, self.pic);
            pc.split(Split::Quad, self.pic);
            loop {
                let b = pl.area().blk[0];
                if Area::new(0, 0, self.pic.width, self.pic.height).contains(b.x, b.y) {
                    self.dt_implicit_qt_split(pl, cl, pc, cc)?;
                }
                if !(pl.next_part(self.pic, self.wpp) && pc.next_part(self.pic, self.wpp)) {
                    break;
                }
            }
            // vvdec does not pop these levels; the partitioners are discarded per CTU.
            return Ok(());
        }
        self.coding_tree(pl, cl)?;
        self.coding_tree(pc, cc)?;
        Ok(())
    }

    fn sao(&mut self) -> Result<(), Error> {
        let addr = self.ctu_addr as usize;
        self.pic.ctus[addr].sao = [SaoParam::default(); 3];
        let sh = self.si.sh;
        let luma = sh.sao_enabled[0];
        let chroma = sh.sao_enabled[1] && self.pic.fmt.chroma != 0;
        if !luma && !chroma {
            return Ok(());
        }
        let w = self.pic.width_ctus;
        let ry = self.ctu_addr / w;
        let rx = self.ctu_addr - ry * w;
        let size = 1i32 << self.pic.ctu_log2;
        let (px, py) = (rx as i32 * size, ry as i32 * size);
        let mut merge: i32 = -1;
        if self
            .pic
            .get_cu_restricted_pos(
                px - size,
                py,
                px,
                py,
                self.si.slice_idx,
                self.tile,
                0,
                self.wpp,
            )
            .is_some()
        {
            merge += self.bin(ctx::SAO_MERGE_FLAG) as i32;
        }
        if merge < 0
            && self
                .pic
                .get_cu_restricted_pos(
                    px,
                    py - size,
                    px,
                    py,
                    self.si.slice_idx,
                    self.tile,
                    0,
                    self.wpp,
                )
                .is_some()
        {
            merge += (self.bin(ctx::SAO_MERGE_FLAG) as i32) << 1;
        }
        let mut p = [SaoParam::default(); 3];
        if merge >= 0 {
            p[0].mode = 2;
            p[0].type_idc = merge as u8;
            if chroma {
                p[1].mode = 2;
                p[2].mode = 2;
                p[1].type_idc = merge as u8;
                p[2].type_idc = merge as u8;
            }
            self.pic.ctus[addr].sao = p;
            return Ok(());
        }
        let first = if luma { 0 } else { 1 };
        let last = if chroma { 2 } else { 0 };
        let max_offset = (1 << (self.si.sps.bit_depth.min(10) - 5)) - 1;
        for c in first..=last {
            if c != 2 {
                if self.bin(ctx::SAO_TYPE_IDX) != 0 {
                    p[c].mode = 1;
                    // edge offset starts at 1, band offset is 0
                    p[c].type_idc = if self.ep() != 0 { 1 } else { 0 };
                }
            } else {
                p[2].mode = p[1].mode;
                p[2].type_idc = p[1].type_idc;
            }
            if p[c].mode == 0 {
                continue;
            }
            let mut off = [0i32; 4];
            for o in off.iter_mut() {
                *o = self.unary_max_eqprob(max_offset) as i32;
            }
            if p[c].type_idc == 0 {
                for o in off.iter_mut() {
                    if *o != 0 && self.ep() != 0 {
                        *o = -*o;
                    }
                }
                p[c].band_pos = self.cabac.decode_bypass_bins(5) as u8;
                p[c].offset = [off[0], off[1], off[2], off[3], 0];
                continue;
            }
            p[c].band_pos = 0;
            if c != 2 {
                p[c].type_idc += self.cabac.decode_bypass_bins(2) as u8;
            } else {
                p[2].type_idc = p[1].type_idc;
            }
            p[c].offset = [off[0], off[1], 0, -off[2], -off[3]];
        }
        self.pic.ctus[addr].sao = p;
        Ok(())
    }

    fn unary_max_eqprob(&mut self, max: u32) -> u32 {
        for k in 0..max {
            if self.ep() == 0 {
                return k;
            }
        }
        max
    }

    fn unary_max_symbol(&mut self, ctx0: usize, ctx_n: usize, max: u32) -> u32 {
        let mut ones = 0;
        while ones < max && self.bin(if ones == 0 { ctx0 } else { ctx_n }) == 1 {
            ones += 1;
        }
        ones
    }

    fn exp_golomb_eqprob(&mut self, mut count: u32) -> Result<u32, Error> {
        let mut symbol = 0u32;
        let mut bit = 1;
        while bit != 0 {
            bit = self.ep();
            symbol = symbol.wrapping_add(bit << count);
            count += 1;
            if count >= 31 {
                return Err(Error::Invalid("exp_golomb_eqprob count overflow"));
            }
        }
        count -= 1;
        if count != 0 {
            symbol = symbol.wrapping_add(self.cabac.decode_bypass_bins(count));
        }
        Ok(symbol)
    }

    fn trunc_bin(&mut self, max: u32) -> u32 {
        let thresh = if max > 256 {
            let mut t = 8;
            let mut v = 1u32 << 8;
            while v <= max {
                t += 1;
                v <<= 1;
            }
            t - 1
        } else {
            tb_max(max)
        };
        let val = 1u32 << thresh;
        let b = max - val;
        let mut symbol = self.cabac.decode_bypass_bins(thresh);
        if symbol >= val - b {
            let alt = self.ep();
            symbol <<= 1;
            symbol += alt;
            symbol -= val - b;
        }
        symbol
    }

    fn read_alf(&mut self, part: &Partitioner) -> Result<(), Error> {
        let addr = self.ctu_addr as usize;
        let w = self.pic.width_ctus;
        let ry = self.ctu_addr / w;
        let rx = self.ctu_addr - ry * w;
        let size = 1i32 << self.pic.ctu_log2;
        let (px, py) = (rx as i32 * size, ry as i32 * size);
        let left = self
            .pic
            .get_cu_restricted_pos(px - 1, py, px, py, part.slice, part.tile, 0, self.wpp)
            .is_some();
        let above = self
            .pic
            .get_cu_restricted_pos(px, py - 1, px, py, part.slice, part.tile, 0, self.wpp)
            .is_some();
        let left_d = if left {
            self.pic.ctus[addr - 1].alf
        } else {
            AlfCtu::default()
        };
        let above_d = if above {
            self.pic.ctus[addr - w as usize].alf
        } else {
            AlfCtu::default()
        };
        let mut cur = AlfCtu::default();
        let sh = self.si.sh;
        if sh.alf_enabled[0] {
            for c in 0..3 {
                if !sh.alf_enabled[c] {
                    continue;
                }
                let ctx_inc = usize::from(left_d.enable[c]) + usize::from(above_d.enable[c]);
                cur.enable[c] = self.bin(ctx::CTB_ALF_FLAG + c * 3 + ctx_inc) != 0;
                if c == 0 && cur.enable[0] {
                    let num_aps = sh.alf_aps_ids_luma.len() as u32;
                    let use_prev = num_aps > 0 && self.bin(ctx::ALF_USE_TEMPORAL_FILT) != 0;
                    let mut idx = 0;
                    if use_prev {
                        if num_aps > 1 {
                            idx = self.trunc_bin(num_aps);
                        }
                        idx += 16;
                    } else {
                        idx = self.trunc_bin(16);
                    }
                    cur.filter_idx = idx as u16;
                }
                if c > 0 {
                    let aps = self.si.alf_aps[sh.alf_aps_id_chroma as usize]
                        .as_ref()
                        .ok_or(Error::Invalid("APS not initialized"))?;
                    let num_alts = aps.num_alt_chroma;
                    cur.alt[c - 1] = 0;
                    if cur.enable[c] {
                        let mut decoded = 0u8;
                        while u32::from(decoded) + 1 < num_alts
                            && self.bin(ctx::CTB_ALF_ALTERNATIVE + c - 1) != 0
                        {
                            decoded += 1;
                        }
                        cur.alt[c - 1] = decoded;
                    }
                }
            }
        }
        for c in 1..self.pic.fmt.num_comp() {
            if sh.ccalf_enabled[c - 1] {
                let mut ctx_inc =
                    usize::from(left_d.cc[c - 1] != 0) + usize::from(above_d.cc[c - 1] != 0);
                if c == 2 {
                    ctx_inc += 3;
                }
                let mut idc = self.bin(ctx::CC_ALF_FILTER_CONTROL_FLAG + ctx_inc);
                if idc != 0 {
                    let aps_id = sh.ccalf_aps_id[c - 1] as usize;
                    let count = self.si.alf_aps[aps_id]
                        .as_ref()
                        .ok_or(Error::Invalid("APS not initialized"))?
                        .cc_count[c - 1];
                    while idc != count && self.ep() != 0 {
                        idc += 1;
                    }
                }
                cur.cc[c - 1] = idc as u8;
            }
        }
        self.pic.ctus[addr].alf = cur;
        Ok(())
    }

    fn coding_tree(&mut self, part: &mut Partitioner, cu_ctx: &mut CuCtx) -> Result<(), Error> {
        let pps = self.si.pps;
        let mode_parent = part.mode_type;
        if pps.cu_qp_delta && part.qg_enable() && part.ch_type == 0 {
            cu_ctx.qg_start = true;
            cu_ctx.dqp_coded = false;
        }
        if self.si.sh.cu_chroma_qp_offset_enabled && part.qg_chroma_enable() {
            cu_ctx.cqp_adj_coded = false;
            self.chroma_qp_adj = 0;
        }
        let split = self.split_cu_mode(part)?;
        if split != Split::None {
            part.mode_type = self.mode_constraint(part, split, mode_parent);
            let chroma_not_split =
                mode_parent == ModeType::All && part.mode_type == ModeType::Intra;
            if chroma_not_split && part.ch_type != 0 {
                return Err(Error::Invalid("chType must be luma"));
            }
            if part.tree == Tree::D {
                part.tree = if chroma_not_split { Tree::L } else { Tree::D };
            }
            part.split(split, self.pic);
            let chan = self.pic.chan_area(part.ch_type);
            loop {
                let b = part.area().blk[part.ch_type];
                if chan.contains(b.x, b.y) {
                    self.coding_tree(part, cu_ctx)?;
                }
                if !part.next_part(self.pic, self.wpp) {
                    break;
                }
            }
            part.exit_split(self.pic);
            if chroma_not_split {
                part.ch_type = 1;
                part.tree = Tree::C;
                part.update_neighbors(self.pic, self.wpp);
                let b = part.area().blk[1];
                if self.pic.chan_area(1).contains(b.x, b.y) {
                    self.coding_tree(part, cu_ctx)?;
                } else {
                    return Err(Error::Invalid(
                        "Unexpected behavior, not parsing chroma even though luma data is available!",
                    ));
                }
                part.ch_type = 0;
                part.tree = Tree::D;
            }
            part.mode_type = mode_parent;
            return Ok(());
        }
        let mut area = *part.area();
        let mut tree = part.tree;
        if part.ch_type == 1 {
            area.blk[0] = Area::default();
            tree = Tree::C;
        } else if part.dual || part.tree == Tree::L {
            area.blk[1] = Area::default();
            area.blk[2] = Area::default();
            tree = Tree::L;
        }
        // addCU
        let ch = part.ch_type;
        let ctu = self.pic.ctu_addr_of(area.blk[ch].x, area.blk[ch].y, ch);
        let ctud = &mut self.pic.ctus[ctu as usize];
        ctud.num_cus += 1;
        let cu = Cu {
            blk: area.blk,
            ch_type: ch,
            tree,
            mode_type: part.mode_type,
            qt_depth: part.qt_depth,
            depth: part.depth,
            split_series: part.split_series(),
            slice: self.si.slice_idx,
            tile: part.tile,
            ctu,
            idx: ctud.num_cus,
            left: part.cu_left(),
            above: part.cu_above(),
            first_tu: self.pic.tus.len() as u32,
            // vvdec's CodingUnit::minInit
            intra_dir: [DC, 0],
            ref_idx: [-1, -1],
            ..Default::default()
        };
        let cu_id = self.pic.cus.len() as u32;
        self.pic.cus.push(cu);
        self.pic.fill_map(cu_id);
        if cu_ctx.qg_start {
            cu_ctx.qg_start = false;
            cu_ctx.qp = self.predict_qp(cu_id, cu_ctx.qp);
        }
        let mut luma_qp_local = None;
        if pps.cu_qp_delta && part.is_sep_tree() && ch == 1 {
            let c = self.pic.cus[cu_id as usize].blk[1];
            let (cx, cy) = (c.x + (c.w >> 1), c.y + (c.h >> 1));
            let (lx, ly) = (cx << self.pic.fmt.sx, cy << self.pic.fmt.sy);
            let col = self
                .pic
                .get_cu(lx, ly, 0)
                .ok_or(Error::Invalid("colLumaCU shall exist"))?;
            luma_qp_local = Some(cu_ctx.qp);
            cu_ctx.qp = self.pic.cus[col as usize].qp;
        }
        self.pic.cus[cu_id as usize].qp = cu_ctx.qp;
        self.pic.cus[cu_id as usize].chroma_qp_adj = self.chroma_qp_adj;
        self.coding_unit(cu_id, part, cu_ctx)?;
        if let Some(q) = luma_qp_local {
            cu_ctx.qp = q;
        }
        recon::reconstruct_cu(self.pic, self.si, cu_id)?;
        Ok(())
    }

    fn predict_qp(&self, cu_id: u32, prev: i32) -> i32 {
        let cu = &self.pic.cus[cu_id as usize];
        let ch = cu.ch_type;
        let b = cu.blk[ch];
        let above = self.pic.get_cu(b.x, b.y - 1, ch);
        let left = self.pic.get_cu(b.x - 1, b.y, ch);
        let (sx, sy) = self.pic.fmt.scale(ch);
        let mask_w = ((1i32 << self.pic.ctu_log2) - 1) >> sx;
        let mask_h = ((1i32 << self.pic.ctu_log2) - 1) >> sy;
        let ctu_x = cu.ctu % self.pic.width_ctus;
        let pps = self.si.pps;
        let tile_col = pps.ctu_to_tile_col[ctu_x as usize];
        let tile_x = pps.tile_col_bd[tile_col as usize];
        if ctu_x == tile_x
            && b.x & mask_w == 0
            && b.y & mask_h == 0
            && let Some(a) = above
        {
            let ac = &self.pic.cus[a as usize];
            if ac.slice == cu.slice && ac.tile == cu.tile {
                return ac.qp;
            }
        }
        let a = if b.y & mask_h != 0 {
            above.map_or(prev, |a| self.pic.cus[a as usize].qp)
        } else {
            prev
        };
        let l = if b.x & mask_w != 0 {
            left.map_or(prev, |l| self.pic.cus[l as usize].qp)
        } else {
            prev
        };
        (a + l + 1) >> 1
    }

    fn mode_constraint(&mut self, part: &Partitioner, split: Split, parent: ModeType) -> ModeType {
        let fmt = self.pic.fmt;
        let val = if part.dual || parent != ModeType::All || fmt.chroma == 3 || fmt.chroma == 0 {
            0
        } else {
            let a = part.area();
            let mut min_luma = a.blk[0].area();
            match split {
                Split::Quad | Split::TriH | Split::TriV => min_luma >>= 2,
                Split::Horz | Split::Vert => min_luma >>= 1,
                _ => {}
            }
            let min_chroma = min_luma >> (fmt.sx + fmt.sy);
            let c = a.blk[1];
            let is_2xn = (c.w == 4 && split == Split::Vert) || (c.w == 8 && split == Split::TriV);
            if min_chroma >= 16 && !is_2xn {
                0
            } else if min_luma < 32 || self.si.is_intra() {
                1
            } else {
                2
            }
        };
        match val {
            2 => {
                let l = part
                    .cu_left()
                    .map(|c| self.pic.cus[c as usize].pred == Pred::Intra)
                    .unwrap_or(false);
                let a = part
                    .cu_above()
                    .map(|c| self.pic.cus[c as usize].pred == Pred::Intra)
                    .unwrap_or(false);
                let ctx_id = usize::from(l || a);
                if self.bin(ctx::MODE_CONS_FLAG + ctx_id) != 0 {
                    ModeType::Intra
                } else {
                    ModeType::Inter
                }
            }
            1 => ModeType::Intra,
            _ => part.mode_type,
        }
    }

    fn split_cu_mode(&mut self, part: &Partitioner) -> Result<Split, Error> {
        let can = part.can_split(self.pic);
        let num_hor = u32::from(can[2]) + u32::from(can[4]);
        let num_ver = u32::from(can[3]) + u32::from(can[5]);
        let num_split = (u32::from(can[1]) << 1) + num_hor + num_ver;
        let mut is_split = num_split != 0;
        if can[0] && !is_split {
            return Ok(Split::None);
        }
        let ch = part.ch_type;
        let left = part.cu_left().map(|c| &self.pic.cus[c as usize]);
        let above = part.cu_above().map(|c| &self.pic.cus[c as usize]);
        let w = part.area().blk[ch].w;
        let h = part.area().blk[ch].h;
        let (l_h, l_qt) = left.map_or((0, 0), |c| (c.blk[ch].h, c.qt_depth));
        let (a_w, a_qt) = above.map_or((0, 0), |c| (c.blk[ch].w, c.qt_depth));
        let has_l = left.is_some();
        let has_a = above.is_some();
        if can[0] && is_split {
            let mut ctx_split = usize::from(has_l && l_h < h) + usize::from(has_a && a_w < w);
            const OFFSET: [usize; 7] = [0, 0, 0, 3, 3, 6, 6];
            ctx_split += OFFSET[num_split as usize];
            is_split = self.bin(ctx::SPLIT_FLAG + ctx_split) != 0;
        }
        if !is_split {
            return Ok(Split::None);
        }
        let can_btt = num_hor != 0 || num_ver != 0;
        let mut is_qt = can[1];
        if is_qt && can_btt {
            let mut c = usize::from(has_l && l_qt > part.qt_depth)
                + usize::from(has_a && a_qt > part.qt_depth);
            c += if part.qt_depth < 2 { 0 } else { 3 };
            is_qt = self.bin(ctx::SPLIT_QT_FLAG + c) != 0;
        }
        if is_qt {
            vtrace!("split_cu_mode() mode={}", Split::Quad.code());
            return Ok(Split::Quad);
        }
        let can_hor = num_hor != 0;
        let mut is_ver = num_ver != 0;
        if is_ver && can_hor {
            let mut c = 0;
            if num_ver == num_hor {
                if has_l && has_a {
                    let dep_above = w >> log2(a_w);
                    let dep_left = h >> log2(l_h);
                    c = if dep_above == dep_left {
                        0
                    } else if dep_above < dep_left {
                        1
                    } else {
                        2
                    };
                }
            } else if num_ver < num_hor {
                c = 3;
            } else {
                c = 4;
            }
            is_ver = self.bin(ctx::SPLIT_HV_FLAG + c) != 0;
        }
        let can14 = if is_ver { can[5] } else { can[4] };
        let mut is12 = if is_ver { can[3] } else { can[2] };
        if is12 && can14 {
            let c = usize::from(part.mt_depth <= 1) + (usize::from(is_ver) << 1);
            is12 = self.bin(ctx::SPLIT12_FLAG + c) != 0;
        }
        let split = match (is_ver, is12) {
            (true, true) => Split::Vert,
            (true, false) => Split::TriV,
            (false, true) => Split::Horz,
            (false, false) => Split::TriH,
        };
        vtrace!("split_cu_mode() mode={}", split.code());
        Ok(split)
    }

    fn cu(&self, id: u32) -> &Cu {
        &self.pic.cus[id as usize]
    }
    fn cu_mut(&mut self, id: u32) -> &mut Cu {
        &mut self.pic.cus[id as usize]
    }

    fn coding_unit(
        &mut self,
        cu_id: u32,
        part: &mut Partitioner,
        cu_ctx: &mut CuCtx,
    ) -> Result<(), Error> {
        let sps = self.si.sps;
        if !self.si.is_intra() || sps.ibc {
            if self.cu(cu_id).blk[0].valid() {
                self.cu_skip_flag(cu_id)?;
            }
            if self.cu(cu_id).skip {
                self.cu_mut(cu_id).act = false;
                self.add_empty_tus(cu_id, part);
                self.prediction_unit(cu_id)?;
                if self.cu(cu_id).pred == Pred::Inter {
                    self.derive_cu_mv(cu_id)?;
                }
                return Ok(());
            }
            self.pred_mode(cu_id)?;
        } else {
            self.cu_mut(cu_id).pred = Pred::Intra;
        }
        if self.cu(cu_id).pred == Pred::Intra {
            self.adaptive_color_transform(cu_id);
        }
        self.cu_pred_data(cu_id)?;
        self.cu_residual(cu_id, part, cu_ctx)?;
        Ok(())
    }

    fn is_cons_intra(&self, cu_id: u32) -> bool {
        self.cu(cu_id).mode_type == ModeType::Intra
    }
    fn is_cons_inter(&self, cu_id: u32) -> bool {
        self.cu(cu_id).mode_type == ModeType::Inter
    }

    fn ctx_ibc(&self, cu_id: u32) -> usize {
        let c = self.cu(cu_id);
        usize::from(c.left.is_some_and(|l| self.cu(l).pred == Pred::Ibc))
            + usize::from(c.above.is_some_and(|a| self.cu(a).pred == Pred::Ibc))
    }

    fn cu_skip_flag(&mut self, cu_id: u32) -> Result<(), Error> {
        let c = self.cu(cu_id);
        let ibc_flag = self.si.sps.ibc && c.lw() <= 64 && c.lh() <= 64;
        let ctx_skip = usize::from(c.left.is_some_and(|l| self.cu(l).skip))
            + usize::from(c.above.is_some_and(|a| self.cu(a).skip));
        if (self.si.is_intra() || self.is_cons_intra(cu_id)) && ibc_flag {
            let skip = self.bin(ctx::SKIP_FLAG + ctx_skip) != 0;
            vtrace!("cu_skip_flag() ctx={} skip={}", ctx_skip, u8::from(skip));
            if skip {
                let c = self.cu_mut(cu_id);
                c.skip = true;
                c.pred = Pred::Ibc;
            }
            return Ok(());
        } else if !ibc_flag && ((c.lw() == 4 && c.lh() == 4) || self.is_cons_intra(cu_id)) {
            return Ok(());
        }
        let skip = self.bin(ctx::SKIP_FLAG + ctx_skip) != 0;
        vtrace!("cu_skip_flag() ctx={} skip={}", ctx_skip, u8::from(skip));
        if skip && ibc_flag && !self.is_cons_inter(cu_id) {
            let c = self.cu(cu_id);
            if c.lw() == 4 && c.lh() == 4 {
                let c = self.cu_mut(cu_id);
                c.skip = true;
                c.pred = Pred::Ibc;
                return Ok(());
            }
            let ctx_id = self.ctx_ibc(cu_id);
            let ibc = self.bin(ctx::IBC_FLAG + ctx_id) != 0;
            if ibc {
                let c = self.cu_mut(cu_id);
                c.skip = true;
                c.pred = Pred::Ibc;
            }
            vtrace!(
                "ibc() ctx={} cu.predMode={}",
                ctx_id,
                if ibc { 2 } else { 0 }
            );
        }
        if skip {
            self.cu_mut(cu_id).skip = true;
        }
        Ok(())
    }

    fn pred_mode(&mut self, cu_id: u32) -> Result<(), Error> {
        if self.is_cons_inter(cu_id) {
            return Ok(());
        }
        let c = self.cu(cu_id);
        let mut ibc_allowed = false;
        if self.si.is_intra() || (c.lw() == 4 && c.lh() == 4) || self.is_cons_intra(cu_id) {
            ibc_allowed = true;
            self.cu_mut(cu_id).pred = Pred::Intra;
        } else {
            let ctx_id = usize::from(
                c.above.is_some_and(|a| self.cu(a).pred == Pred::Intra)
                    || c.left.is_some_and(|l| self.cu(l).pred == Pred::Intra),
            );
            if self.bin(ctx::PRED_MODE + ctx_id) != 0 {
                self.cu_mut(cu_id).pred = Pred::Intra;
            } else {
                ibc_allowed = true;
            }
        }
        let c = self.cu(cu_id);
        ibc_allowed &= c.ch_type == 0 && self.si.sps.ibc && c.lw() <= 64 && c.lh() <= 64;
        if ibc_allowed {
            let ctx_id = self.ctx_ibc(cu_id);
            if self.bin(ctx::IBC_FLAG + ctx_id) != 0 {
                self.cu_mut(cu_id).pred = Pred::Ibc;
            }
        }
        Ok(())
    }

    fn adaptive_color_transform(&mut self, cu_id: u32) {
        if !self.si.sps.act || self.cu(cu_id).is_sep_tree(self.si.dual_tree()) {
            return;
        }
        let v = self.bin(ctx::ACT_FLAG) != 0;
        self.cu_mut(cu_id).act = v;
    }

    /// vvdec's `prediction_unit`, followed by the motion derivation of
    /// `DecCu::xDeriveCUMV`.
    fn prediction_unit(&mut self, cu_id: u32) -> Result<(), Error> {
        let merge = if self.cu(cu_id).skip {
            true
        } else {
            let m = self.bin(ctx::MERGE_FLAG) != 0;
            let c = self.cu(cu_id);
            vtrace!(
                "merge_flag() merge={} pos=({},{}) size={}x{}",
                u8::from(m),
                c.lx(),
                c.ly(),
                c.lw(),
                c.lh()
            );
            m
        };
        self.cu_mut(cu_id).merge = merge;
        if self.cu(cu_id).pred == Pred::Ibc {
            return self.prediction_unit_ibc(cu_id, merge);
        }
        if merge {
            self.merge_data(cu_id);
        } else {
            self.inter_pred_idc(cu_id);
            self.affine_flag(cu_id);
            self.smvd_mode(cu_id);
            let c = self.cu(cu_id).clone();
            let ncp = if c.affine {
                if c.affine_type == 1 { 3 } else { 2 }
            } else {
                1
            };
            if c.inter_dir != 2 {
                self.ref_idx(cu_id, 0);
                for k in 0..ncp {
                    let (x, y) = self.mvd_coding();
                    self.cu_mut(cu_id).mv[0][k] = Mv::new(x, y);
                }
                self.mvp_flag(cu_id, 0);
            }
            if c.inter_dir != 1 {
                if self.cu(cu_id).smvd != 1 {
                    self.ref_idx(cu_id, 1);
                    if !(self.si.ph.mvd_l1_zero && c.inter_dir == 3) {
                        for k in 0..ncp {
                            let (x, y) = self.mvd_coding();
                            self.cu_mut(cu_id).mv[1][k] = Mv::new(x, y);
                        }
                    }
                }
                self.mvp_flag(cu_id, 1);
            }
        }
        if self.cu(cu_id).smvd != 0 {
            let cur = (self.cu(cu_id).smvd - 1) as usize;
            let m = self.cu(cu_id).mv[cur][0];
            let sym = self.si.inter.sym_ref[1 - cur];
            let c = self.cu_mut(cu_id);
            c.mv[1 - cur][0] = Mv::new(-m.x, -m.y);
            c.ref_idx[1 - cur] = sym;
        }
        Ok(())
    }

    /// IBC part of `prediction_unit` with the block vector derivation.
    fn prediction_unit_ibc(&mut self, cu_id: u32, merge: bool) -> Result<(), Error> {
        let max_cand = self.si.sps.max_num_ibc_merge_cand;
        let mut mvd = (0i32, 0i32);
        if merge {
            let mut idx = 0u32;
            if max_cand > 1 && self.bin(ctx::MERGE_IDX) != 0 {
                idx = 1;
                while idx < max_cand - 1 && self.ep() != 0 {
                    idx += 1;
                }
            }
            self.cu_mut(cu_id).merge_idx = idx as u8;
        } else {
            mvd = self.mvd_coding();
            let mvp = if max_cand == 1 {
                0
            } else {
                self.bin(ctx::MVP_IDX) as u8
            };
            self.cu_mut(cu_id).mvp_idx[0] = mvp;
            // amvr_mode: IBC always uses integer or four-sample precision
            let mut imv = 0u8;
            if self.si.sps.amvr && (mvd.0 != 0 || mvd.1 != 0) {
                imv = 1 + self.bin(ctx::IMV_FLAG + 1) as u8;
            }
            self.cu_mut(cu_id).imv = imv;
        }
        self.derive_bv(cu_id, merge, mvd);
        Ok(())
    }

    fn merge_data(&mut self, cu_id: u32) {
        let sps = self.si.sps;
        // subblock_merge_flag
        let c = self.cu(cu_id);
        if !self.si.is_intra()
            && self.si.ph.max_num_affine_merge_cand > 0
            && c.lw() >= 8
            && c.lh() >= 8
        {
            let ctx_id = self.ctx_affine(cu_id);
            let f = self.bin(ctx::SUBBLOCK_MERGE_FLAG + ctx_id) != 0;
            self.cu_mut(cu_id).affine = f;
            let c = self.cu(cu_id);
            vtrace!(
                "subblock_merge_flag() subblock_merge_flag={} ctx={} pos=({},{})",
                u8::from(f),
                ctx_id,
                c.lx(),
                c.ly()
            );
        }
        if self.cu(cu_id).affine {
            self.merge_idx(cu_id);
            return;
        }
        let c = self.cu(cu_id);
        let (w, h) = (c.lw(), c.lh());
        let ciip_avail = sps.ciip && !c.skip && w < 128 && h < 128 && w * h >= 64;
        let geo_avail = sps.gpm
            && self.si.sh.slice_type == super::ps::B_SLICE
            && w >= 8
            && h >= 8
            && w <= 64
            && h <= 64
            && w < 8 * h
            && h < 8 * w;
        let mut regular = true;
        if geo_avail || ciip_avail {
            regular = self.bin(ctx::REGULAR_MERGE_FLAG + usize::from(!c.skip)) != 0;
        }
        if regular {
            if sps.mmvd {
                let f = self.bin(ctx::MMVD_FLAG) != 0;
                self.cu_mut(cu_id).mmvd = f;
            }
        } else {
            if geo_avail && ciip_avail {
                let f = self.bin(ctx::CIIP_FLAG) != 0;
                self.cu_mut(cu_id).ciip = f;
            } else if ciip_avail {
                self.cu_mut(cu_id).ciip = true;
            }
            if self.cu(cu_id).ciip {
                let c = self.cu_mut(cu_id);
                c.intra_dir = [PLANAR, DM_CHROMA];
            } else {
                self.cu_mut(cu_id).geo = true;
            }
        }
        if self.cu(cu_id).mmvd {
            self.mmvd_merge_idx(cu_id);
        } else {
            self.merge_idx(cu_id);
        }
    }

    fn merge_idx(&mut self, cu_id: u32) {
        let sps = self.si.sps;
        if self.cu(cu_id).geo {
            let split = self.trunc_bin(64);
            let max = sps.max_num_gpm_cand as i32;
            let n2 = max - 2;
            let mut c0 = 0i32;
            let mut c1 = 0i32;
            if self.bin(ctx::MERGE_IDX) != 0 {
                c0 += self.unary_max_eqprob(n2 as u32) as i32 + 1;
            }
            if n2 > 0 && self.bin(ctx::MERGE_IDX) != 0 {
                c1 += self.unary_max_eqprob((n2 - 1) as u32) as i32 + 1;
            }
            c1 += i32::from(c1 >= c0);
            let c = self.cu_mut(cu_id);
            c.geo_split = split as u8;
            c.geo_idx = [c0 as u8, c1 as u8];
            vtrace!("merge_idx() geo_split_dir={}", split);
            vtrace!("merge_idx() geo_idx0={}", c0);
            vtrace!("merge_idx() geo_idx1={}", c1);
            return;
        }
        let affine = self.cu(cu_id).affine;
        let (n1, ctx_id) = if affine {
            (
                self.si.ph.max_num_affine_merge_cand as i32 - 1,
                ctx::AFF_MERGE_IDX,
            )
        } else {
            (sps.max_num_merge_cand as i32 - 1, ctx::MERGE_IDX)
        };
        let mut idx = 0i32;
        if n1 > 0 && self.bin(ctx_id) != 0 {
            idx = 1;
            while idx < n1 && self.ep() != 0 {
                idx += 1;
            }
        }
        self.cu_mut(cu_id).merge_idx = idx as u8;
        if affine {
            vtrace!("aff_merge_idx() aff_merge_idx={}", idx);
        } else {
            vtrace!("merge_idx() merge_idx={}", idx);
        }
    }

    fn mmvd_merge_idx(&mut self, cu_id: u32) {
        let n = self.si.sps.max_num_merge_cand;
        let base_n1 = if n > 1 { 1 } else { 0 };
        let mut v0 = 0u32;
        if base_n1 > 0 && self.bin(ctx::MMVD_MERGE_IDX) != 0 {
            v0 = 1;
        }
        let mut v1 = 0u32;
        if self.bin(ctx::MMVD_STEP_MVP_IDX) != 0 {
            v1 = 1;
            while v1 < 7 && self.ep() != 0 {
                v1 += 1;
            }
        }
        let mut v2 = 0u32;
        if self.ep() != 0 {
            v2 += 2;
        }
        if self.ep() != 0 {
            v2 += 1;
        }
        let idx = v0 * 32 + v1 * 4 + v2;
        self.cu_mut(cu_id).mmvd_idx = idx as u8;
        vtrace!("mmvd_merge_idx() mmvd_merge_idx={}", idx);
    }

    fn inter_pred_idc(&mut self, cu_id: u32) {
        if self.si.sh.slice_type == super::ps::P_SLICE {
            self.cu_mut(cu_id).inter_dir = 1;
            return;
        }
        let c = self.cu(cu_id);
        if c.lw() + c.lh() > 12 {
            let ctx_id = (7 - ((log2(c.lw()) + log2(c.lh()) + 1) >> 1)) as usize;
            if self.bin(ctx::INTER_DIR + ctx_id) != 0 {
                self.cu_mut(cu_id).inter_dir = 3;
                return;
            }
        }
        let d = if self.bin(ctx::INTER_DIR + 5) != 0 {
            2
        } else {
            1
        };
        self.cu_mut(cu_id).inter_dir = d;
    }

    fn ctx_affine(&self, cu_id: u32) -> usize {
        let c = self.cu(cu_id);
        usize::from(c.left.is_some_and(|l| self.cu(l).affine))
            + usize::from(c.above.is_some_and(|a| self.cu(a).affine))
    }

    fn affine_flag(&mut self, cu_id: u32) {
        let c = self.cu(cu_id);
        if self.si.sps.affine && c.lw() >= 16 && c.lh() >= 16 {
            let ctx_id = self.ctx_affine(cu_id);
            let f = self.bin(ctx::AFFINE_FLAG + ctx_id) != 0;
            self.cu_mut(cu_id).affine = f;
            if f && self.si.sps.affine_type {
                let t = self.bin(ctx::AFFINE_TYPE) as u8;
                self.cu_mut(cu_id).affine_type = t;
            }
        }
    }

    fn smvd_mode(&mut self, cu_id: u32) {
        let c = self.cu(cu_id);
        if c.inter_dir != 3 || c.affine || !self.si.sps.smvd || self.si.ph.mvd_l1_zero {
            return;
        }
        if !self.si.inter.bidir {
            return;
        }
        let f = self.bin(ctx::SMVD_FLAG) as u8;
        self.cu_mut(cu_id).smvd = f;
    }

    fn ref_idx(&mut self, cu_id: u32, l: usize) {
        if self.cu(cu_id).smvd != 0 {
            let r = self.si.inter.sym_ref[l];
            self.cu_mut(cu_id).ref_idx[l] = r;
            return;
        }
        let num = self.si.sh.num_ref_idx[l] as i32;
        let v = if num <= 1 || self.bin(ctx::REF_PIC) == 0 {
            0
        } else if num <= 2 || self.bin(ctx::REF_PIC + 1) == 0 {
            1
        } else {
            let mut idx = 3;
            loop {
                if num <= idx || self.ep() == 0 {
                    break idx - 1;
                }
                idx += 1;
            }
        };
        self.cu_mut(cu_id).ref_idx[l] = v as i8;
    }

    fn mvp_flag(&mut self, cu_id: u32, l: usize) {
        let v = self.bin(ctx::MVP_IDX) as u8;
        self.cu_mut(cu_id).mvp_idx[l] = v;
    }

    /// `CU::hasSubCUNonZeroMVd` / `hasSubCUNonZeroAffineMVd`.
    fn has_nonzero_mvd(&self, cu_id: u32, affine: bool) -> bool {
        let c = self.cu(cu_id);
        let n = if !affine {
            1
        } else if c.affine_type == 1 {
            3
        } else {
            2
        };
        let nz = |l: usize| (0..n).any(|k| c.mv[l][k] != Mv::default());
        (c.inter_dir != 2 && nz(0))
            || (c.inter_dir != 1 && (!self.si.ph.mvd_l1_zero || c.inter_dir != 3) && nz(1))
    }

    fn amvr_mode(&mut self, cu_id: u32) {
        if !self.si.sps.amvr || !self.has_nonzero_mvd(cu_id, false) {
            return;
        }
        let mut v = self.bin(ctx::IMV_FLAG);
        if v != 0 {
            self.cu_mut(cu_id).imv = v as u8;
            v = self.bin(ctx::IMV_FLAG + 4);
            self.cu_mut(cu_id).imv = if v != 0 { 1 } else { IMV_HPEL };
            if v != 0 {
                v = self.bin(ctx::IMV_FLAG + 1) + 1;
                self.cu_mut(cu_id).imv = v as u8;
            }
        }
    }

    fn affine_amvr_mode(&mut self, cu_id: u32) {
        if !self.si.sps.affine_amvr || !self.has_nonzero_mvd(cu_id, true) {
            return;
        }
        let mut v = self.bin(ctx::IMV_FLAG + 2);
        if v != 0 {
            v = self.bin(ctx::IMV_FLAG + 3) + 1;
        }
        self.cu_mut(cu_id).imv = v as u8;
    }

    /// vvdec's `sbt_mode` with `CU::checkAllowedSbt`.
    fn sbt_mode(&mut self, cu_id: u32) {
        let c = self.cu(cu_id);
        if !self.si.sps.sbt || c.pred != Pred::Inter || c.ciip {
            return;
        }
        let (w, h) = (c.lw(), c.lh());
        let max = 1i32 << self.si.sps.log2_max_tb_size;
        if w > max || h > max {
            return;
        }
        // bits: VER_HALF 1, HOR_HALF 2, VER_QUAD 3, HOR_QUAD 4
        let ver_half = w >= 8;
        let hor_half = h >= 8;
        let ver_quad = w >= 16;
        let hor_quad = h >= 16;
        if !(ver_half || hor_half || ver_quad || hor_quad) {
            return;
        }
        if self.bin(ctx::SBT_FLAG + usize::from(w * h <= 256)) == 0 {
            return;
        }
        let quad = if (hor_half || ver_half) && (hor_quad || ver_quad) {
            self.bin(ctx::SBT_QUAD_FLAG) != 0
        } else {
            false
        };
        let hor = if (quad && ver_quad && hor_quad) || (!quad && ver_half && hor_half) {
            let ctx_id = if w == h {
                0
            } else if w < h {
                1
            } else {
                2
            };
            self.bin(ctx::SBT_HOR_FLAG + ctx_id) != 0
        } else {
            (quad && hor_quad) || (!quad && hor_half)
        };
        let idx = if hor {
            if quad { 4 } else { 2 }
        } else if quad {
            3
        } else {
            1
        };
        let pos = self.bin(ctx::SBT_POS_FLAG) as u8;
        self.cu_mut(cu_id).sbt = idx | (pos << 4);
        vtrace!(
            "sbt_mode() pos=({},{}) sbtInfo={}",
            self.cu(cu_id).lx(),
            self.cu(cu_id).ly(),
            idx | (pos << 4)
        );
    }

    fn cu_bcw_flag(&mut self, cu_id: u32) {
        let c = self.cu(cu_id);
        if !self.si.sps.bcw
            || c.pred != Pred::Inter
            || self.si.sh.slice_type == super::ps::P_SLICE
            || c.inter_dir != 3
            || c.lw() * c.lh() < 256
        {
            return;
        }
        let wp = &self.si.sh.wp;
        let (r0, r1) = (c.ref_idx[0].max(0) as usize, c.ref_idx[1].max(0) as usize);
        if (0..3).any(|k| wp[0][r0][k].present || wp[1][r1][k].present) {
            return;
        }
        let mut idx = 0usize;
        if self.bin(ctx::BCW_IDX) != 0 {
            let num = if self.si.inter.check_ldc { 5 } else { 3 };
            idx = 1;
            for _ in 0..num - 2 {
                if self.ep() == 0 {
                    break;
                }
                idx += 1;
            }
        }
        const PARSING_ORDER: [usize; 5] = [2, 3, 1, 4, 0];
        const INTERN_FWD: [u8; 5] = [1, 2, 0, 3, 4];
        let v = INTERN_FWD[PARSING_ORDER[idx]];
        // CU::setBcwIdx: only non-merge bi-prediction keeps it
        self.cu_mut(cu_id).bcw = v;
    }

    fn mvd_coding(&mut self) -> (i32, i32) {
        let mut hor = self.bin(ctx::MVD) as i32;
        let mut ver = self.bin(ctx::MVD) as i32;
        if hor != 0 {
            hor += self.bin(ctx::MVD + 1) as i32;
        }
        if ver != 0 {
            ver += self.bin(ctx::MVD + 1) as i32;
        }
        for v in [&mut hor, &mut ver] {
            if *v != 0 {
                if *v > 1 {
                    *v += self.cabac.decode_rem_abs(1, 0, 18 - 1) as i32;
                }
                if self.ep() != 0 {
                    *v = -*v;
                }
            }
        }
        (hor, ver)
    }

    /// vvdec's `PU::getIBCMergeCandidates` (full list).
    fn ibc_candidates(&self, cu_id: u32, max: usize) -> Vec<(i32, i32)> {
        let c = self.cu(cu_id);
        let (x, y, w, h) = (c.lx(), c.ly(), c.lw(), c.lh());
        let gt4x4 = w * h > 16;
        let mut list: Vec<(i32, i32)> = Vec::new();
        let left = self
            .pic
            .get_cu_restricted(x - 1, y + h - 1, cu_id, 0, c.left, self.wpp)
            .filter(|&l| self.cu(l).pred == Pred::Ibc);
        if gt4x4 && let Some(l) = left {
            list.push(self.cu(l).bv);
        }
        if list.len() < max {
            let above = self
                .pic
                .get_cu_restricted(x + w - 1, y - 1, cu_id, 0, c.above, self.wpp)
                .filter(|&a| self.cu(a).pred == Pred::Ibc);
            if gt4x4 && let Some(a) = above {
                let ab = self.cu(a).bv;
                let same = left
                    .is_some_and(|l| self.cu(l).slice == self.cu(a).slice && self.cu(l).bv == ab);
                if !same {
                    list.push(ab);
                }
            }
        }
        let spatial = list.len();
        if list.len() < max {
            let n = self.pic.ibc_hist.len();
            for k in 1..=n {
                let cand = self.pic.ibc_hist[n - k];
                let pruned = k == 1 && gt4x4 && list[..spatial].contains(&cand);
                if !pruned {
                    list.push(cand);
                    if list.len() == max {
                        break;
                    }
                }
            }
        }
        while list.len() < max.max(2) {
            list.push((0, 0));
        }
        list
    }

    fn derive_bv(&mut self, cu_id: u32, merge: bool, mvd: (i32, i32)) {
        let bv = if merge {
            let max = self.si.sps.max_num_ibc_merge_cand as usize;
            self.ibc_candidates(cu_id, max)[self.cu(cu_id).merge_idx as usize]
        } else {
            let c = self.cu(cu_id);
            // fillIBCMvpCand: two candidates rounded to the signalled precision
            let cands = self.ibc_candidates(cu_id, self.si.sps.max_num_ibc_merge_cand as usize);
            let shift = if c.imv == 2 { 6 } else { 4 };
            let round = |v: i32| {
                let off = 1 << (shift - 1);
                ((v + off - i32::from(v >= 0)) >> shift) << shift
            };
            let p = cands[c.mvp_idx[0] as usize];
            let wrap = |v: i32| {
                let v = (v + (1 << 18)) & ((1 << 18) - 1);
                if v >= 1 << 17 { v - (1 << 18) } else { v }
            };
            (
                wrap(round(p.0) + (mvd.0 << shift)),
                wrap(round(p.1) + (mvd.1 << shift)),
            )
        };
        self.cu_mut(cu_id).bv = bv;
        let c = self.cu(cu_id);
        vtrace!(
            "ibc pos=({},{}) size={}x{} merge={} bv=({},{})",
            c.lx(),
            c.ly(),
            c.lw(),
            c.lh(),
            i32::from(merge),
            bv.0,
            bv.1
        );
        if c.lw() * c.lh() > 16 {
            let h = &mut self.pic.ibc_hist;
            if let Some(i) = h.iter().position(|&v| v == bv) {
                h.remove(i);
            } else if h.len() == 5 {
                h.remove(0);
            }
            h.push(bv);
        }
    }

    fn add_empty_tus(&mut self, cu_id: u32, part: &mut Partitioner) {
        let a = part.area().blk[0];
        if a.w > part.max_tr as i32 || a.h > part.max_tr as i32 {
            part.split(Split::MaxTr, self.pic);
            loop {
                let area = *part.area();
                self.add_tu(cu_id, &area, part.ch_type, part.tree);
                if !part.next_part(self.pic, self.wpp) {
                    break;
                }
            }
            part.exit_split(self.pic);
        } else {
            let area = *part.area();
            self.add_tu(cu_id, &area, part.ch_type, part.tree);
        }
    }

    fn add_tu(&mut self, cu_id: u32, area: &UnitArea, ch_type: usize, tree: Tree) -> u32 {
        let mut blk = area.blk;
        if self.si.dual_tree() || tree != Tree::D {
            // singleChan( chType )
            if ch_type == 0 {
                blk[1] = Area::default();
                blk[2] = Area::default();
            } else {
                blk[0] = Area::default();
            }
        }
        let ctu = self.cu(cu_id).ctu as usize;
        self.pic.ctus[ctu].num_tus += 1;
        let idx = self.pic.ctus[ctu].num_tus;
        let id = self.pic.tus.len() as u32;
        self.pic.tus.push(Tu {
            blk,
            ch_type,
            idx,
            cu: cu_id,
            ..Default::default()
        });
        self.cu_mut(cu_id).num_tu += 1;
        id
    }

    fn cu_pred_data(&mut self, cu_id: u32) -> Result<(), Error> {
        if self.cu(cu_id).pred == Pred::Intra {
            let fmt = self.pic.fmt;
            if self.cu(cu_id).ch_type == 0 {
                self.bdpcm_mode(cu_id, 0);
                self.intra_luma_pred_mode(cu_id)?;
            }
            let c = self.cu(cu_id);
            if (c.ch_type == 1 || !c.is_sep_tree(self.si.dual_tree())) && fmt.chroma != 0 {
                self.bdpcm_mode(cu_id, 1);
                self.intra_chroma_pred_mode(cu_id)?;
            }
            return Ok(());
        }
        if !self.cu(cu_id).blk[0].valid() {
            self.cu_mut(cu_id).pred = Pred::Ibc;
            return Err(Error::Unsupported("chroma intra block copy"));
        }
        self.prediction_unit(cu_id)?;
        if !self.cu(cu_id).merge {
            if self.cu(cu_id).affine {
                self.affine_amvr_mode(cu_id);
            } else if self.cu(cu_id).pred != Pred::Ibc {
                self.amvr_mode(cu_id);
            }
            self.cu_bcw_flag(cu_id);
        }
        if self.cu(cu_id).pred == Pred::Inter {
            self.derive_cu_mv(cu_id)?;
        }
        Ok(())
    }

    fn bdpcm_mode(&mut self, cu_id: u32, ch: usize) {
        let sps = self.si.sps;
        let c = self.cu(cu_id);
        let comp = if ch == 0 { 0 } else { 1 };
        let ts_max = 1i32 << sps.log2_max_ts_size;
        let b = c.blk[comp];
        let allowed = sps.bdpcm && (ch == 0 || !c.act) && b.w <= ts_max && b.h <= ts_max;
        if !allowed {
            return;
        }
        let ctx_id = if ch == 0 { 0 } else { 2 };
        let mut mode = self.bin(ctx::BDPCM_MODE + ctx_id);
        if mode != 0 {
            mode += self.bin(ctx::BDPCM_MODE + ctx_id + 1);
        }
        self.cu_mut(cu_id).bdpcm[ch] = mode as u8;
    }

    fn intra_luma_pred_mode(&mut self, cu_id: u32) -> Result<(), Error> {
        let c = self.cu(cu_id);
        if c.bdpcm[0] != 0 {
            let dir = if c.bdpcm[0] == 2 { VER } else { HOR };
            self.cu_mut(cu_id).intra_dir[0] = dir;
            return Ok(());
        }
        let sps = self.si.sps;
        if sps.mip {
            let c = self.cu(cu_id);
            let mut ctx_id = usize::from(c.left.is_some_and(|l| self.cu(l).mip))
                + usize::from(c.above.is_some_and(|a| self.cu(a).mip));
            if c.lw() > 2 * c.lh() || c.lh() > 2 * c.lw() {
                ctx_id = 3;
            }
            let v = self.bin(ctx::MIP_FLAG + ctx_id) != 0;
            self.cu_mut(cu_id).mip = v;
        }
        if self.cu(cu_id).mip {
            let t = self.ep() != 0;
            let c = self.cu(cu_id);
            let num_modes = match mip_size_id(c.lw(), c.lh()) {
                0 => 16,
                1 => 8,
                _ => 6,
            };
            let mode = self.trunc_bin(num_modes);
            let c = self.cu_mut(cu_id);
            c.mip_transposed = t;
            c.intra_dir[0] = mode as u8;
            return Ok(());
        }
        // extend_ref_line
        let c = self.cu(cu_id);
        if c.bdpcm[0] == 0 && sps.mrl {
            let mask = (1i32 << self.pic.ctu_log2) - 1;
            if c.ly() & mask != 0 {
                let mut mrl = if self.bin(ctx::MULTI_REF_LINE_IDX) == 1 {
                    1
                } else {
                    0
                };
                if mrl != 0 {
                    mrl = if self.bin(ctx::MULTI_REF_LINE_IDX + 1) == 1 {
                        2
                    } else {
                        1
                    };
                }
                self.cu_mut(cu_id).mrl = mrl;
            }
        }
        // isp_mode
        let c = self.cu(cu_id);
        if c.mrl == 0 && sps.isp && c.bdpcm[0] == 0 && !c.act {
            let allowed = can_use_isp(c.blk[0].w, c.blk[0].h, 1 << sps.log2_max_tb_size);
            if allowed != 0 && self.bin(ctx::ISP_MODE) != 0 {
                let isp = match allowed {
                    1 => HOR_ISP,
                    2 => VER_ISP,
                    _ => 1 + self.bin(ctx::ISP_MODE + 1) as u8,
                };
                self.cu_mut(cu_id).isp = isp;
            }
        }
        let c = self.cu(cu_id);
        let mpm_flag = if c.mrl != 0 {
            true
        } else {
            self.bin(ctx::I_PRED_MODE0) != 0
        };
        let mut mpm = self.intra_mpms(cu_id);
        if mpm_flag {
            let c = self.cu(cu_id);
            let ctx_id = if c.isp == NOT_ISP { 1 } else { 0 };
            let mut idx = if c.mrl == 0 {
                self.bin(ctx::INTRA_LUMA_PLANAR_FLAG + ctx_id)
            } else {
                1
            };
            if idx != 0 {
                while idx < 5 && self.ep() != 0 {
                    idx += 1;
                }
            }
            self.cu_mut(cu_id).intra_dir[0] = mpm[idx as usize];
        } else {
            let mut mode = self.trunc_bin(67 - 6);
            mpm.sort_unstable();
            for m in mpm {
                if mode >= u32::from(m) {
                    mode += 1;
                }
            }
            self.cu_mut(cu_id).intra_dir[0] = mode as u8;
        }
        let c = self.cu(cu_id);
        vtrace!(
            "intra_luma_pred_modes() idx=0 pos=({},{}) mode={}",
            c.lx(),
            c.ly(),
            c.intra_dir[0]
        );
        Ok(())
    }

    fn intra_dir_luma(&self, cu_id: u32) -> u8 {
        let c = self.cu(cu_id);
        if c.mip { PLANAR } else { c.intra_dir[0] }
    }

    fn intra_mpms(&self, cu_id: u32) -> [u8; 6] {
        let c = self.cu(cu_id);
        let b = c.blk[0];
        let (rt_x, rt_y) = (b.x + b.w - 1, b.y);
        let (lb_x, lb_y) = (b.x, b.y + b.h - 1);
        let mut left_dir = PLANAR as i32;
        let mut above_dir = PLANAR as i32;
        if let Some(l) = self
            .pic
            .get_cu_restricted(lb_x - 1, lb_y, cu_id, 0, c.left, self.wpp)
            && self.cu(l).pred == Pred::Intra
        {
            left_dir = self.intra_dir_luma(l) as i32;
        }
        if let Some(a) = self
            .pic
            .get_cu_restricted(rt_x, rt_y - 1, cu_id, 0, c.above, self.wpp)
            && self.cu(a).pred == Pred::Intra
        {
            let ac = self.cu(a);
            let same_ctu = (ac.lx() >> self.pic.ctu_log2) == (c.lx() >> self.pic.ctu_log2)
                && (ac.ly() >> self.pic.ctu_log2) == (c.ly() >> self.pic.ctu_log2);
            if same_ctu {
                above_dir = self.intra_dir_luma(a) as i32;
            }
        }
        let offset = 67 - 6;
        let m = offset + 3;
        let mut mpm = [
            PLANAR as i32,
            DC as i32,
            VER as i32,
            HOR as i32,
            VER as i32 - 4,
            VER as i32 + 4,
        ];
        if left_dir == above_dir {
            if left_dir > DC as i32 {
                mpm = [
                    PLANAR as i32,
                    left_dir,
                    ((left_dir + offset) % m) + 2,
                    ((left_dir - 1) % m) + 2,
                    ((left_dir + offset - 1) % m) + 2,
                    (left_dir % m) + 2,
                ];
            }
        } else if left_dir > DC as i32 && above_dir > DC as i32 {
            mpm[0] = PLANAR as i32;
            mpm[1] = left_dir;
            mpm[2] = above_dir;
            let (max_i, min_i) = if mpm[1] > mpm[2] { (1, 2) } else { (2, 1) };
            let (mx, mn) = (mpm[max_i], mpm[min_i]);
            if mx - mn == 1 {
                mpm[3] = ((mn + offset) % m) + 2;
                mpm[4] = ((mx - 1) % m) + 2;
                mpm[5] = ((mn + offset - 1) % m) + 2;
            } else if mx - mn >= 62 {
                mpm[3] = ((mn - 1) % m) + 2;
                mpm[4] = ((mx + offset) % m) + 2;
                mpm[5] = (mn % m) + 2;
            } else if mx - mn == 2 {
                mpm[3] = ((mn - 1) % m) + 2;
                mpm[4] = ((mn + offset) % m) + 2;
                mpm[5] = ((mx - 1) % m) + 2;
            } else {
                mpm[3] = ((mn + offset) % m) + 2;
                mpm[4] = ((mn - 1) % m) + 2;
                mpm[5] = ((mx + offset) % m) + 2;
            }
        } else if left_dir + above_dir >= 2 {
            let mx = left_dir.max(above_dir);
            mpm = [
                PLANAR as i32,
                mx,
                ((mx + offset) % m) + 2,
                ((mx - 1) % m) + 2,
                ((mx + offset - 1) % m) + 2,
                (mx % m) + 2,
            ];
        }
        mpm.map(|v| v as u8)
    }

    fn check_cclm_allowed(&self, cu_id: u32) -> bool {
        let c = self.cu(cu_id);
        let dual = self.si.dual_tree();
        if !dual {
            return true;
        }
        let ctu_size = 1u32 << self.pic.ctu_log2;
        if ctu_size <= 32 {
            return true;
        }
        let depth64 = if ctu_size == 128 { 1 } else { 0 };
        let split_at = |d: u32| -> Split {
            if d >= c.depth {
                return Split::None;
            }
            match (c.split_series >> (d * 3)) & 7 {
                1 => Split::Quad,
                2 => Split::Horz,
                3 => Split::Vert,
                4 => Split::TriH,
                5 => Split::TriV,
                _ => Split::None,
            }
        };
        let s1 = split_at(depth64);
        let s2 = split_at(depth64 + 1);
        let mut allow = s1 == Split::Quad
            || (s1 == Split::Horz && s2 == Split::Vert)
            || s1 == Split::None
            || (s1 == Split::Horz && s2 == Split::None);
        if allow {
            let cb = c.blk[1];
            let (lx, ly) = (cb.x << self.pic.fmt.sx, cb.y << self.pic.fmt.sy);
            if let Some(col) = self.pic.get_cu(lx, ly, 0) {
                let cc = self.cu(col);
                if (cc.depth > depth64 && cc.qt_depth == depth64)
                    || (cc.depth == depth64 && cc.isp != 0)
                {
                    allow = false;
                }
            }
        }
        allow
    }

    fn intra_chroma_pred_mode(&mut self, cu_id: u32) -> Result<(), Error> {
        let c = self.cu(cu_id);
        if c.bdpcm[1] != 0 {
            let dir = if c.bdpcm[1] == 2 { VER } else { HOR };
            self.cu_mut(cu_id).intra_dir[1] = dir;
            return Ok(());
        }
        if c.act {
            self.cu_mut(cu_id).intra_dir[1] = DM_CHROMA;
            return Ok(());
        }
        if self.si.sps.cclm && self.check_cclm_allowed(cu_id) && self.bin(ctx::CCLM_MODE_FLAG) != 0
        {
            let mut symbol = self.bin(ctx::CCLM_MODE_IDX);
            if symbol != 0 {
                symbol += self.ep();
            }
            self.cu_mut(cu_id).intra_dir[1] = [LM_CHROMA, MDLM_L, MDLM_T][symbol as usize];
            return Ok(());
        }
        if self.bin(ctx::I_PRED_MODE1) == 0 {
            self.cu_mut(cu_id).intra_dir[1] = DM_CHROMA;
            return Ok(());
        }
        let cand = self.cabac.decode_bypass_bins(2) as usize;
        let mut list = [PLANAR, VER, HOR, DC, LM_CHROMA, MDLM_L, MDLM_T, DM_CHROMA];
        if !self.is_dm_chroma_mip(cu_id) {
            let luma_mode = recon::co_located_intra_luma_mode(self.pic, self.si, cu_id);
            for m in list.iter_mut().take(4) {
                if luma_mode == *m {
                    *m = VDIA;
                    break;
                }
            }
        }
        self.cu_mut(cu_id).intra_dir[1] = list[cand];
        Ok(())
    }

    fn is_dm_chroma_mip(&self, cu_id: u32) -> bool {
        recon::is_dm_chroma_mip(self.pic, self.si, cu_id)
    }

    fn cu_residual(
        &mut self,
        cu_id: u32,
        part: &mut Partitioner,
        cu_ctx: &mut CuCtx,
    ) -> Result<(), Error> {
        if self.cu(cu_id).pred != Pred::Intra {
            let root = if self.cu(cu_id).merge {
                true
            } else {
                self.bin(ctx::QT_ROOT_CBF) != 0
            };
            self.cu_mut(cu_id).root_cbf = root;
            if root {
                self.sbt_mode(cu_id);
            }
            if !root {
                self.add_empty_tus(cu_id, part);
                return Ok(());
            }
            self.adaptive_color_transform(cu_id);
        }
        self.cu_mut(cu_id).root_cbf = true;
        cu_ctx.violates_lfnst = [false; 2];
        cu_ctx.lfnst_last_scan_pos = false;
        cu_ctx.violates_mts = false;
        cu_ctx.mts_last_scan_pos = false;
        self.transform_tree(cu_id, part, cu_ctx)?;
        self.residual_lfnst_mode(cu_id, cu_ctx);
        self.mts_idx(cu_id, cu_ctx);
        let c = self.cu(cu_id);
        let mut root = false;
        for comp in 0..3 {
            if c.blk[comp].valid() {
                root |= c.plane_cbf[comp];
            }
        }
        self.cu_mut(cu_id).root_cbf = root;
        Ok(())
    }

    fn transform_tree(
        &mut self,
        cu_id: u32,
        part: &mut Partitioner,
        cu_ctx: &mut CuCtx,
    ) -> Result<(), Error> {
        let area = *part.area();
        let mut split = area.blk[0].w > part.max_tr as i32 || area.blk[0].h > part.max_tr as i32;
        let c = self.cu(cu_id);
        let isp_type = if c.isp != NOT_ISP && part.ch_type == 0 {
            if c.isp == HOR_ISP {
                Split::IspHorz
            } else {
                Split::IspVert
            }
        } else {
            Split::None
        };
        let sbt = self.cu(cu_id).sbt;
        split |= (sbt != 0 || isp_type != Split::None) && part.tr_depth == 0;
        if split {
            if isp_type == Split::None && sbt == 0 {
                part.split(Split::MaxTr, self.pic);
            } else if isp_type != Split::None {
                part.split(isp_type, self.pic);
            } else {
                // CU::getSbtTuSplit
                part.split(
                    Split::Sbt(((sbt & 15) - 1) * 2 + ((sbt >> 4) & 3)),
                    self.pic,
                );
            }
            loop {
                self.transform_tree(cu_id, part, cu_ctx)?;
                if !part.next_part(self.pic, self.wpp) {
                    break;
                }
            }
            part.exit_split(self.pic);
            Ok(())
        } else {
            let tu = self.add_tu(cu_id, &area, part.ch_type, part.tree);
            self.transform_unit(tu, cu_id, part, cu_ctx)
        }
    }

    fn cbf_comp(&mut self, cu_id: u32, comp: usize, prev_cbf: bool, use_isp: bool) -> bool {
        let c = self.cu(cu_id);
        let base = [ctx::QT_CBF0, ctx::QT_CBF1, ctx::QT_CBF2][comp];
        if (comp == 0 && c.bdpcm[0] != 0) || (comp != 0 && c.bdpcm[1] != 0) {
            let inc = if comp == 2 { 2 } else { 1 };
            return self.bin(base + inc) != 0;
        }
        let inc = if use_isp && comp == 0 {
            2 + usize::from(prev_cbf)
        } else if comp == 2 {
            usize::from(prev_cbf)
        } else {
            0
        };
        self.bin(base + inc) != 0
    }

    fn transform_unit(
        &mut self,
        tu_id: u32,
        cu_id: u32,
        part: &mut Partitioner,
        cu_ctx: &mut CuCtx,
    ) -> Result<(), Error> {
        let area = *part.area();
        let tr_depth = part.tr_depth;
        let fmt = self.pic.fmt;
        let dual = self.si.dual_tree();
        let c = self.cu(cu_id).clone();
        let chroma_cbf_isp = fmt.chroma != 0 && area.blk[1].valid() && c.isp != NOT_ISP;
        // TU::checkTuNoResidual
        let sbt_pos = (c.sbt >> 4) & 3;
        let part_idx = part.part_idx();
        let no_residual = (c.sbt & 15) != 0
            && ((sbt_pos == 0 && part_idx == 1) || (sbt_pos == 1 && part_idx == 0));
        let mut cbf_cb = false;
        let mut cbf_cr = false;
        if fmt.chroma != 0
            && area.blk[1].valid()
            && (!c.is_sep_tree(dual) || part.ch_type == 1)
            && (c.isp == NOT_ISP || chroma_cbf_isp)
            && !(c.sbt != 0 && no_residual)
        {
            cbf_cb = self.cbf_comp(cu_id, 1, false, false);
            cbf_cr = self.cbf_comp(cu_id, 2, cbf_cb, false);
        }
        let sig_chroma = fmt.chroma != 0 && (cbf_cb || cbf_cr);
        if part.ch_type == 0 {
            let cbf_y = if c.pred != Pred::Intra && tr_depth == 0 && !sig_chroma {
                true
            } else if c.sbt != 0 && no_residual {
                false
            } else if c.sbt != 0 && !sig_chroma {
                true
            } else if c.isp != NOT_ISP {
                let luma_inferred_act = c.act && tr_depth == 0 && !sig_chroma;
                let mut last_inferred = luma_inferred_act;
                let tu_blk = self.pic.tus[tu_id as usize].blk[0];
                let n_tus = if c.isp == HOR_ISP {
                    c.lh() >> log2(tu_blk.h)
                } else {
                    c.lw() >> log2(tu_blk.w)
                };
                if part.part_idx() as i32 == n_tus - 1 {
                    let mut root_so_far = false;
                    for t in c.first_tu..tu_id {
                        root_so_far |= self.pic.tus[t as usize].cbf(0);
                    }
                    if !root_so_far {
                        last_inferred = true;
                    }
                }
                if !last_inferred {
                    let prev = self.prev_tu_cbf(tu_id, 0);
                    self.cbf_comp(cu_id, 0, prev, true)
                } else {
                    true
                }
            } else {
                self.cbf_comp(cu_id, 0, false, false)
            };
            self.pic.tus[tu_id as usize].set_cbf(0, cbf_y);
        }
        if fmt.chroma != 0 && (c.isp == NOT_ISP || chroma_cbf_isp) {
            let t = &mut self.pic.tus[tu_id as usize];
            t.set_cbf(1, cbf_cb);
            t.set_cbf(2, cbf_cr);
        }
        {
            let t = self.pic.tus[tu_id as usize].clone();
            let cm = self.cu_mut(cu_id);
            for comp in 0..3 {
                cm.plane_cbf[comp] |= t.cbf(comp);
            }
        }
        let tu_b = self.pic.tus[tu_id as usize].blk;
        let luma_only = fmt.chroma == 0 || !tu_b[1].valid();
        let cbf_luma = self.pic.tus[tu_id as usize].cbf(0);
        let cbf_chroma = !luma_only && (cbf_cb || cbf_cr);
        if c.lw() > 64 || c.lh() > 64 || cbf_luma || cbf_chroma {
            if self.si.pps.cu_qp_delta
                && !cu_ctx.dqp_coded
                && (!c.is_sep_tree(dual) || self.pic.tus[tu_id as usize].ch_type == 0)
            {
                let qp = self.cu_qp_delta(cu_ctx.qp)?;
                self.cu_mut(cu_id).qp = qp;
                cu_ctx.qp = qp;
                cu_ctx.dqp_coded = true;
            }
            if !c.is_sep_tree(dual) || self.pic.tus[tu_id as usize].ch_type == 1 {
                let (cw, ch) = if !c.is_sep_tree(dual) {
                    (c.lw(), c.lh())
                } else {
                    (c.blk[1].w, c.blk[1].h)
                };
                if self.si.sh.cu_chroma_qp_offset_enabled
                    && (cw > 64 || ch > 64 || cbf_chroma)
                    && !cu_ctx.cqp_adj_coded
                {
                    self.cu_chroma_qp_offset(cu_id);
                    cu_ctx.cqp_adj_coded = true;
                }
            }
            if !luma_only {
                let mask = (if cbf_cb { 2 } else { 0 }) + (if cbf_cr { 1 } else { 0 });
                if self.si.sps.joint_cbcr && ((c.pred == Pred::Intra && mask != 0) || mask == 3) {
                    let j = if self.bin(ctx::JOINT_CB_CR_FLAG + mask - 1) != 0 {
                        mask as u8
                    } else {
                        0
                    };
                    self.pic.tus[tu_id as usize].joint = j;
                    if j != 0 {
                        let cm = self.cu_mut(cu_id);
                        cm.plane_cbf[1] = true;
                        cm.plane_cbf[2] = true;
                    }
                }
            }
            if cbf_luma {
                self.residual_coding(tu_id, 0, cu_ctx)?;
            }
            if !luma_only {
                for comp in 1..3 {
                    if self.pic.tus[tu_id as usize].cbf(comp) {
                        self.residual_coding(tu_id, comp, cu_ctx)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn prev_tu_cbf(&self, tu_id: u32, comp: usize) -> bool {
        let t = &self.pic.tus[tu_id as usize];
        let c = self.cu(t.cu);
        if tu_id == c.first_tu {
            return false;
        }
        let p = &self.pic.tus[tu_id as usize - 1];
        if p.cu != t.cu || !p.blk[comp].valid() {
            return false;
        }
        p.cbf(comp)
    }

    fn cu_qp_delta(&mut self, pred_qp: i32) -> Result<i32, Error> {
        let mut dqp = self.unary_max_symbol(ctx::DELTAQP, ctx::DELTAQP + 1, 5) as i32;
        if dqp >= 5 {
            dqp += self.exp_golomb_eqprob(0)? as i32;
        }
        let mut qp = pred_qp;
        if dqp > 0 {
            if self.ep() != 0 {
                dqp = -dqp;
            }
            let off = self.si.sps.qp_bd_offset;
            if dqp < -(32 + off / 2) || dqp > 31 + off / 2 {
                return Err(Error::Invalid("CuQpDeltaVal"));
            }
            qp = ((pred_qp + dqp + 64 + 2 * off) % (64 + off)) - off;
        }
        Ok(qp)
    }

    fn cu_chroma_qp_offset(&mut self, cu_id: u32) {
        let len = self.si.pps.chroma_qp_offset_list.len() as u32 - 1;
        let mut adj = self.bin(ctx::CHROMA_QP_ADJ_FLAG);
        if adj != 0 && len > 1 {
            adj += self.unary_max_symbol(ctx::CHROMA_QP_ADJ_IDC, ctx::CHROMA_QP_ADJ_IDC, len - 1);
        }
        self.cu_mut(cu_id).chroma_qp_adj = adj as u8;
        self.chroma_qp_adj = adj as u8;
    }

    fn is_ts_allowed(&self, tu_id: u32, comp: usize) -> bool {
        let sps = self.si.sps;
        let t = &self.pic.tus[tu_id as usize];
        let c = self.cu(t.cu);
        let max = 1i32 << sps.log2_max_ts_size;
        let mut ok = sps.transform_skip;
        ok &= c.isp == NOT_ISP || comp != 0;
        ok &= !(c.bdpcm[0] != 0 && comp == 0);
        ok &= !(c.bdpcm[1] != 0 && comp != 0);
        ok &= t.blk[comp].w <= max && t.blk[comp].h <= max;
        ok &= c.sbt == 0;
        ok
    }

    fn residual_coding(
        &mut self,
        tu_id: u32,
        comp: usize,
        cu_ctx: &mut CuCtx,
    ) -> Result<(), Error> {
        let t = self.pic.tus[tu_id as usize].clone();
        let c = self.cu(t.cu).clone();
        let b = t.blk[comp];
        vtrace!(
            "residual_coding() etype={} pos=({},{}) size={}x{}",
            comp,
            b.x,
            b.y,
            b.w,
            b.h
        );
        if comp == 2 && t.joint == 3 {
            return Ok(());
        }
        // ts_flag
        let mut ts = ((c.bdpcm[0] != 0 && comp == 0) || (c.bdpcm[1] != 0 && comp != 0))
            || t.mts[comp] == MTS_SKIP;
        if self.is_ts_allowed(tu_id, comp) {
            ts = self.bin(ctx::MTS_INDEX + if comp == 0 { 4 } else { 5 }) != 0;
        }
        self.pic.tus[tu_id as usize].mts[comp] = if ts { MTS_SKIP } else { MTS_DCT2 };
        let b = t.blk[comp];
        if ts && !self.si.sh.ts_residual_coding_disabled {
            return self.residual_coding_ts(tu_id, comp, &c);
        }
        let ch = if comp == 0 { 0 } else { 1 };
        let mut cc = CoeffCtx::new(
            b.w,
            b.h,
            ch,
            self.si.sh.sign_data_hiding,
            comp == 0,
            &c,
            t.mts[comp],
            false,
            self.si.sps.mts && c.sbt != 0,
        );
        // last_sig_coeff
        let last = self.last_sig_coeff(&cc, comp, &c);
        cc.scan_pos_last = last;
        let mts = self.pic.tus[tu_id as usize].mts[comp];
        if mts != MTS_SKIP && b.h >= 4 && b.w >= 4 {
            let max_lfnst = if (b.h == 4 && b.w == 4) || (b.h == 8 && b.w == 8) {
                7
            } else {
                15
            };
            cu_ctx.violates_lfnst[ch] |= last > max_lfnst;
            cu_ctx.lfnst_last_scan_pos |= last >= 1;
        }
        if comp == 0 && mts != MTS_SKIP {
            cu_ctx.mts_last_scan_pos |= last >= 1;
        }
        let dep_quant = self.si.sh.dep_quant;
        let trans_tab: u32 = if dep_quant { 32040 } else { 0 };
        let mut state = 0u32;
        let n = (b.w * b.h) as usize;
        let mut coeff = vec![0i32; n];
        let mut sig_list: Vec<(usize, u32, u32, u32)> = Vec::new(); // (numSig, signPattern, sub1Pattern, startIdx)
        let mut sig_pos: Vec<i32> = Vec::with_capacity(n);
        let mut max_x = 0;
        let mut max_y = 0;
        let skip_pre = comp == 0 && self.si.sps.mts && c.sbt != 0 && b.w <= 32 && b.h <= 32;
        let mut sub_set = (last >> cc.log2_cg_size) as i32;
        while sub_set >= 0 {
            cc.init_subblock(sub_set as usize);
            if skip_pre
                && ((cc.height == 32 && cc.cg_y >= (16 >> cc.log2_cg_h))
                    || (cc.width == 32 && cc.cg_x >= (16 >> cc.log2_cg_w)))
            {
                sub_set -= 1;
                continue;
            }
            let start = sig_pos.len();
            let (num, sign, sub1) = self.residual_coding_subblock(
                &mut cc,
                &mut coeff,
                trans_tab,
                &mut state,
                &mut sig_pos,
            );
            if num > 0 {
                sig_list.push((num, sign, sub1, start as u32));
                max_x = max_x.max(cc.cg_x);
                max_y = max_y.max(cc.cg_y);
            }
            if comp == 0 && cc.is_sig_group() && (cc.cg_y > 3 || cc.cg_x > 3) {
                cu_ctx.violates_mts = true;
            }
            sub_set -= 1;
        }
        if cc.bdpcm {
            max_x = b.w;
            max_y = b.h;
        } else {
            max_x = (max_x + 1) << cc.log2_cg_w;
            max_y = (max_y + 1) << cc.log2_cg_h;
        }
        let dq = dep_quant && mts != MTS_SKIP;
        let mut out = vec![0i32; n];
        for &(num, sign, sub1, start) in sig_list.iter() {
            let mut sign = sign;
            let mut sub1 = sub1;
            // Signs were read in reverse sigPos order within each subblock.
            for k in 0..num {
                let pos = sig_pos[start as usize + num - 1 - k] as usize;
                let abs = if dq {
                    coeff[pos] * 2 - (sub1 & 1) as i32
                } else {
                    coeff[pos]
                };
                out[pos] = if sign & 1 != 0 { -abs } else { abs };
                sign >>= 1;
                sub1 >>= 1;
            }
        }
        let tm = &mut self.pic.tus[tu_id as usize];
        tm.coeff[comp] = out;
        if last == 0 {
            tm.max_scan[comp] = (0, 0);
        } else {
            tm.max_scan[comp] = (max_x - 1, max_y - 1);
        }
        Ok(())
    }

    fn last_sig_coeff(&mut self, cc: &CoeffCtx, comp: usize, c: &Cu) -> usize {
        let mut max_x = cc.max_last_x;
        let mut max_y = cc.max_last_y;
        if comp == 0 && self.si.sps.mts && c.sbt != 0 && cc.width <= 32 && cc.height <= 32 {
            if cc.width == 32 {
                max_x = GROUP_IDX[15];
            }
            if cc.height == 32 {
                max_y = GROUP_IDX[15];
            }
        }
        let (lx_ctx, ly_ctx) = if cc.ch == 0 {
            (ctx::LASTX0, ctx::LASTY0)
        } else {
            (ctx::LASTX1, ctx::LASTY1)
        };
        let mut px = 0u32;
        while px < max_x {
            if self.bin(lx_ctx + (cc.last_off_x + (px >> cc.last_shift_x)) as usize) == 0 {
                break;
            }
            px += 1;
        }
        let mut py = 0u32;
        while py < max_y {
            if self.bin(ly_ctx + (cc.last_off_y + (py >> cc.last_shift_y)) as usize) == 0 {
                break;
            }
            py += 1;
        }
        max_x = 0;
        max_y = 0;
        let _ = (max_x, max_y);
        if px > 3 {
            let count = (px - 2) >> 1;
            let mut tmp = 0;
            for i in (0..count).rev() {
                tmp += self.ep() << i;
            }
            px = MIN_IN_GROUP[px as usize] + tmp;
        }
        if py > 3 {
            let count = (py - 2) >> 1;
            let mut tmp = 0;
            for i in (0..count).rev() {
                tmp += self.ep() << i;
            }
            py = MIN_IN_GROUP[py as usize] + tmp;
        }
        let blk = px as usize + py as usize * cc.width as usize;
        let n = (cc.width * cc.height) as usize;
        for s in 0..n - 1 {
            if cc.scan[s] as usize == blk {
                return s;
            }
        }
        n - 1
    }

    fn residual_coding_subblock(
        &mut self,
        cc: &mut CoeffCtx,
        coeff: &mut [i32],
        trans_tab: u32,
        state: &mut u32,
        sig_pos: &mut Vec<i32>,
    ) -> (usize, u32, u32) {
        let min_sub = cc.min_sub_pos as i32;
        let is_last = cc.is_last();
        let first_sig = if is_last {
            cc.scan_pos_last as i32
        } else {
            cc.max_sub_pos as i32
        };
        let mut next = first_sig;
        let mut sig_group = is_last || min_sub == 0;
        if !sig_group {
            sig_group = self.bin(cc.sig_group_ctx) != 0;
        }
        if sig_group {
            cc.set_sig_group();
        } else {
            return (0, 0, 0);
        }
        let mut gt1_pos: [usize; 16] = [0; 16];
        let mut num_gt1 = 0usize;
        let infer_sig = if next != cc.scan_pos_last as i32 {
            if cc.sub_set != 0 { min_sub } else { -1 }
        } else {
            next
        };
        let mut first_nz = next;
        let mut last_nz: i32 = -1;
        let mut num_nz = 0usize;
        let mut rem_bins = cc.reg_bin_limit;
        let mut gt2_mask = 0u32;
        let mut state_val = 0u32;
        let start_len = sig_pos.len();
        while next >= min_sub && rem_bins >= 4 {
            let blk = cc.scan[next as usize] as usize;
            let mut sig = num_nz == 0 && next == infer_sig;
            if !sig {
                let ctx_id = cc.sig_ctx(blk, *state);
                sig = self.bin(ctx_id) != 0;
                rem_bins -= 1;
            }
            if sig {
                let off = cc.ctx_offset_abs();
                state_val = ((*state >> 1) & 1) | (state_val << 1);
                sig_pos.push(blk as i32);
                num_nz += 1;
                first_nz = next;
                last_nz = last_nz.max(next);
                let gt1 = self.bin(cc.gtx1_ctx(off));
                rem_bins -= 1;
                let abs;
                if gt1 != 0 {
                    let par = self.bin(cc.par_ctx(off));
                    num_gt1 += 1;
                    rem_bins -= 1;
                    let gt2 = self.bin(cc.gtx2_ctx(off));
                    gt2_mask |= gt2 << (num_gt1 - 1);
                    rem_bins -= 1;
                    gt1_pos[num_gt1 - 1] = blk;
                    abs = 2 + par + (gt2 << 1);
                    *state = (trans_tab >> ((*state << 2) + (par << 1))) & 3;
                } else {
                    abs = 1;
                    *state = (trans_tab >> ((*state << 2) + 2)) & 3;
                }
                cc.abs_val_1st_pass(blk, coeff, abs as i32);
            } else {
                *state = (trans_tab >> (*state << 2)) & 3;
            }
            next -= 1;
        }
        cc.reg_bin_limit = rem_bins;
        for (k, &pos) in gt1_pos.iter().take(num_gt1).enumerate() {
            if (gt2_mask >> k) & 1 != 0 {
                let sum = cc.template_abs_sum(pos, coeff, 4);
                let rice = GO_RICE_PARS[sum as usize];
                let rem = self.cabac.decode_rem_abs(rice, 5, cc.max_log2_range);
                coeff[pos] += (rem << 1) as i32;
            }
        }
        while next >= min_sub {
            let sub1 = (*state >> 1) & 1;
            let blk = cc.scan[next as usize] as usize;
            let sum = cc.template_abs_sum(blk, coeff, 0);
            let rice = GO_RICE_PARS[sum as usize];
            let pos0 = (if *state < 2 { 1 } else { 2 }) << rice;
            let rem = self.cabac.decode_rem_abs(rice, 5, cc.max_log2_range);
            let tc = if rem == pos0 {
                0
            } else if rem < pos0 {
                rem + 1
            } else {
                rem
            };
            *state = (trans_tab >> ((*state << 2) + ((tc & 1) << 1))) & 3;
            if tc != 0 {
                coeff[blk] = tc as i32;
                state_val = sub1 | (state_val << 1);
                sig_pos.push(blk as i32);
                num_nz += 1;
                first_nz = next;
                last_nz = last_nz.max(next);
            }
            next -= 1;
        }
        let hide = cc.sign_hiding && (last_nz - first_nz >= 4);
        let num_signs = if hide { num_nz - 1 } else { num_nz };
        let mut sign = self.cabac.decode_bypass_bins(num_signs as u32);
        if num_nz > num_signs {
            let mut sum = 0i32;
            for &p in &sig_pos[start_len..] {
                sum += coeff[p as usize];
            }
            sign <<= 1;
            sign += (sum & 1) as u32;
        }
        (num_nz, sign, state_val)
    }

    fn residual_coding_ts(&mut self, tu_id: u32, comp: usize, c: &Cu) -> Result<(), Error> {
        let t = &self.pic.tus[tu_id as usize];
        let b = t.blk[comp];
        let ch = if comp == 0 { 0 } else { 1 };
        let mut cc = CoeffCtx::new(b.w, b.h, ch, false, comp == 0, c, MTS_SKIP, true, false);
        let n = (b.w * b.h) as usize;
        let mut coeff = vec![0i32; n];
        let mut out = vec![0i32; n];
        cc.num_ctx_bins = ((n * 7) >> 2) as i32;
        let mut max_x = 0;
        let mut max_y = 0;
        let last_sub = (n - 1) >> cc.log2_cg_size;
        for sub in 0..=last_sub {
            cc.init_subblock(sub);
            self.residual_coding_subblock_ts(&mut cc, &mut coeff, &mut out, &mut max_x, &mut max_y);
        }
        let tm = &mut self.pic.tus[tu_id as usize];
        tm.coeff[comp] = out;
        tm.max_scan[comp] = if cc.bdpcm { (b.w, b.h) } else { (max_x, max_y) };
        Ok(())
    }

    fn residual_coding_subblock_ts(
        &mut self,
        cc: &mut CoeffCtx,
        coeff: &mut [i32],
        out: &mut [i32],
        max_x: &mut i32,
        max_y: &mut i32,
    ) {
        let min_sub = cc.max_sub_pos as i32; // vvdec swaps the names here
        let first_sig = cc.min_sub_pos as i32;
        let mut next = first_sig;
        let mut sign_pattern = 0u32;
        let mut sig_group = cc.sub_set == cc.last_sub_set() && cc.none_sig_group();
        if !sig_group {
            sig_group = self.bin(cc.sig_group_ctx_ts) != 0;
        }
        if sig_group {
            cc.set_sig_group();
        } else {
            return;
        }
        let infer = min_sub;
        let mut num_nz = 0usize;
        let mut sig_blk = [0usize; 16];
        let mut last_pass1: i32 = -1;
        let mut last_pass2: i32 = -1;
        let w = cc.width as usize;
        while next <= min_sub && cc.num_ctx_bins >= 4 {
            let blk = cc.scan[next as usize] as usize;
            let mut sig = u32::from(num_nz == 0 && next == infer);
            if sig == 0 {
                let (px, py) = (blk % w, blk / w);
                let mut num_pos = 0;
                if px > 0 && coeff[blk - 1] != 0 {
                    num_pos += 1;
                }
                if py > 0 && coeff[blk - w] != 0 {
                    num_pos += 1;
                }
                sig = self.bin(ctx::TS_SIG_FLAG + num_pos);
                cc.num_ctx_bins -= 1;
            }
            if sig != 0 {
                let (px, py) = (blk % w, blk / w);
                let right = if px > 0 { coeff[blk - 1] } else { 0 };
                let below = if py > 0 { coeff[blk - w] } else { 0 };
                let mut sign_ctx = if (right == 0 && below == 0) || right * below < 0 {
                    0
                } else if right >= 0 && below >= 0 {
                    1
                } else {
                    2
                };
                if cc.bdpcm {
                    sign_ctx += 3;
                }
                let s = self.bin(ctx::TS_RESIDUAL_SIGN + sign_ctx);
                cc.num_ctx_bins -= 1;
                sign_pattern += s << num_nz;
                sig_blk[num_nz] = blk;
                num_nz += 1;
                let num_pos = if cc.bdpcm {
                    3
                } else {
                    usize::from(px > 0 && coeff[blk - 1] != 0)
                        + usize::from(py > 0 && coeff[blk - w] != 0)
                };
                let gt1 = self.bin(ctx::TS_LRG1_FLAG + num_pos);
                cc.num_ctx_bins -= 1;
                let mut par = 0;
                if gt1 != 0 {
                    par = self.bin(ctx::TS_PAR_FLAG);
                    cc.num_ctx_bins -= 1;
                }
                coeff[blk] = (if s != 0 { -1 } else { 1 }) * (1 + par + gt1) as i32;
            }
            last_pass1 = next;
            next += 1;
        }
        let mut scan = first_sig;
        while scan <= min_sub && cc.num_ctx_bins >= 4 {
            let blk = cc.scan[scan as usize] as usize;
            let mut cutoff = 2;
            for _ in 0..4 {
                if coeff[blk] < 0 {
                    coeff[blk] = -coeff[blk];
                }
                if coeff[blk] >= cutoff {
                    let g = self.bin(ctx::TS_GTX_FLAG + (cutoff >> 1) as usize);
                    coeff[blk] += (g << 1) as i32;
                    cc.num_ctx_bins -= 1;
                }
                cutoff += 2;
            }
            last_pass2 = scan;
            scan += 1;
        }
        for scan in first_sig..=min_sub {
            let blk = cc.scan[scan as usize] as usize;
            let cutoff = if scan <= last_pass2 {
                10
            } else if scan <= last_pass1 {
                2
            } else {
                0
            };
            if coeff[blk] < 0 {
                coeff[blk] = -coeff[blk];
            }
            if coeff[blk] >= cutoff {
                let rem = self.cabac.decode_rem_abs(1, 5, cc.max_log2_range);
                coeff[blk] += if scan <= last_pass1 {
                    (rem << 1) as i32
                } else {
                    rem as i32
                };
                if coeff[blk] != 0 && scan > last_pass1 {
                    let s = self.ep();
                    sign_pattern += s << num_nz;
                    sig_blk[num_nz] = blk;
                    num_nz += 1;
                }
            }
            if !cc.bdpcm && cutoff != 0 && coeff[blk] > 0 {
                let (px, py) = (blk % w, blk / w);
                let right = if px > 0 { coeff[blk - 1] } else { 0 };
                let below = if py > 0 { coeff[blk - w] } else { 0 };
                let pred1 = right.abs().max(below.abs());
                let abs = coeff[blk];
                coeff[blk] = if abs == 1 && pred1 > 0 {
                    pred1
                } else {
                    abs - i32::from(abs <= pred1)
                };
            }
        }
        for &blk in sig_blk.iter().take(num_nz) {
            let abs = coeff[blk];
            let (px, py) = ((blk % w) as i32, (blk / w) as i32);
            *max_x = (*max_x).max(px);
            *max_y = (*max_y).max(py);
            let v = if sign_pattern & 1 != 0 { -abs } else { abs };
            out[blk] = v;
            coeff[blk] = v;
            sign_pattern >>= 1;
        }
    }

    fn residual_lfnst_mode(&mut self, cu_id: u32, cu_ctx: &CuCtx) {
        let sps = self.si.sps;
        let c = self.cu(cu_id).clone();
        if !sps.lfnst || c.pred != Pred::Intra {
            return;
        }
        let dual = self.si.dual_tree();
        let sep = c.is_sep_tree(dual);
        let ch_idx = if sep && c.ch_type == 1 { 1 } else { 0 };
        let isp_ok = c.isp == NOT_ISP || {
            let (tw, th) = if c.isp == HOR_ISP {
                (c.blk[0].w, isp_split_dim(c.blk[0].w, c.blk[0].h, true))
            } else {
                (isp_split_dim(c.blk[0].w, c.blk[0].h, false), c.blk[0].h)
            };
            tw >= 4 && th >= 4
        };
        if (c.isp != NOT_ISP && !isp_ok)
            || (c.mip && !(c.blk[0].w >= 16 && c.blk[0].h >= 16))
            || (c.ch_type == 1 && c.blk[1].w.min(c.blk[1].h) < 4)
        {
            return;
        }
        let (sx, sy) = self.pic.fmt.scale(ch_idx);
        let lw = c.blk[ch_idx].w << sx;
        let lh = c.blk[ch_idx].h << sy;
        let max_tb = 1 << sps.log2_max_tb_size;
        if lw > max_tb || lh > max_tb {
            return;
        }
        let luma_flag = if sep { c.ch_type == 0 } else { true };
        let chroma_flag = if sep { c.ch_type == 1 } else { true };
        let non_zero =
            (luma_flag && cu_ctx.violates_lfnst[0]) || (chroma_flag && cu_ctx.violates_lfnst[1]);
        let mut is_ts = false;
        for t in c.first_tu..c.first_tu + c.num_tu {
            let tu = &self.pic.tus[t as usize];
            for comp in 0..self.pic.fmt.num_comp() {
                if tu.blk[comp].valid() && tu.cbf(comp) && tu.mts[comp] == MTS_SKIP {
                    is_ts = true;
                }
            }
        }
        if non_zero || (!cu_ctx.lfnst_last_scan_pos && c.isp == NOT_ISP) || is_ts {
            return;
        }
        let ctx_id = usize::from(sep);
        let mut idx = self.bin(ctx::LFNST_IDX + ctx_id);
        if idx != 0 {
            idx += self.bin(ctx::LFNST_IDX + 2);
        }
        self.cu_mut(cu_id).lfnst = idx as u8;
        vtrace!(
            "residual_lfnst_mode() etype=0 pos=({},{}) mode={}",
            c.lx(),
            c.ly(),
            idx
        );
    }

    fn mts_idx(&mut self, cu_id: u32, cu_ctx: &CuCtx) {
        let c = self.cu(cu_id).clone();
        let tu0 = c.first_tu as usize;
        let mut mts = self.pic.tus[tu0].mts[0];
        let sps = self.si.sps;
        let ts_max = 1 << sps.log2_max_ts_size;
        let mut allowed = c.ch_type == 0;
        allowed &= if c.pred == Pred::Intra {
            sps.explicit_mts_intra
        } else {
            sps.explicit_mts_inter && c.pred == Pred::Inter
        };
        allowed &= c.lw() <= 32 && c.lh() <= 32;
        allowed &= c.isp == NOT_ISP;
        allowed &= c.sbt == 0;
        allowed &= !(c.bdpcm[0] != 0 && c.lw() <= ts_max && c.lh() <= ts_max);
        if allowed
            && !cu_ctx.violates_mts
            && cu_ctx.mts_last_scan_pos
            && c.lfnst == 0
            && mts != MTS_SKIP
            && self.bin(ctx::MTS_INDEX) != 0
        {
            mts = MTS_DST7_DST7;
            for i in 0..3 {
                let s = self.bin(ctx::MTS_INDEX + 1 + i);
                mts += s as u8;
                if s == 0 {
                    break;
                }
            }
        }
        self.pic.tus[tu0].mts[0] = mts;
        vtrace!(
            "mts_idx() etype=0 pos=({},{}) mtsIdx={}",
            c.lx(),
            c.ly(),
            mts
        );
    }
}

pub fn mip_size_id(w: i32, h: i32) -> u32 {
    if w == 4 && h == 4 {
        0
    } else if w == 4 || h == 4 || (w == 8 && h == 8) {
        1
    } else {
        2
    }
}

/// vvdec's `CoeffCodingContext`.
pub struct CoeffCtx {
    pub ch: usize,
    pub width: i32,
    pub height: i32,
    pub log2_cg_w: i32,
    pub log2_cg_h: i32,
    pub log2_cg_size: usize,
    width_in_groups: i32,
    height_in_groups: i32,
    log2_w: i32,
    pub scan: Vec<u16>,
    scan_cg: Vec<u16>,
    pub max_last_x: u32,
    pub max_last_y: u32,
    pub last_off_x: u32,
    pub last_off_y: u32,
    pub last_shift_x: u32,
    pub last_shift_y: u32,
    pub scan_pos_last: usize,
    pub sub_set: usize,
    sub_set_pos: usize,
    pub cg_x: i32,
    pub cg_y: i32,
    pub min_sub_pos: usize,
    pub max_sub_pos: usize,
    pub sig_group_ctx: usize,
    pub sig_group_ctx_ts: usize,
    sig_cg: Vec<bool>,
    tpl: Vec<u8>,
    tmpl_diag: i32,
    tmpl_sum1: i32,
    sig_sets: [usize; 3],
    par_set: usize,
    gtx_sets: [usize; 2],
    pub sign_hiding: bool,
    pub bdpcm: bool,
    pub reg_bin_limit: i32,
    pub num_ctx_bins: i32,
    pub max_log2_range: u32,
}

/// Grouped 4x4 scan over the zero-out region with full-block positions.
fn grouped_scan(w: i32, h: i32) -> Vec<u16> {
    let (lcw, lch) = LOG2_SBB_SIZE[log2(w) as usize][log2(h) as usize];
    let (cw, ch) = (1i32 << lcw, 1i32 << lch);
    let wg = (32.min(w) >> lcw) as usize;
    let hg = (32.min(h) >> lch) as usize;
    let cg_scan = super::ps::diag_scan(wg, hg);
    let in_cg = super::ps::diag_scan(cw as usize, ch as usize);
    let mut out = Vec::with_capacity((w * h) as usize);
    for &g in &cg_scan {
        let (gx, gy) = ((g as usize % wg) as i32 * cw, (g as usize / wg) as i32 * ch);
        for &p in &in_cg {
            let (px, py) = ((p as i32) % cw, (p as i32) / cw);
            out.push(((gy + py) * w + gx + px) as u16);
        }
    }
    out.resize((w * h) as usize, 0);
    out
}

pub fn grouped_scan_cached(w: i32, h: i32) -> Vec<u16> {
    grouped_scan(w, h)
}

impl CoeffCtx {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        w: i32,
        h: i32,
        ch: usize,
        sign_hide: bool,
        is_luma: bool,
        cu: &Cu,
        mts: u8,
        ts: bool,
        sbt_zero_out: bool,
    ) -> Self {
        let (lcw, lch) = LOG2_SBB_SIZE[log2(w) as usize][log2(h) as usize];
        let (lcw, lch) = (lcw as i32, lch as i32);
        let wig = 32.min(w) >> lcw;
        let hig = 32.min(h) >> lch;
        let log2_w = log2(w);
        let log2_h = log2(h);
        const PREFIX: [u32; 8] = [0, 0, 0, 3, 6, 10, 15, 21];
        let bdpcm = if is_luma {
            cu.bdpcm[0] != 0
        } else {
            cu.bdpcm[1] != 0
        };
        // getTbAreaAfterCoefZeroOut
        let (mut zw, mut zh) = (w, h);
        if is_luma && (mts > MTS_SKIP || (sbt_zero_out && zw <= 32 && zh <= 32)) {
            if zw == 32 {
                zw = 16;
            }
            if zh == 32 {
                zh = 16;
            }
        }
        zw = zw.min(32);
        zh = zh.min(32);
        let reg = (zw * zh * 28) >> 4;
        let sig_base = [
            ctx::SIG_FLAG0,
            ctx::SIG_FLAG1,
            ctx::SIG_FLAG2,
            ctx::SIG_FLAG3,
            ctx::SIG_FLAG4,
            ctx::SIG_FLAG5,
        ];
        let par_base = [ctx::PAR_FLAG0, ctx::PAR_FLAG1];
        let gtx_base = [
            ctx::GTX_FLAG0,
            ctx::GTX_FLAG1,
            ctx::GTX_FLAG2,
            ctx::GTX_FLAG3,
        ];
        let _ = ts;
        Self {
            ch,
            width: w,
            height: h,
            log2_cg_w: lcw,
            log2_cg_h: lch,
            log2_cg_size: (lcw + lch) as usize,
            width_in_groups: wig,
            height_in_groups: hig,
            log2_w,
            scan: grouped_scan(w, h),
            scan_cg: super::ps::diag_scan(wig as usize, hig as usize),
            max_last_x: GROUP_IDX[(32.min(w) - 1) as usize],
            max_last_y: GROUP_IDX[(32.min(h) - 1) as usize],
            last_off_x: if ch == 0 { PREFIX[log2_w as usize] } else { 0 },
            last_off_y: if ch == 0 { PREFIX[log2_h as usize] } else { 0 },
            last_shift_x: if ch == 1 {
                (w >> 3).clamp(0, 2) as u32
            } else {
                ((log2_w + 1) >> 2) as u32
            },
            last_shift_y: if ch == 1 {
                (h >> 3).clamp(0, 2) as u32
            } else {
                ((log2_h + 1) >> 2) as u32
            },
            scan_pos_last: 0,
            sub_set: 0,
            sub_set_pos: 0,
            cg_x: 0,
            cg_y: 0,
            min_sub_pos: 0,
            max_sub_pos: 0,
            sig_group_ctx: 0,
            sig_group_ctx_ts: 0,
            sig_cg: vec![false; 64],
            tpl: vec![0; (w * h) as usize],
            tmpl_diag: -1,
            tmpl_sum1: -1,
            sig_sets: [sig_base[ch], sig_base[ch + 2], sig_base[ch + 4]],
            par_set: par_base[ch],
            gtx_sets: [gtx_base[ch], gtx_base[ch + 2]],
            sign_hiding: sign_hide,
            bdpcm,
            reg_bin_limit: reg,
            num_ctx_bins: 0,
            max_log2_range: 15,
        }
    }

    pub fn last_sub_set(&self) -> usize {
        ((self.width * self.height - 1) as usize) >> self.log2_cg_size
    }
    pub fn none_sig_group(&self) -> bool {
        !self.sig_cg.iter().any(|&b| b)
    }
    pub fn is_last(&self) -> bool {
        (self.scan_pos_last >> self.log2_cg_size) == self.sub_set
    }
    pub fn is_sig_group(&self) -> bool {
        self.sig_cg[self.sub_set_pos]
    }
    pub fn set_sig_group(&mut self) {
        self.sig_cg[self.sub_set_pos] = true;
    }

    pub fn init_subblock(&mut self, sub_set: usize) {
        self.sub_set = sub_set;
        self.sub_set_pos = self.scan_cg[sub_set] as usize;
        let log2_wig = log2(self.width_in_groups);
        self.cg_y = (self.sub_set_pos >> log2_wig) as i32;
        self.cg_x = self.sub_set_pos as i32 - self.cg_y * self.width_in_groups;
        self.min_sub_pos = sub_set << self.log2_cg_size;
        self.max_sub_pos = self.min_sub_pos + (1 << self.log2_cg_size) - 1;
        let last_hor = self.cg_x == self.width_in_groups - 1;
        let last_ver = self.cg_y == self.height_in_groups - 1;
        let right = !last_hor && self.sig_cg[self.sub_set_pos + 1];
        let lower = !last_ver && self.sig_cg[self.sub_set_pos + self.width_in_groups as usize];
        let base = if self.ch == 0 {
            ctx::SIG_COEFF_GROUP0
        } else {
            ctx::SIG_COEFF_GROUP1
        };
        self.sig_group_ctx = base + usize::from(right || lower);
        let left = self.cg_x > 0 && self.sig_cg[self.sub_set_pos - 1];
        let above = self.cg_y > 0 && self.sig_cg[self.sub_set_pos - self.width_in_groups as usize];
        self.sig_group_ctx_ts = ctx::TS_SIG_COEFF_GROUP + usize::from(left) + usize::from(above);
    }

    fn sig_ctx(&mut self, blk: usize, state: u32) -> usize {
        let py = (blk >> self.log2_w) as i32;
        let px = (blk & ((1 << self.log2_w) - 1)) as i32;
        let diag = px + py;
        let tpl = self.tpl[blk] as i32;
        let num_pos = tpl >> 5;
        let sum_abs = tpl & 31;
        let mut ofs = ((sum_abs + 1) >> 1).min(3) + if diag < 2 { 4 } else { 0 };
        if self.ch == 0 {
            ofs += if diag < 5 { 4 } else { 0 };
        }
        self.tmpl_diag = diag;
        self.tmpl_sum1 = sum_abs - num_pos;
        self.sig_sets[(state as i32 - 1).max(0) as usize] + ofs as usize
    }

    fn abs_val_1st_pass(&mut self, blk: usize, coeff: &mut [i32], abs: i32) {
        coeff[blk] = abs;
        let py = blk >> self.log2_w;
        let px = blk & ((1 << self.log2_w) - 1);
        let w = self.width as usize;
        let add = (32 + abs) as u8;
        let mut upd = |off: usize| {
            let v = &mut self.tpl[blk - off];
            *v = v.wrapping_add(add);
        };
        if py > 1 {
            upd(2 * w);
        }
        if py > 0 && px > 0 {
            upd(w + 1);
        }
        if py > 0 {
            upd(w);
        }
        if px > 1 {
            upd(2);
        }
        if px > 0 {
            upd(1);
        }
    }

    fn ctx_offset_abs(&self) -> usize {
        let mut offset = 0;
        if self.tmpl_diag != -1 {
            offset = self.tmpl_sum1.min(4) + 1;
            offset += if self.tmpl_diag == 0 {
                if self.ch == 0 { 15 } else { 5 }
            } else if self.ch == 0 {
                if self.tmpl_diag < 3 {
                    10
                } else if self.tmpl_diag < 10 {
                    5
                } else {
                    0
                }
            } else {
                0
            };
        }
        offset as usize
    }

    fn par_ctx(&self, off: usize) -> usize {
        self.par_set + off
    }
    fn gtx1_ctx(&self, off: usize) -> usize {
        self.gtx_sets[1] + off
    }
    fn gtx2_ctx(&self, off: usize) -> usize {
        self.gtx_sets[0] + off
    }

    fn template_abs_sum(&self, blk: usize, coeff: &[i32], base: i32) -> u32 {
        let py = (blk >> self.log2_w) as i32;
        let px = (blk & ((1 << self.log2_w) - 1)) as i32;
        let w = self.width;
        let h = self.height;
        let at = |dx: i32, dy: i32| coeff[((py + dy) * w + px + dx) as usize];
        let mut sum = 0;
        if px + 2 < w {
            sum += at(1, 0);
            sum += at(2, 0);
            if py + 1 < h {
                sum += at(1, 1);
            }
        } else if px + 1 < w {
            sum += at(1, 0);
            if py + 1 < h {
                sum += at(1, 1);
            }
        }
        if py + 2 < h {
            sum += at(0, 1);
            sum += at(0, 2);
        } else if py + 1 < h {
            sum += at(0, 1);
        }
        (sum - 5 * base).clamp(0, 31) as u32
    }
}
