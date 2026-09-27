// SPDX-License-Identifier: LGPL-3.0-or-later
//! Motion vector derivation for inter coding units: merge, MMVD, geometric,
//! affine and subblock temporal merge candidates, AMVP and history-based
//! prediction, following vvdec's `DecCu::xDeriveCUMV` and `UnitTools`.
use super::Error;
use super::ctu::CtuDecoder;
use super::mv::*;
use super::pic::*;
use super::ps::{B_SLICE, Pps};

/// One merge candidate (a slot of vvdec's `MergeCtx`).
#[derive(Clone, Copy, Debug)]
pub struct MergeCand {
    pub dir: u8,
    pub mv: [Mv; 2],
    pub ref_idx: [i8; 2],
    pub bcw: u8,
    pub alt_hpel: bool,
    pub typ: u8,
}

impl Default for MergeCand {
    fn default() -> Self {
        Self {
            dir: 0,
            mv: [Mv::default(); 2],
            ref_idx: [-1, -1],
            bcw: 0,
            alt_hpel: false,
            typ: MRG_TYPE_DEFAULT_N,
        }
    }
}

impl MergeCand {
    fn from_mi(mi: &MotionInfo, is_b: bool) -> Self {
        let mut c = MergeCand {
            dir: mi.inter_dir(),
            ..Default::default()
        };
        c.mv[0] = mi.mv[0];
        c.ref_idx[0] = mi.ref_idx[0];
        if is_b {
            c.mv[1] = mi.mv[1];
            c.ref_idx[1] = mi.ref_idx[1];
        }
        c
    }
}

/// An affine merge candidate (vvdec's `AffineMergeCtx` slot).
#[derive(Clone, Copy, Debug)]
pub struct AffineCand {
    pub dir: u8,
    pub mv: [[Mv; 3]; 2],
    pub ref_idx: [i8; 2],
    pub affine_type: u8,
    pub merge_type: u8,
    pub bcw: u8,
}

impl Default for AffineCand {
    fn default() -> Self {
        Self {
            dir: 0,
            mv: [[Mv::default(); 3]; 2],
            ref_idx: [-1, -1],
            affine_type: 0,
            merge_type: MRG_TYPE_DEFAULT_N,
            bcw: 0,
        }
    }
}

/// `xGetDistScaleFactor`.
pub fn dist_scale_factor(cur_poc: i32, cur_ref_poc: i32, col_poc: i32, col_ref_poc: i32) -> i32 {
    let diff_d = col_poc.wrapping_sub(col_ref_poc);
    let diff_b = cur_poc.wrapping_sub(cur_ref_poc);
    if diff_d == diff_b {
        return 4096;
    }
    let tdb = diff_b.clamp(-128, 127);
    let tdd = diff_d.clamp(-128, 127);
    let x = (0x4000 + (tdd / 2).abs()) / tdd;
    ((tdb * x + 32) >> 6).clamp(-4096, 4095)
}

fn floor_log2(v: i32) -> i32 {
    31 - (v as u32).leading_zeros() as i32
}

/// `roundMvComp`: round a vector component to the 6-bit mantissa, 4-bit
/// exponent storage format of collocated motion.
fn round_mv_comp(val: i32) -> i32 {
    let sign = val >> 31;
    let scale = floor_log2((val ^ sign) | 31) - 5;
    let packed = if scale >= 0 {
        let round = (1 << scale) >> 1;
        let n = (val + round) >> scale;
        let exponent = scale + ((n ^ sign) >> 5);
        let mantissa = (n & 31) | (sign * 32);
        exponent | (mantissa * 16)
    } else {
        val * 16
    };
    let exponent = packed & 15;
    let mantissa = packed >> 4;
    if exponent == 0 {
        mantissa
    } else {
        (mantissa ^ 32) * (1 << (exponent - 1))
    }
}

/// `PPS::getSubPicFromPos`.
pub fn subpic_at(pps: &Pps, x: i32, y: i32) -> Option<&super::ps::SubPic> {
    pps.subpics
        .iter()
        .find(|s| {
            x >= s.left as i32 && x <= s.right as i32 && y >= s.top as i32 && y <= s.bottom as i32
        })
        .or(pps.subpics.first())
}

/// `InterPrediction::isSubblockVectorSpreadOverLimit`.
pub fn subblock_spread_over_limit(a: i32, b: i32, c: i32, d: i32, pred_type: u8) -> bool {
    let s4 = 4 << 11;
    let tap = 6;
    if pred_type == 3 {
        let w = (0.max(4 * a + s4)).max((4 * c).max(4 * a + 4 * c + s4))
            - (0.min(4 * a + s4)).min((4 * c).min(4 * a + 4 * c + s4));
        let h = (0.max(4 * b)).max((4 * d + s4).max(4 * b + 4 * d + s4))
            - (0.min(4 * b)).min((4 * d + s4).min(4 * b + 4 * d + s4));
        let w = (w >> 11) + tap + 3;
        let h = (h >> 11) + tap + 3;
        w * h > (tap + 9) * (tap + 9)
    } else {
        let w = 0.max(4 * a + s4) - 0.min(4 * a + s4);
        let h = 0.max(4 * b) - 0.min(4 * b);
        let w = (w >> 11) + tap + 3;
        let h = (h >> 11) + tap + 3;
        if w * h > (tap + 9) * (tap + 5) {
            return true;
        }
        let w = 0.max(4 * c) - 0.min(4 * c);
        let h = 0.max(4 * d + s4) - 0.min(4 * d + s4);
        let w = (w >> 11) + tap + 3;
        let h = (h >> 11) + tap + 3;
        w * h > (tap + 5) * (tap + 9)
    }
}

/// Directions of `addMVPCandUnscaled`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Left,
    Above,
    AboveRight,
    BelowLeft,
    AboveLeft,
}

impl<'a, 's, 'b> CtuDecoder<'a, 's, 'b> {
    fn is_b(&self) -> bool {
        self.si.sh.slice_type == B_SLICE
    }

    fn plevel(&self) -> i32 {
        self.si.sps.log2_parallel_merge_level as i32
    }

    /// `PU::isDiffMER`.
    fn diff_mer(&self, cu_id: u32, x: i32, y: i32) -> bool {
        let c = &self.pic.cus[cu_id as usize];
        let p = self.plevel();
        (c.lx() >> p) != (x >> p) || (c.ly() >> p) != (y >> p)
    }

    /// `getCURestricted` in the luma channel.
    fn restricted(&self, cu_id: u32, x: i32, y: i32, guess: Option<u32>) -> Option<u32> {
        self.pic.get_cu_restricted(x, y, cu_id, 0, guess, self.wpp)
    }

    fn is_inter_cu(&self, id: u32) -> bool {
        self.pic.cus[id as usize].pred == Pred::Inter
    }

    fn ref_poc(&self, l: usize, idx: i8) -> i32 {
        self.si.inter.info.ref_poc[l]
            .get(idx.max(0) as usize)
            .copied()
            .unwrap_or(0)
    }

    fn ref_lt(&self, l: usize, idx: i8) -> bool {
        self.si.inter.info.ref_lt[l]
            .get(idx.max(0) as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Bottom-right collocated position `C0` and whether it is usable.
    fn col_c0(&self, cu_id: u32) -> (Option<(i32, i32)>, (i32, i32)) {
        let c = &self.pic.cus[cu_id as usize];
        let (rbx, rby) = (c.lx() + c.lw() - 1 - 3, c.ly() + c.lh() - 1 - 3);
        let ctu = 1i32 << self.si.sps.log2_ctu_size;
        let mut boundary = rbx + 4 < self.pic.width && rby + 4 < self.pic.height;
        if let Some(sp) = subpic_at(self.si.pps, c.lx(), c.ly())
            && sp.treated_as_pic
        {
            boundary = rbx + 4 <= sp.right as i32 && rby + 4 <= sp.bottom as i32;
        }
        let center = (c.lx() + (c.lw() >> 1), c.ly() + (c.lh() >> 1));
        if boundary {
            let iy = rby & (ctu - 1);
            // Available unless the position is on the CTU's last 4x4 row;
            // the column does not matter (vvdec's C0 conditions).
            if iy + 4 < ctu {
                return (Some((rbx + 4, rby + 4)), center);
            }
        }
        (None, center)
    }

    /// `PU::getColocatedMVP`.
    pub(super) fn col_mvp(
        &self,
        cu_id: u32,
        l: usize,
        x: i32,
        y: i32,
        ref_idx: i8,
        sb: bool,
    ) -> Option<Mv> {
        let c = &self.pic.cus[cu_id as usize];
        if c.pred == Pred::Ibc {
            return None;
        }
        let col = self.si.inter.col.as_ref()?;
        if let Some(sp) = subpic_at(self.si.pps, c.lx(), c.ly())
            && sp.treated_as_pic
            && !(x >= sp.left as i32
                && x <= sp.right as i32
                && y >= sp.top as i32
                && y <= sp.bottom as i32)
        {
            return None;
        }
        let ldc = self.si.inter.check_ldc;
        let mut col_list = if ldc {
            l
        } else {
            usize::from(self.si.sh.col_from_l0)
        };
        let (mi, col_slice) = col.col_info(x, y);
        if !mi.is_inter() {
            return None;
        }
        let mut col_ref = mi.ref_idx[col_list];
        if sb && !ldc {
            col_list = l;
            col_ref = mi.ref_idx[col_list];
            if col_ref < 0 {
                return None;
            }
        } else if col_ref < 0 {
            col_list = 1 - col_list;
            col_ref = mi.ref_idx[col_list];
            if col_ref < 0 {
                return None;
            }
        }
        let cs = col_slice?;
        let cur_lt = self.ref_lt(l, ref_idx);
        let col_lt = cs.ref_lt[col_list]
            .get(col_ref as usize)
            .copied()
            .unwrap_or(false);
        if cur_lt != col_lt {
            return None;
        }
        let m = mi.mv[col_list];
        let m = Mv::new(round_mv_comp(m.x), round_mv_comp(m.y));
        if cur_lt {
            return Some(m.clip_storage());
        }
        let col_ref_poc = cs.ref_poc[col_list]
            .get(col_ref as usize)
            .copied()
            .unwrap_or(0);
        let scale = dist_scale_factor(
            self.si.inter.info.poc,
            self.ref_poc(l, ref_idx),
            cs.poc,
            col_ref_poc,
        );
        Some(if scale == 4096 {
            m.clip_storage()
        } else {
            m.scale(scale)
        })
    }

    /// `PU::xCheckSimilarMotion`.
    fn similar_motion(list: &[MergeCand], idx: usize, prev: usize, pruned: &mut [bool; 6]) -> bool {
        for ui in 0..prev {
            if pruned[ui] {
                continue;
            }
            if list[ui].dir != list[idx].dir {
                continue;
            }
            let (a, b) = (&list[ui], &list[idx]);
            let same = if a.dir == 3 {
                a.ref_idx == b.ref_idx && a.mv == b.mv
            } else {
                let l = (a.dir - 1) as usize;
                a.ref_idx[l] == b.ref_idx[l] && a.mv[l] == b.mv[l]
            };
            if same {
                pruned[ui] = true;
                return true;
            }
        }
        false
    }

    /// `PU::getInterMergeCandidates`; stops early once `stop` is reached.
    pub(super) fn merge_candidates(&self, cu_id: u32, stop: Option<usize>) -> Vec<MergeCand> {
        let max = self.si.sps.max_num_merge_cand as usize;
        let is_b = self.is_b();
        let c = self.pic.cus[cu_id as usize].clone();
        let mut list: Vec<MergeCand> = Vec::with_capacity(6);
        let done = |list: &Vec<MergeCand>| stop.is_some_and(|s| list.len() == s + 1);
        let (lx, ly, w, h) = (c.lx(), c.ly(), c.lw(), c.lh());
        let pos_rt = (lx + w - 1, ly);
        let pos_lb = (lx, ly + h - 1);
        let cand_of = |id: u32, x: i32, y: i32| -> MergeCand {
            let n = &self.pic.cus[id as usize];
            let mi = self.pic.mi(x, y);
            let mut m = MergeCand::from_mi(&mi, is_b);
            m.alt_hpel = n.imv == IMV_HPEL;
            m.bcw = if m.dir == 3 { n.bcw } else { 0 };
            m
        };
        // B1: above
        let above = self.restricted(cu_id, pos_rt.0, pos_rt.1 - 1, c.above);
        let avail_b1 =
            above.filter(|&a| self.is_inter_cu(a) && self.diff_mer(cu_id, pos_rt.0, pos_rt.1 - 1));
        let mi_above = self.pic.mi(pos_rt.0, (pos_rt.1 - 1).max(0));
        if let Some(a) = avail_b1 {
            list.push(cand_of(a, pos_rt.0, pos_rt.1 - 1));
            if done(&list) {
                return list;
            }
        }
        if list.len() == max {
            return list;
        }
        // A1: left
        let left = self.restricted(cu_id, pos_lb.0 - 1, pos_lb.1, c.left);
        let avail_a1 =
            left.filter(|&l| self.is_inter_cu(l) && self.diff_mer(cu_id, pos_lb.0 - 1, pos_lb.1));
        let mi_left = self.pic.mi((pos_lb.0 - 1).max(0), pos_lb.1);
        let slice_of = |id: u32| self.pic.cus[id as usize].slice;
        if let Some(l) = avail_a1
            && (avail_b1.is_none()
                || slice_of(avail_b1.unwrap()) != slice_of(l)
                || mi_above != mi_left)
        {
            list.push(cand_of(l, pos_lb.0 - 1, pos_lb.1));
            if done(&list) {
                return list;
            }
        }
        if list.len() == max {
            return list;
        }
        let spatial = list.len();
        // B0: above right
        let ar = self.restricted(cu_id, pos_rt.0 + 1, pos_rt.1 - 1, above);
        if let Some(r) =
            ar.filter(|&r| self.is_inter_cu(r) && self.diff_mer(cu_id, pos_rt.0 + 1, pos_rt.1 - 1))
        {
            let mi = self.pic.mi(pos_rt.0 + 1, pos_rt.1 - 1);
            if avail_b1.is_none() || slice_of(avail_b1.unwrap()) != slice_of(r) || mi_above != mi {
                list.push(cand_of(r, pos_rt.0 + 1, pos_rt.1 - 1));
                if done(&list) {
                    return list;
                }
            }
            if list.len() == max {
                return list;
            }
        }
        let _ = spatial;
        // A0: below left
        let bl = self.restricted(cu_id, pos_lb.0 - 1, pos_lb.1 + 1, left);
        if let Some(b) =
            bl.filter(|&b| self.is_inter_cu(b) && self.diff_mer(cu_id, pos_lb.0 - 1, pos_lb.1 + 1))
        {
            let mi = self.pic.mi(pos_lb.0 - 1, pos_lb.1 + 1);
            if avail_a1.is_none() || slice_of(b) != slice_of(avail_a1.unwrap()) || mi != mi_left {
                list.push(cand_of(b, pos_lb.0 - 1, pos_lb.1 + 1));
                if done(&list) {
                    return list;
                }
            }
            if list.len() == max {
                return list;
            }
        }
        // B2: above left
        if list.len() < 4 {
            let guess = if c.left.is_some() { c.left } else { c.above };
            let al = self.restricted(cu_id, lx - 1, ly - 1, guess);
            if let Some(a) =
                al.filter(|&a| self.is_inter_cu(a) && self.diff_mer(cu_id, lx - 1, ly - 1))
            {
                let mi = self.pic.mi(lx - 1, ly - 1);
                let ok_a1 = avail_a1.is_none()
                    || slice_of(avail_a1.unwrap()) != slice_of(a)
                    || mi_left != mi;
                let ok_b1 = avail_b1.is_none()
                    || slice_of(avail_b1.unwrap()) != slice_of(a)
                    || mi_above != mi;
                if ok_a1 && ok_b1 {
                    list.push(cand_of(a, lx - 1, ly - 1));
                    if done(&list) {
                        return list;
                    }
                }
            }
            if list.len() == max {
                return list;
            }
        }
        // temporal
        if self.si.ph.temporal_mvp && w + h > 12 {
            let (c0, c1) = self.col_c0(cu_id);
            let mut cand = MergeCand::default();
            let mut dir = 0u8;
            let get = |l: usize| {
                c0.and_then(|p| self.col_mvp(cu_id, l, p.0, p.1, 0, false))
                    .or_else(|| self.col_mvp(cu_id, l, c1.0, c1.1, 0, false))
            };
            if let Some(m) = get(0) {
                dir |= 1;
                cand.mv[0] = m;
                cand.ref_idx[0] = 0;
            }
            if is_b && let Some(m) = get(1) {
                dir |= 2;
                cand.mv[1] = m;
                cand.ref_idx[1] = 0;
            }
            if dir != 0 {
                cand.dir = dir;
                list.push(cand);
                if done(&list) {
                    return list;
                }
            }
            if list.len() == max {
                return list;
            }
        }
        // history
        let max1 = max - 1;
        if list.len() != max1 && self.hmvp_merge(&mut list, stop, max1, spatial, false, true, is_b)
        {
            return list;
        }
        // pairwise average
        if list.len() > 1 && list.len() < max {
            let mut cand = MergeCand {
                alt_hpel: if list[0].alt_hpel == list[1].alt_hpel {
                    list[0].alt_hpel
                } else {
                    false
                },
                ..Default::default()
            };
            let mut dir = 0u8;
            for l in 0..if is_b { 2 } else { 1 } {
                let (ri, rj) = (list[0].ref_idx[l], list[1].ref_idx[l]);
                if ri < 0 && rj < 0 {
                    continue;
                }
                dir += 1 << l;
                if ri >= 0 && rj >= 0 {
                    let s = list[0].mv[l].add(list[1].mv[l]);
                    let (x, y) = round_affine(s.x, s.y, 1);
                    cand.mv[l] = Mv::new(x, y);
                    cand.ref_idx[l] = ri;
                } else if ri >= 0 {
                    cand.mv[l] = list[0].mv[l];
                    cand.ref_idx[l] = ri;
                } else {
                    cand.mv[l] = list[1].mv[l];
                    cand.ref_idx[l] = rj;
                }
            }
            cand.dir = dir;
            if dir > 0 {
                list.push(cand);
            }
            if list.len() == max {
                return list;
            }
        }
        // zero candidates
        let num_ref = if is_b {
            self.si.sh.num_ref_idx[0].min(self.si.sh.num_ref_idx[1])
        } else {
            self.si.sh.num_ref_idx[0]
        } as i32;
        let mut r = 0i32;
        let mut refcnt = 0i32;
        while list.len() < max {
            let mut cand = MergeCand {
                dir: 1,
                ..Default::default()
            };
            cand.ref_idx[0] = r as i8;
            if is_b {
                cand.dir = 3;
                cand.ref_idx[1] = r as i8;
            }
            list.push(cand);
            if refcnt == num_ref - 1 {
                r = 0;
            } else {
                r += 1;
                refcnt += 1;
            }
        }
        list
    }

    /// `PU::addMergeHMVPCand`; returns true on an early exit at `stop`.
    #[allow(clippy::too_many_arguments)]
    fn hmvp_merge(
        &self,
        list: &mut Vec<MergeCand>,
        stop: Option<usize>,
        max1: usize,
        prev: usize,
        ibc: bool,
        gt4x4: bool,
        is_b: bool,
    ) -> bool {
        let mut pruned = [false; 6];
        let lut = &self.pic.hmvp;
        let n = lut.len();
        for k in 1..=n {
            let h = lut[n - k];
            let mut cand = MergeCand {
                dir: h.mi.inter_dir(),
                alt_hpel: !ibc && h.alt_hpel,
                ..Default::default()
            };
            cand.mv[0] = h.mi.mv[0];
            cand.ref_idx[0] = h.mi.ref_idx[0];
            if is_b {
                cand.mv[1] = h.mi.mv[1];
                cand.ref_idx[1] = h.mi.ref_idx[1];
            }
            list.push(cand);
            let idx = list.len() - 1;
            if k > 2
                || ((k > 1 || !gt4x4) && ibc)
                || !Self::similar_motion(list, idx, prev, &mut pruned)
            {
                list[idx].bcw = if list[idx].dir == 3 { h.bcw } else { 0 };
                if stop == Some(idx) {
                    return true;
                }
                if list.len() == max1 {
                    break;
                }
            } else {
                list.pop();
            }
        }
        false
    }

    /// `MergeCtx::setMergeInfo` followed by `restrictBiPredMergeCandsOne`.
    fn set_merge_info(&mut self, cu_id: u32, cand: &MergeCand) {
        let c = &mut self.pic.cus[cu_id as usize];
        c.inter_dir = cand.dir;
        c.imv = if !c.geo && cand.alt_hpel { IMV_HPEL } else { 0 };
        c.merge_type = cand.typ;
        c.mv[0][0] = cand.mv[0];
        c.mv[1][0] = cand.mv[1];
        c.ref_idx = cand.ref_idx;
        c.bcw = if cand.dir == 3 { cand.bcw } else { 0 };
        restrict_bi(c);
    }

    /// `PU::getInterMMVDMergeCandidates` + `MergeCtx::setMmvdMergeCandiInfo`.
    fn set_mmvd_info(&mut self, cu_id: u32, list: &[MergeCand]) {
        let idx = self.pic.cus[cu_id as usize].mmvd_idx as i32;
        // base candidates: the first two default-type candidates
        let mut base: Vec<(MergeCand, usize)> = Vec::new();
        for (k, c) in list.iter().enumerate() {
            if c.typ == MRG_TYPE_DEFAULT_N {
                let mut b = *c;
                if c.ref_idx[0] >= 0 && c.ref_idx[1] >= 0 {
                } else if c.ref_idx[0] >= 0 {
                    b.mv[1] = Mv::default();
                    b.ref_idx[1] = -1;
                } else if c.ref_idx[1] >= 0 {
                    b.mv[0] = Mv::default();
                    b.ref_idx[0] = -1;
                }
                base.push((b, k));
                if base.len() == 2 {
                    break;
                }
            }
        }
        let rem = idx % 64;
        let base_idx = (rem / 32) as usize;
        let rem = rem % 32;
        let step = rem / 4;
        let pos = rem % 4;
        let mut offset = (1 << step) << 2;
        if self.si.ph.dis_frac_mmvd {
            offset <<= 2;
        }
        let (b, k) = base[base_idx.min(base.len().saturating_sub(1))];
        let dir_mv = |p: i32| match p {
            0 => Mv::new(offset, 0),
            1 => Mv::new(-offset, 0),
            2 => Mv::new(0, offset),
            _ => Mv::new(0, -offset),
        };
        let (r0, r1) = (b.ref_idx[0], b.ref_idx[1]);
        let poc = self.si.inter.info.poc;
        let mut out_mv = [Mv::default(); 2];
        let mut out_ref = [-1i8; 2];
        let dir;
        if r0 != -1 && r1 != -1 {
            let poc0 = self.ref_poc(0, r0);
            let poc1 = self.ref_poc(1, r1);
            let mut t = [dir_mv(pos), Mv::default()];
            let lt = self.ref_lt(0, r0) || self.ref_lt(1, r1);
            if poc0 - poc == poc1 - poc {
                t[1] = t[0];
            } else if (poc1 - poc).abs() > (poc0 - poc).abs() {
                t[1] = t[0];
                let scale = dist_scale_factor(poc, poc0, poc, poc1);
                if lt {
                    t[0] = if (poc1 - poc) * (poc0 - poc) > 0 {
                        t[1]
                    } else {
                        Mv::new(-t[1].x, -t[1].y)
                    };
                } else {
                    t[0] = t[1].scale(scale);
                }
            } else {
                let scale = dist_scale_factor(poc, poc1, poc, poc0);
                if lt {
                    t[1] = if (poc1 - poc) * (poc0 - poc) > 0 {
                        t[0]
                    } else {
                        Mv::new(-t[0].x, -t[0].y)
                    };
                } else {
                    t[1] = t[0].scale(scale);
                }
            }
            dir = 3;
            out_mv = [b.mv[0].add(t[0]), b.mv[1].add(t[1])];
            out_ref = [r0, r1];
        } else if r0 != -1 {
            dir = 1;
            out_mv[0] = b.mv[0].add(dir_mv(pos));
            out_ref[0] = r0;
        } else {
            dir = 2;
            out_mv[1] = b.mv[1].add(dir_mv(pos));
            out_ref[1] = r1;
        }
        let c = &mut self.pic.cus[cu_id as usize];
        c.inter_dir = dir;
        for l in 0..2 {
            c.mv[l][0] = if out_ref[l] >= 0 {
                out_mv[l].clip_storage()
            } else {
                out_mv[l]
            };
        }
        c.ref_idx = out_ref;
        c.imv = if list[k].alt_hpel { IMV_HPEL } else { 0 };
        c.bcw = if list[base_idx.min(list.len() - 1)].dir == 3 {
            list[base_idx.min(list.len() - 1)].bcw
        } else {
            0
        };
        restrict_bi(c);
    }

    /// `PU::spanMotionInfo`.
    fn span_motion(&mut self, cu_id: u32) {
        let c = self.pic.cus[cu_id as usize].clone();
        if c.merge && c.merge_type == MRG_TYPE_SUBPU_ATMVP {
            return;
        }
        let ibc = c.pred == Pred::Ibc;
        let mut mi = MotionInfo::default();
        for l in 0..2 {
            mi.mv[l] = c.mv[l][0];
            mi.ref_idx[l] = if ibc { -1 } else { c.ref_idx[l] };
        }
        let (x0, y0) = (c.lx() >> 2, c.ly() >> 2);
        let (w, h) = (c.lw() >> 2, c.lh() >> 2);
        let mw = self.pic.map_w;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let d = &mut self.pic.motion[y as usize * mw + x as usize];
                if c.affine {
                    for l in 0..2 {
                        if mi.ref_idx[l] < 0 {
                            d.mv[l] = Mv::default();
                        }
                        d.ref_idx[l] = mi.ref_idx[l];
                    }
                } else {
                    *d = mi;
                }
            }
        }
    }

    /// `PU::addMVPCandUnscaled`.
    fn mvp_cand_unscaled(
        &self,
        cu_id: u32,
        l: usize,
        ref_idx: i8,
        pos: (i32, i32),
        dir: Dir,
    ) -> Option<Mv> {
        let c = &self.pic.cus[cu_id as usize];
        let (guess, np) = match dir {
            Dir::Left => (c.left, (pos.0 - 1, pos.1)),
            Dir::Above => (c.above, (pos.0, pos.1 - 1)),
            Dir::AboveRight => (c.above, (pos.0 + 1, pos.1 - 1)),
            Dir::BelowLeft => (c.left, (pos.0 - 1, pos.1 + 1)),
            Dir::AboveLeft => (
                if c.left.is_some() { c.left } else { c.above },
                (pos.0 - 1, pos.1 - 1),
            ),
        };
        let n = self.restricted(cu_id, np.0, np.1, guess)?;
        if !self.is_inter_cu(n) {
            return None;
        }
        let mi = self.pic.mi(np.0, np.1);
        let cur_poc = self.ref_poc(l, ref_idx);
        for src in 0..2 {
            let li = if src == 0 { l } else { 1 - l };
            let nr = mi.ref_idx[li];
            if nr >= 0 && cur_poc == self.ref_poc(li, nr) {
                return Some(mi.mv[li]);
            }
        }
        None
    }

    /// `PU::fillMvpCand`.
    pub(super) fn fill_mvp(&self, cu_id: u32, l: usize, ref_idx: i8) -> [Mv; 2] {
        let c = self.pic.cus[cu_id as usize].clone();
        let imv = c.imv;
        let mut cands: Vec<Mv> = Vec::with_capacity(3);
        if ref_idx < 0 {
            return [Mv::default(); 2];
        }
        let pos_lt = (c.lx(), c.ly());
        let pos_rt = (c.lx() + c.lw() - 1, c.ly());
        let pos_lb = (c.lx(), c.ly() + c.lh() - 1);
        if let Some(m) = self
            .mvp_cand_unscaled(cu_id, l, ref_idx, pos_lb, Dir::BelowLeft)
            .or_else(|| self.mvp_cand_unscaled(cu_id, l, ref_idx, pos_lb, Dir::Left))
        {
            cands.push(m);
        }
        if let Some(m) = self
            .mvp_cand_unscaled(cu_id, l, ref_idx, pos_rt, Dir::AboveRight)
            .or_else(|| self.mvp_cand_unscaled(cu_id, l, ref_idx, pos_rt, Dir::Above))
            .or_else(|| self.mvp_cand_unscaled(cu_id, l, ref_idx, pos_lt, Dir::AboveLeft))
        {
            cands.push(m);
        }
        for m in cands.iter_mut() {
            *m = m.round_to_amvr_signal_precision(MV_PRECISION_INTERNAL, imv);
        }
        if cands.len() == 2 && cands[0] == cands[1] {
            cands.pop();
        }
        if self.si.ph.temporal_mvp && cands.len() < 2 && c.lw() + c.lh() > 12 {
            let (c0, c1) = self.col_c0(cu_id);
            if let Some(m) = c0
                .and_then(|p| self.col_mvp(cu_id, l, p.0, p.1, ref_idx, false))
                .or_else(|| self.col_mvp(cu_id, l, c1.0, c1.1, ref_idx, false))
            {
                cands.push(m.round_to_amvr_signal_precision(MV_PRECISION_INTERNAL, imv));
            }
        }
        if cands.len() < 2 {
            // addAMVPHMVPCand: oldest entries first, as vvdec indexes lut[mrgIdx - 1]
            let cur_poc = self.ref_poc(l, ref_idx);
            let lut = &self.pic.hmvp;
            let allowed = lut.len().min(4);
            'outer: for h in lut.iter().take(allowed) {
                if cands.len() >= 2 {
                    break;
                }
                for src in 0..2 {
                    let li = if src == 0 { l } else { 1 - l };
                    let nr = h.mi.ref_idx[li];
                    if nr >= 0 && cur_poc == self.ref_poc(li, nr) {
                        cands.push(
                            h.mi.mv[li].round_to_amvr_signal_precision(MV_PRECISION_INTERNAL, imv),
                        );
                        if cands.len() >= 2 {
                            break 'outer;
                        }
                    }
                }
            }
        }
        cands.truncate(2);
        while cands.len() < 2 {
            cands.push(Mv::default());
        }
        [
            cands[0].round_to_amvr_signal_precision(MV_PRECISION_INTERNAL, imv),
            cands[1].round_to_amvr_signal_precision(MV_PRECISION_INTERNAL, imv),
        ]
    }

    /// `DecCu::xDeriveCUMV` for inter (non-IBC) coding units, including
    /// the history table update.
    pub(super) fn derive_cu_mv(&mut self, cu_id: u32) -> Result<(), Error> {
        let c = self.pic.cus[cu_id as usize].clone();
        if c.merge {
            if c.mmvd {
                let base = (c.mmvd_idx as usize) / 32;
                let list = self.merge_candidates(cu_id, Some(base + 1));
                self.set_mmvd_info(cu_id, &list);
                self.span_motion(cu_id);
            } else if c.geo {
                self.derive_geo(cu_id);
            } else if c.affine {
                self.derive_affine_merge(cu_id)?;
                self.span_motion(cu_id);
            } else {
                let idx = c.merge_idx as usize;
                let list = self.merge_candidates(cu_id, Some(idx));
                let cand = *list
                    .get(idx)
                    .ok_or(Error::Invalid("Merge candidate does not exist"))?;
                self.set_merge_info(cu_id, &cand);
                self.span_motion(cu_id);
            }
        } else if c.affine {
            self.derive_affine_amvp(cu_id);
            self.span_motion(cu_id);
        } else {
            for l in 0..2 {
                let cur = &self.pic.cus[cu_id as usize];
                if self.si.sh.num_ref_idx[l] > 0 && (cur.inter_dir & (1 << l)) != 0 {
                    let mut mvd = cur.mv[l][0];
                    let apply =
                        l == 0 || !(self.si.ph.mvd_l1_zero && cur.inter_dir == 3) || cur.imv == 0;
                    if apply {
                        mvd = mvd.change_precision_amvr(cur.imv, MV_PRECISION_INTERNAL);
                    }
                    let mvp_idx = cur.mvp_idx[l] as usize;
                    let ref_idx = cur.ref_idx[l];
                    let cands = self.fill_mvp(cu_id, l, ref_idx);
                    self.pic.cus[cu_id as usize].mv[l][0] = cands[mvp_idx].add(mvd).wrap_storage();
                }
            }
            self.span_motion(cu_id);
        }
        // history update
        let c = &self.pic.cus[cu_id as usize];
        if !c.affine && !c.geo {
            let p = self.plevel();
            let (x, y, w, h) = (c.lx(), c.ly(), c.lw(), c.lh());
            let enable = ((x + w) >> p) > (x >> p) && ((y + h) >> p) > (y >> p);
            if enable {
                let mi = self.pic.mi(x, y);
                let info = HpMvInfo {
                    mi,
                    bcw: if c.inter_dir == 3 { c.bcw } else { 0 },
                    alt_hpel: c.imv == IMV_HPEL,
                };
                HpMvInfo::add_to_lut(&mut self.pic.hmvp, info);
            }
        }
        Ok(())
    }
}

/// `PU::restrictBiPredMergeCandsOne`.
fn restrict_bi(c: &mut Cu) {
    if c.lw() + c.lh() <= 12 && c.inter_dir == 3 {
        c.inter_dir = 1;
        c.ref_idx[1] = -1;
        c.mv[1][0] = Mv::default();
        c.bcw = 0;
    }
}

impl<'a, 's, 'b> CtuDecoder<'a, 's, 'b> {
    /// `PU::getGeoMergeCandidates` + `PU::spanGeoMotionInfo`.
    fn derive_geo(&mut self, cu_id: u32) {
        let tmp = self.merge_candidates(cu_id, None);
        let max = self.si.sps.max_num_merge_cand as usize;
        let mut geo: Vec<MergeCand> = Vec::with_capacity(6);
        for (i, t) in tmp.iter().enumerate().take(max) {
            let parity = i & 1;
            if t.dir & (1 + parity as u8) != 0 {
                let mut g = MergeCand {
                    dir: 1 + parity as u8,
                    ..Default::default()
                };
                g.mv[parity] = t.mv[parity];
                g.ref_idx[parity] = t.ref_idx[parity];
                geo.push(g);
                if geo.len() == 6 {
                    break;
                }
                continue;
            }
            if t.dir & (2 - parity as u8) != 0 {
                let np = 1 - parity;
                let mut g = MergeCand {
                    dir: 2 - parity as u8,
                    ..Default::default()
                };
                g.mv[np] = t.mv[np];
                g.ref_idx[np] = t.ref_idx[np];
                geo.push(g);
                if geo.len() == 6 {
                    break;
                }
            }
        }
        while geo.len() < 6 {
            geo.push(MergeCand::default());
        }
        let c = self.pic.cus[cu_id as usize].clone();
        let (i0, i1) = (c.geo_idx[0] as usize, c.geo_idx[1] as usize);
        let off0 = if geo[i0].dir == 1 { 0 } else { 1 };
        let off1 = if geo[i1].dir == 1 { 0 } else { 1 };
        let mut bi = MotionInfo::default();
        match (geo[i0].dir, geo[i1].dir) {
            (1, 2) => {
                bi.mv = [geo[i0].mv[0], geo[i1].mv[1]];
                bi.ref_idx = [geo[i0].ref_idx[0], geo[i1].ref_idx[1]];
            }
            (2, 1) => {
                bi.mv = [geo[i1].mv[0], geo[i0].mv[1]];
                bi.ref_idx = [geo[i1].ref_idx[0], geo[i0].ref_idx[1]];
            }
            (1, 1) => {
                bi.mv[0] = geo[i1].mv[0];
                bi.ref_idx[0] = geo[i1].ref_idx[0];
            }
            (2, 2) => {
                bi.mv[1] = geo[i1].mv[1];
                bi.ref_idx[1] = geo[i1].ref_idx[1];
            }
            _ => {}
        }
        {
            let cm = &mut self.pic.cus[cu_id as usize];
            cm.mv[0][1] = geo[i0].mv[off0];
            cm.mv[1][1] = geo[i1].mv[off1];
            cm.geo_dir_ref[0] = (geo[i0].dir << 4).wrapping_add(geo[i0].ref_idx[off0] as u8);
            cm.geo_dir_ref[1] = (geo[i1].dir << 4).wrapping_add(geo[i1].ref_idx[off1] as u8);
        }
        let (angle, dist_idx) = super::tables_inter::geo_params(c.geo_split as usize);
        let is_flip = (13..=27).contains(&angle);
        let dis = super::tables_inter::GEO_DIS;
        let distance_x = angle;
        let distance_y = (distance_x + 8) % 32;
        let (w, h) = (c.lw(), c.lh());
        let mut offset_x = (-w) >> 1;
        let mut offset_y = (-h) >> 1;
        if dist_idx > 0 {
            if angle % 16 == 8 || (angle % 16 != 0 && h >= w) {
                offset_y += if angle < 16 {
                    (dist_idx * h) >> 3
                } else {
                    -((dist_idx * h) >> 3)
                };
            } else {
                offset_x += if angle < 16 {
                    (dist_idx * w) >> 3
                } else {
                    -((dist_idx * w) >> 3)
                };
            }
        }
        let mw = self.pic.map_w;
        let (x0, y0) = (c.lx() >> 2, c.ly() >> 2);
        for y in 0..h >> 2 {
            let look_y = (((4 * y + offset_y) * 2) + 5) * i32::from(dis[distance_y as usize]);
            for x in 0..w >> 2 {
                let idx =
                    (((4 * x + offset_x) * 2) + 5) * i32::from(dis[distance_x as usize]) + look_y;
                let mask = if idx.abs() < 32 {
                    2
                } else if idx <= 0 {
                    i32::from(!is_flip)
                } else {
                    i32::from(is_flip)
                };
                let d = &mut self.pic.motion[(y0 + y) as usize * mw + (x0 + x) as usize];
                let g = if mask == 0 { &geo[i0] } else { &geo[i1] };
                if mask == 2 {
                    *d = bi;
                } else {
                    d.ref_idx = g.ref_idx;
                    d.mv = g.mv;
                }
            }
        }
    }

    /// `PU::xInheritedAffineMv`.
    fn inherited_affine_mv(&self, cu_id: u32, six: bool, nb: u32, l: usize) -> [Mv; 3] {
        let c = &self.pic.cus[cu_id as usize];
        let n = &self.pic.cus[nb as usize];
        let nx = n.lx();
        let mut ny = n.ly();
        let (cx, cy) = (c.lx(), c.ly());
        let (nw, nh, cw, ch) = (n.lw(), n.lh(), c.lw(), c.lh());
        let mut lt = n.mv[l][0];
        let mut rt = n.mv[l][1];
        let lb = n.mv[l][2];
        let ctu = 1i32 << self.si.sps.log2_ctu_size;
        let mut top_ctu = false;
        if (ny + nh) % ctu == 0 && ny + nh == cy {
            lt = self.pic.mi(n.lx(), n.ly() + nh - 1).mv[l];
            rt = self.pic.mi(n.lx() + nw - 1, n.ly() + nh - 1).mv[l];
            ny += nh;
            top_ctu = true;
        }
        let shift = 7;
        let hx = (rt.x - lt.x) * (1 << (shift - floor_log2(nw)));
        let hy = (rt.y - lt.y) * (1 << (shift - floor_log2(nw)));
        let (vx, vy) = if n.affine_type == 1 && !top_ctu {
            (
                (lb.x - lt.x) * (1 << (shift - floor_log2(nh))),
                (lb.y - lt.y) * (1 << (shift - floor_log2(nh))),
            )
        } else {
            (-hy, hx)
        };
        let (sx, sy) = (lt.x * (1 << shift), lt.y * (1 << shift));
        let calc = |dx: i32, dy: i32| {
            let (x, y) = round_affine(sx + hx * dx + vx * dy, sy + hy * dx + vy * dy, shift);
            Mv::new(x, y).clip_storage()
        };
        let mut out = [Mv::default(); 3];
        out[0] = calc(cx - nx, cy - ny);
        out[1] = calc(cx + cw - nx, cy - ny);
        if six {
            out[2] = calc(cx - nx, cy + ch - ny);
        }
        out
    }

    /// `PU::setAllAffineMv`: control points and the 4x4 subblock motion.
    fn set_all_affine_mv(&mut self, cu_id: u32, lt: Mv, rt: Mv, lb: Mv, l: usize, clip: bool) {
        let c = self.pic.cus[cu_id as usize].clone();
        let (w, h) = (c.lw(), c.lh());
        let shift = 7;
        let (mut lt, mut rt, mut lb) = (lt, rt, lb);
        if clip {
            lt = lt.wrap_storage();
            rt = rt.wrap_storage();
            if c.affine_type == 1 {
                lb = lb.wrap_storage();
            }
        }
        let hx = (rt.x - lt.x) * (1 << (shift - floor_log2(w)));
        let hy = (rt.y - lt.y) * (1 << (shift - floor_log2(w)));
        let (vx, vy) = if c.affine_type == 1 {
            (
                (lb.x - lt.x) * (1 << (shift - floor_log2(h))),
                (lb.y - lt.y) * (1 << (shift - floor_log2(h))),
            )
        } else {
            (-hy, hx)
        };
        let (sx, sy) = (lt.x * (1 << shift), lt.y * (1 << shift));
        let spread = subblock_spread_over_limit(hx, hy, vx, vy, c.inter_dir);
        let flb = if spread {
            let (x, y) = round_affine(
                sx + hx * (w >> 1) + vx * (h >> 1),
                sy + hy * (w >> 1) + vy * (h >> 1),
                shift,
            );
            Mv::new(x, y).clip_storage()
        } else {
            Mv::default()
        };
        let mw = self.pic.map_w;
        let (x0, y0) = (c.lx() >> 2, c.ly() >> 2);
        for y in 0..h >> 2 {
            for x in 0..w >> 2 {
                let m = if spread {
                    flb
                } else {
                    let (mx, my) = round_affine(
                        sx + hx * (2 + (x << 2)) + vx * (2 + (y << 2)),
                        sy + hy * (2 + (x << 2)) + vy * (2 + (y << 2)),
                        shift,
                    );
                    Mv::new(mx, my).clip_storage()
                };
                self.pic.motion[(y0 + y) as usize * mw + (x0 + x) as usize].mv[l] = m;
            }
        }
        let cm = &mut self.pic.cus[cu_id as usize];
        cm.mv[l] = [lt, rt, lb];
    }

    /// `clipColPos`.
    fn clip_col_pos(&self, cu_id: u32, x: i32, y: i32) -> (i32, i32) {
        let c = &self.pic.cus[cu_id as usize];
        let log2 = self.si.sps.log2_ctu_size;
        let ctu = 1i32 << log2;
        let (ctux, ctuy) = ((c.lx() >> log2) << log2, (c.ly() >> log2) << log2);
        let hor_max = match subpic_at(self.si.pps, c.lx(), c.ly()) {
            Some(sp) if sp.treated_as_pic => (sp.right as i32).min(ctux + ctu + 3),
            _ => (self.si.pps.width as i32 - 1).min(ctux + ctu + 3),
        };
        let hor_min = ctux.max(0);
        let ver_max = (self.si.pps.height as i32 - 1).min(ctuy + ctu - 1);
        let ver_min = ctuy.max(0);
        (hor_max.min(hor_min.max(x)), ver_max.min(ver_min.max(y)))
    }

    /// `PU::getInterMergeSubPuMvpCand`; writes the subblock motion when
    /// `fill` is set.
    fn sbtmvp_cand(
        &mut self,
        cu_id: u32,
        first: Option<&AffineCand>,
        fill: bool,
    ) -> Option<AffineCand> {
        let col = self.si.inter.col.clone()?;
        let is_b = self.is_b();
        let c = self.pic.cus[cu_id as usize].clone();
        let mut tmv = Mv::default();
        if let Some(f) = first {
            let col_is = |l: usize, r: i8| {
                r >= 0
                    && self.si.inter.refs[l]
                        .get(r as usize)
                        .is_some_and(|p| p.id == col.id)
            };
            if f.dir & 1 != 0 && col_is(0, f.ref_idx[0]) {
                tmv = f.mv[0][0];
            } else if is_b && f.dir & 2 != 0 && col_is(1, f.ref_idx[1]) {
                tmv = f.mv[1][0];
            }
        }
        let t = tmv.change_precision(MV_PRECISION_INTERNAL, MV_PRECISION_INT);
        let (cx, cy) = self.clip_col_pos(
            cu_id,
            c.lx() + (c.lw() >> 1) + t.x,
            c.ly() + (c.lh() >> 1) + t.y,
        );
        let (cx, cy) = (cx & !7, cy & !7);
        let (mi, _) = col.col_info(cx, cy);
        let mut cand = AffineCand {
            merge_type: MRG_TYPE_SUBPU_ATMVP,
            ..Default::default()
        };
        let mut found = false;
        if mi.is_inter() {
            cand.dir = 0;
            for l in 0..if is_b { 2 } else { 1 } {
                if let Some(m) = self.col_mvp(cu_id, l, cx, cy, 0, true) {
                    cand.mv[l][0] = m;
                    cand.ref_idx[l] = 0;
                    cand.dir |= 1 << l;
                    found = true;
                } else {
                    cand.mv[l][0] = Mv::default();
                    cand.ref_idx[l] = -1;
                    cand.dir &= !(1 << l);
                }
            }
        }
        if !found {
            return None;
        }
        if !fill {
            return Some(cand);
        }
        let bi_restrict = c.lw() + c.lh() <= 12;
        let (xoff, yoff) = (4 + t.x, 4 + t.y);
        let mw = self.pic.map_w;
        let mut y = c.ly();
        while y < c.ly() + c.lh() {
            let mut x = c.lx();
            while x < c.lx() + c.lw() {
                let mut sub = MotionInfo::default();
                let (px, py) = self.clip_col_pos(cu_id, x + xoff, y + yoff);
                let (cmi, _) = col.col_info(px, py);
                let mut ok = false;
                if cmi.is_inter() {
                    for l in 0..if !bi_restrict && is_b { 2 } else { 1 } {
                        if let Some(m) = self.col_mvp(cu_id, l, px, py, 0, true) {
                            sub.ref_idx[l] = 0;
                            sub.mv[l] = m;
                            ok = true;
                        }
                    }
                }
                if !ok {
                    sub.mv = [cand.mv[0][0], cand.mv[1][0]];
                    sub.ref_idx = cand.ref_idx;
                }
                if bi_restrict && sub.inter_dir() == 3 {
                    sub.mv[1] = Mv::default();
                    sub.ref_idx[1] = -1;
                }
                let (ux, uy) = ((x >> 2) as usize, (y >> 2) as usize);
                for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                    self.pic.motion[(uy + dy) * mw + ux + dx] = sub;
                }
                x += 8;
            }
            y += 8;
        }
        Some(cand)
    }

    /// Affine neighbours for inherited candidates (left, then above).
    fn affine_neighbours(&self, cu_id: u32) -> Vec<u32> {
        let c = &self.pic.cus[cu_id as usize];
        let (lx, ly, w, h) = (c.lx(), c.ly(), c.lw(), c.lh());
        let ok = |id: Option<u32>, x: i32, y: i32| -> Option<u32> {
            id.filter(|&n| {
                let nc = &self.pic.cus[n as usize];
                nc.affine && nc.merge_type == MRG_TYPE_DEFAULT_N && self.diff_mer(cu_id, x, y)
            })
        };
        let mut out = Vec::new();
        let pos_lb = (lx, ly + h - 1);
        if let Some(n) = ok(
            self.restricted(cu_id, pos_lb.0 - 1, pos_lb.1 + 1, c.left),
            pos_lb.0 - 1,
            pos_lb.1 + 1,
        )
        .or_else(|| {
            ok(
                self.restricted(cu_id, pos_lb.0 - 1, pos_lb.1, c.left),
                pos_lb.0 - 1,
                pos_lb.1,
            )
        }) {
            out.push(n);
        }
        let pos_rt = (lx + w - 1, ly);
        let guess = if c.left.is_some() { c.left } else { c.above };
        if let Some(n) = ok(
            self.restricted(cu_id, pos_rt.0 + 1, pos_rt.1 - 1, c.above),
            pos_rt.0 + 1,
            pos_rt.1 - 1,
        )
        .or_else(|| {
            ok(
                self.restricted(cu_id, pos_rt.0, pos_rt.1 - 1, c.above),
                pos_rt.0,
                pos_rt.1 - 1,
            )
        })
        .or_else(|| {
            ok(
                self.restricted(cu_id, lx - 1, ly - 1, guess),
                lx - 1,
                ly - 1,
            )
        }) {
            out.push(n);
        }
        out
    }

    /// `PU::getAffineMergeCand`, stopping at the signalled index.
    fn affine_merge_list(&mut self, cu_id: u32, stop: usize) -> Vec<AffineCand> {
        let max = self.si.ph.max_num_affine_merge_cand as usize;
        let is_b = self.is_b();
        let c = self.pic.cus[cu_id as usize].clone();
        let mut list: Vec<AffineCand> = Vec::with_capacity(5);
        let irap_self_ref = self.si.inter.info.ref_poc[0].first() == Some(&self.si.inter.info.poc)
            && (7..=9).contains(&self.si.sh.nal_type);
        let enable_sub = self.si.sps.sbtmvp && !irap_self_ref;
        if enable_sub && self.si.ph.temporal_mvp {
            let pos_lb = (c.lx(), c.ly() + c.lh() - 1);
            let left = self.restricted(cu_id, pos_lb.0 - 1, pos_lb.1, c.left);
            let first = left
                .filter(|&l| self.is_inter_cu(l) && self.diff_mer(cu_id, pos_lb.0 - 1, pos_lb.1))
                .map(|_| {
                    let mi = self.pic.mi(pos_lb.0 - 1, pos_lb.1);
                    let mut a = AffineCand {
                        dir: mi.inter_dir(),
                        ..Default::default()
                    };
                    a.mv[0][0] = mi.mv[0];
                    a.ref_idx[0] = mi.ref_idx[0];
                    if is_b {
                        a.mv[1][0] = mi.mv[1];
                        a.ref_idx[1] = mi.ref_idx[1];
                    }
                    a
                });
            if let Some(cand) = self.sbtmvp_cand(cu_id, first.as_ref(), c.merge_idx == 0) {
                list.push(cand);
                if list.len() == stop + 1 || list.len() == max {
                    return list;
                }
            }
        }
        if self.si.sps.affine {
            for nb in self.affine_neighbours(cu_id) {
                let n = self.pic.cus[nb as usize].clone();
                let mut cand = AffineCand::default();
                let six = n.affine_type == 1;
                if n.inter_dir != 2 {
                    cand.mv[0] = self.inherited_affine_mv(cu_id, six, nb, 0);
                }
                if is_b && n.inter_dir != 1 {
                    cand.mv[1] = self.inherited_affine_mv(cu_id, six, nb, 1);
                }
                cand.ref_idx = n.ref_idx;
                cand.dir = n.inter_dir;
                cand.affine_type = n.affine_type;
                cand.bcw = n.bcw;
                list.push(cand);
                if list.len() == stop + 1 || list.len() == max {
                    return list;
                }
            }
            // constructed candidates
            let mut mi = [MotionInfo::default(); 4];
            let mut avail = [false; 4];
            let mut nb_bcw = [0u8; 2];
            let (lx, ly, w, h) = (c.lx(), c.ly(), c.lw(), c.lh());
            let guess_lt = [
                if c.left.is_some() { c.left } else { c.above },
                c.above,
                c.left,
            ];
            for (i, p) in [(lx - 1, ly - 1), (lx, ly - 1), (lx - 1, ly)]
                .iter()
                .enumerate()
            {
                if let Some(n) = self.restricted(cu_id, p.0, p.1, guess_lt[i])
                    && self.is_inter_cu(n)
                    && self.diff_mer(cu_id, p.0, p.1)
                {
                    avail[0] = true;
                    mi[0] = self.pic.mi(p.0, p.1);
                    nb_bcw[0] = self.pic.cus[n as usize].bcw;
                    break;
                }
            }
            for p in [(lx + w - 1, ly - 1), (lx + w, ly - 1)] {
                if let Some(n) = self.restricted(cu_id, p.0, p.1, c.above)
                    && self.is_inter_cu(n)
                    && self.diff_mer(cu_id, p.0, p.1)
                {
                    avail[1] = true;
                    mi[1] = self.pic.mi(p.0, p.1);
                    nb_bcw[1] = self.pic.cus[n as usize].bcw;
                    break;
                }
            }
            for p in [(lx - 1, ly + h - 1), (lx - 1, ly + h)] {
                if let Some(n) = self.restricted(cu_id, p.0, p.1, c.left)
                    && self.is_inter_cu(n)
                    && self.diff_mer(cu_id, p.0, p.1)
                {
                    avail[2] = true;
                    mi[2] = self.pic.mi(p.0, p.1);
                    break;
                }
            }
            if self.si.ph.temporal_mvp {
                let (c0, _) = self.col_c0(cu_id);
                if let Some(p) = c0 {
                    if let Some(m) = self.col_mvp(cu_id, 0, p.0, p.1, 0, false) {
                        mi[3].mv[0] = m;
                        mi[3].ref_idx[0] = 0;
                        avail[3] = true;
                    }
                    if is_b && let Some(m) = self.col_mvp(cu_id, 1, p.0, p.1, 0, false) {
                        mi[3].mv[1] = m;
                        mi[3].ref_idx[1] = 0;
                        avail[3] = true;
                    }
                }
            }
            const MODELS: [[usize; 3]; 6] = [
                [0, 1, 2],
                [0, 1, 3],
                [0, 2, 3],
                [1, 2, 3],
                [0, 1, 0],
                [0, 2, 0],
            ];
            let start = if self.si.sps.affine_type { 0 } else { 4 };
            for m in start..6 {
                let ver_num = if m < 4 { 3 } else { 2 };
                let bcw = if m == 3 { nb_bcw[1] } else { nb_bcw[0] };
                if let Some(cand) =
                    control_point_cand(&c, &mi, &avail, &MODELS[m][..ver_num], bcw, m)
                {
                    list.push(cand);
                    if list.len() == stop + 1 || list.len() == max {
                        return list;
                    }
                }
            }
        }
        while list.len() < max {
            let mut cand = AffineCand {
                dir: 1,
                ..Default::default()
            };
            cand.ref_idx[0] = 0;
            if is_b {
                cand.dir = 3;
                cand.ref_idx[1] = 0;
            }
            list.push(cand);
        }
        list
    }

    fn derive_affine_merge(&mut self, cu_id: u32) -> Result<(), Error> {
        let idx = self.pic.cus[cu_id as usize].merge_idx as usize;
        let list = self.affine_merge_list(cu_id, idx);
        let cand = *list
            .get(idx)
            .ok_or(Error::Invalid("affine merge candidate"))?;
        {
            let c = &mut self.pic.cus[cu_id as usize];
            c.inter_dir = cand.dir;
            c.affine_type = cand.affine_type;
            c.bcw = cand.bcw;
            c.merge_type = cand.merge_type;
        }
        if cand.merge_type == MRG_TYPE_SUBPU_ATMVP {
            let c = &mut self.pic.cus[cu_id as usize];
            c.ref_idx = cand.ref_idx;
            // CU::setBcwIdx for subblock merge
            c.bcw = 0;
        } else {
            for l in 0..2 {
                if self.si.sh.num_ref_idx[l] > 0 {
                    self.pic.cus[cu_id as usize].ref_idx[l] = cand.ref_idx[l];
                    self.set_all_affine_mv(
                        cu_id,
                        cand.mv[l][0],
                        cand.mv[l][1],
                        cand.mv[l][2],
                        l,
                        false,
                    );
                }
            }
        }
        Ok(())
    }

    /// `PU::addAffineMVPCandUnscaled`.
    fn affine_mvp_unscaled(
        &self,
        cu_id: u32,
        l: usize,
        ref_idx: i8,
        pos: (i32, i32),
        dir: Dir,
    ) -> Option<[Mv; 3]> {
        let c = &self.pic.cus[cu_id as usize];
        let (guess, np) = match dir {
            Dir::Left => (c.left, (pos.0 - 1, pos.1)),
            Dir::Above => (c.above, (pos.0, pos.1 - 1)),
            Dir::AboveRight => (c.above, (pos.0 + 1, pos.1 - 1)),
            Dir::BelowLeft => (c.left, (pos.0 - 1, pos.1 + 1)),
            Dir::AboveLeft => (
                if c.left.is_some() { c.left } else { c.above },
                (pos.0 - 1, pos.1 - 1),
            ),
        };
        let n = self.restricted(cu_id, np.0, np.1, guess)?;
        let nc = &self.pic.cus[n as usize];
        if nc.pred != Pred::Inter || !nc.affine || nc.merge_type != MRG_TYPE_DEFAULT_N {
            return None;
        }
        let mi = self.pic.mi(np.0, np.1);
        let cur_poc = self.ref_poc(l, ref_idx);
        for src in 0..2 {
            let li = if src == 0 { l } else { 1 - l };
            let nr = mi.ref_idx[li];
            if (nc.inter_dir & (li as u8 + 1)) == 0 || self.ref_poc(li, nr) != cur_poc {
                continue;
            }
            let six = c.affine_type == 1;
            let mut out = self.inherited_affine_mv(cu_id, six, n, li);
            let prec = match c.imv {
                0 => Some(MV_PRECISION_QUARTER),
                2 => Some(MV_PRECISION_INT),
                _ => None,
            };
            if let Some(p) = prec {
                for m in out.iter_mut().take(if six { 3 } else { 2 }) {
                    *m = m.round_to_precision(MV_PRECISION_INTERNAL, p);
                }
            }
            return Some(out);
        }
        None
    }

    /// `PU::fillAffineMvpCand`.
    fn fill_affine_mvp(&self, cu_id: u32, l: usize, ref_idx: i8) -> Vec<[Mv; 3]> {
        let c = self.pic.cus[cu_id as usize].clone();
        let mut out: Vec<[Mv; 3]> = Vec::with_capacity(3);
        if ref_idx < 0 {
            return out;
        }
        let pos_lt = (c.lx(), c.ly());
        let pos_rt = (c.lx() + c.lw() - 1, c.ly());
        let pos_lb = (c.lx(), c.ly() + c.lh() - 1);
        if let Some(m) = self
            .affine_mvp_unscaled(cu_id, l, ref_idx, pos_lb, Dir::BelowLeft)
            .or_else(|| self.affine_mvp_unscaled(cu_id, l, ref_idx, pos_lb, Dir::Left))
        {
            out.push(m);
        }
        if let Some(m) = self
            .affine_mvp_unscaled(cu_id, l, ref_idx, pos_rt, Dir::AboveRight)
            .or_else(|| self.affine_mvp_unscaled(cu_id, l, ref_idx, pos_rt, Dir::Above))
            .or_else(|| self.affine_mvp_unscaled(cu_id, l, ref_idx, pos_lt, Dir::AboveLeft))
        {
            out.push(m);
        }
        let to_quarter = |v: &mut Vec<[Mv; 3]>| {
            if c.imv != 1 {
                for cand in v.iter_mut() {
                    for m in cand.iter_mut() {
                        *m = m.change_precision(MV_PRECISION_INTERNAL, MV_PRECISION_QUARTER);
                    }
                }
            }
        };
        if out.len() >= 2 {
            to_quarter(&mut out);
            return out;
        }
        let round = |m: Mv| match c.imv {
            0 => m.round_to_precision(MV_PRECISION_INTERNAL, MV_PRECISION_QUARTER),
            2 => m.round_to_precision(MV_PRECISION_INTERNAL, MV_PRECISION_INT),
            _ => m,
        };
        let v0 = self
            .mvp_cand_unscaled(cu_id, l, ref_idx, pos_lt, Dir::AboveLeft)
            .or_else(|| self.mvp_cand_unscaled(cu_id, l, ref_idx, pos_lt, Dir::Above))
            .or_else(|| self.mvp_cand_unscaled(cu_id, l, ref_idx, pos_lt, Dir::Left));
        let v1 = self
            .mvp_cand_unscaled(cu_id, l, ref_idx, pos_rt, Dir::Above)
            .or_else(|| self.mvp_cand_unscaled(cu_id, l, ref_idx, pos_rt, Dir::AboveRight));
        let v2 = self
            .mvp_cand_unscaled(cu_id, l, ref_idx, pos_lb, Dir::Left)
            .or_else(|| self.mvp_cand_unscaled(cu_id, l, ref_idx, pos_lb, Dir::BelowLeft));
        let pattern =
            u8::from(v0.is_some()) | (u8::from(v1.is_some()) << 1) | (u8::from(v2.is_some()) << 2);
        let corner = [
            round(v0.unwrap_or_default()),
            round(v1.unwrap_or_default()),
            round(v2.unwrap_or_default()),
        ];
        if pattern == 7 || (pattern == 3 && c.affine_type == 0) {
            out.push(corner);
        }
        if out.len() < 2 {
            for i in (0..3).rev() {
                if out.len() >= 2 {
                    break;
                }
                if pattern & (1 << i) != 0 {
                    out.push([corner[i]; 3]);
                }
            }
            if out.len() < 2 && self.si.ph.temporal_mvp {
                let (c0, c1) = self.col_c0(cu_id);
                if let Some(m) = c0
                    .and_then(|p| self.col_mvp(cu_id, l, p.0, p.1, ref_idx, false))
                    .or_else(|| self.col_mvp(cu_id, l, c1.0, c1.1, ref_idx, false))
                {
                    out.push([round(m); 3]);
                }
            }
            while out.len() < 2 {
                out.push([Mv::default(); 3]);
            }
        }
        to_quarter(&mut out);
        out
    }

    /// Affine AMVP part of `DecCu::xDeriveCUMV`.
    fn derive_affine_amvp(&mut self, cu_id: u32) {
        for l in 0..2 {
            let c = self.pic.cus[cu_id as usize].clone();
            if self.si.sh.num_ref_idx[l] == 0 || (c.inter_dir & (1 << l)) == 0 {
                continue;
            }
            let cands = self.fill_affine_mvp(cu_id, l, c.ref_idx[l]);
            let p = cands[c.mvp_idx[l] as usize];
            let shift = if c.imv == 2 { 2 } else { 0 };
            let mv0 = c.mv[l][0].shl(shift);
            let mv1 = c.mv[l][1].shl(shift);
            let mut lt = p[0].add(mv0);
            let mut rt = p[1].add(mv1).add(mv0);
            if c.imv != 1 {
                lt = lt.change_precision(MV_PRECISION_QUARTER, MV_PRECISION_INTERNAL);
                rt = rt.change_precision(MV_PRECISION_QUARTER, MV_PRECISION_INTERNAL);
            }
            let mut lb = Mv::default();
            if c.affine_type == 1 {
                let mv2 = c.mv[l][2].shl(shift);
                lb = p[2].add(mv2).add(mv0);
                if c.imv != 1 {
                    lb = lb.change_precision(MV_PRECISION_QUARTER, MV_PRECISION_INTERNAL);
                }
            }
            self.set_all_affine_mv(cu_id, lt, rt, lb, l, true);
        }
    }
}

/// `PU::getAffineControlPointCand`.
fn control_point_cand(
    c: &Cu,
    mi: &[MotionInfo; 4],
    avail: &[bool; 4],
    ver: &[usize],
    bcw: u8,
    model: usize,
) -> Option<AffineCand> {
    if ver.iter().any(|&v| !avail[v]) {
        return None;
    }
    let mut dir = 0u8;
    let mut ref_idx = [-1i8; 2];
    for l in 0..2 {
        if ver.iter().all(|&v| mi[v].ref_idx[l] >= 0)
            && ver
                .iter()
                .all(|&v| mi[v].ref_idx[l] == mi[ver[0]].ref_idx[l])
        {
            dir |= (l + 1) as u8;
            ref_idx[l] = mi[ver[0]].ref_idx[l];
        }
    }
    if dir == 0 {
        return None;
    }
    let shift = 7;
    let shift_hw = shift + floor_log2(c.lw()) - floor_log2(c.lh());
    let mut cmv = [[Mv::default(); 4]; 2];
    for l in 0..2 {
        if dir & (l as u8 + 1) == 0 {
            continue;
        }
        for &v in ver {
            cmv[l][v] = mi[v].mv[l];
        }
        match model {
            1 => {
                cmv[l][2] = Mv::new(
                    cmv[l][3].x + cmv[l][0].x - cmv[l][1].x,
                    cmv[l][3].y + cmv[l][0].y - cmv[l][1].y,
                )
                .clip_storage();
            }
            2 => {
                cmv[l][1] = Mv::new(
                    cmv[l][3].x + cmv[l][0].x - cmv[l][2].x,
                    cmv[l][3].y + cmv[l][0].y - cmv[l][2].y,
                )
                .clip_storage();
            }
            3 => {
                cmv[l][0] = Mv::new(
                    cmv[l][1].x + cmv[l][2].x - cmv[l][3].x,
                    cmv[l][1].y + cmv[l][2].y - cmv[l][3].y,
                )
                .clip_storage();
            }
            5 => {
                let vx = cmv[l][0].x * (1 << shift) + (cmv[l][2].y - cmv[l][0].y) * (1 << shift_hw);
                let vy = cmv[l][0].y * (1 << shift) - (cmv[l][2].x - cmv[l][0].x) * (1 << shift_hw);
                let (x, y) = round_affine(vx, vy, shift);
                cmv[l][1] = Mv::new(x, y).clip_storage();
            }
            _ => {}
        }
    }
    let mut cand = AffineCand {
        dir,
        ref_idx,
        affine_type: if ver.len() == 2 { 0 } else { 1 },
        bcw: if dir == 3 { bcw } else { 0 },
        ..Default::default()
    };
    for l in 0..2 {
        cand.mv[l] = [cmv[l][0], cmv[l][1], cmv[l][2]];
    }
    Some(cand)
}
