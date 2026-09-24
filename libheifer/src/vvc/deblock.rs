// SPDX-License-Identifier: LGPL-3.0-or-later
//! Deblocking filter (H.266 clause 8.8.3), following vvdec's `LoopFilter`:
//! edge parameters are derived per coding unit into 4x4 luma-grid maps,
//! then all vertical and afterwards all horizontal edges are filtered.
use super::pic::{Cu, Picture, Pred, Tree, Tu};
use super::ps::{I_SLICE, PicHeader, Pps, SliceHeader, Sps};

const MARK: u8 = 3 << 6;

fn bs_set(v: u8, comp: usize) -> u8 {
    v << (comp << 1)
}

fn bs_get(bs: u8, comp: usize) -> u8 {
    (bs >> (comp << 1)) & 3
}

#[derive(Clone, Copy, Default)]
struct Lfp {
    qp: [i32; 3],
    bs: u8,
    side_max: u8,
    edge: [bool; 2],
    cmfl: bool,
}

const TC_TABLE: [u16; 66] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 4, 4, 4, 4, 5, 5, 5, 5, 7, 7, 8, 9,
    10, 10, 11, 13, 14, 15, 17, 19, 21, 24, 25, 29, 33, 36, 41, 45, 51, 57, 64, 71, 80, 89, 100,
    112, 125, 141, 157, 177, 198, 222, 250, 280, 314, 352, 395,
];
const BETA_TABLE: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
    20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50, 52, 54, 56, 58, 60, 62, 64, 66,
    68, 70, 72, 74, 76, 78, 80, 82, 84, 86, 88,
];

/// Edge direction: 0 = vertical edges, 1 = horizontal edges (vvdec's
/// `EDGE_VER`/`EDGE_HOR`).
type Dir = usize;

struct Ctx<'a> {
    pic: &'a Picture,
    sps: &'a Sps,
    pps: &'a Pps,
    ph: &'a PicHeader,
    slices: &'a [SliceHeader],
    maps: [Vec<Lfp>; 2],
}

fn perp(dir: Dir, x: i32, y: i32) -> i32 {
    if dir == 0 { x } else { y }
}

fn parl(dir: Dir, x: i32, y: i32) -> i32 {
    if dir == 0 { y } else { x }
}

impl<'a> Ctx<'a> {
    fn comp(ch: usize) -> usize {
        ch
    }

    fn scale(&self, ch: usize) -> (i32, i32) {
        let (sx, sy) = self.pic.fmt.scale(ch);
        (sx as i32, sy as i32)
    }

    /// Map index of a position in channel `ch` coordinates.
    fn idx(&self, ch: usize, x: i32, y: i32) -> usize {
        let (sx, sy) = self.scale(ch);
        (((y << sy) >> 2) as usize) * self.pic.map_w + ((x << sx) >> 2) as usize
    }

    fn cu(&self, id: u32) -> &Cu {
        &self.pic.cus[id as usize]
    }

    fn tu(&self, id: u32) -> &Tu {
        &self.pic.tus[id as usize]
    }

    fn get_cu(&self, x: i32, y: i32, ch: usize) -> Option<u32> {
        self.pic.get_cu(x, y, ch)
    }

    fn get_tu(&self, cu: u32, x: i32, y: i32, ch: usize) -> u32 {
        self.pic.get_tu(cu, x, y, ch)
    }

    fn last_tu(&self, cu: u32) -> u32 {
        let c = self.cu(cu);
        c.first_tu + c.num_tu - 1
    }

    fn dual(&self, c: &Cu) -> bool {
        self.slices[c.slice as usize].slice_type == I_SLICE && self.sps.dual_tree
    }

    fn subpic_of(&self, c: &Cu) -> usize {
        if !self.sps.subpic_info_present {
            return 0;
        }
        let ctu = 1i32 << self.pic.ctu_log2;
        let (cx, cy) = ((c.lx().max(0) / ctu) as u32, (c.ly().max(0) / ctu) as u32);
        let (cx, cy) = if c.blk[0].valid() {
            (cx, cy)
        } else {
            let (sx, sy) = self.scale(1);
            (
                ((c.blk[1].x << sx) / ctu) as u32,
                ((c.blk[1].y << sy) / ctu) as u32,
            )
        };
        for i in 0..self.sps.num_subpics as usize {
            let (x, y, w, h) = (
                self.sps.subpic_x[i],
                self.sps.subpic_y[i],
                self.sps.subpic_w[i],
                self.sps.subpic_h[i],
            );
            if cx >= x && cx < x + w && cy >= y && cy < y + h {
                return i;
            }
        }
        0
    }

    fn available(&self, a: &Cu, b: &Cu) -> bool {
        let sub_ok = !self.sps.subpic_info_present || {
            let (sa, sb) = (self.subpic_of(a), self.subpic_of(b));
            self.sps.loop_filter_across_subpic[sa] && self.sps.loop_filter_across_subpic[sb]
        };
        (self.pps.loop_filter_across_slices || a.slice == b.slice)
            && (self.pps.loop_filter_across_tiles || a.tile == b.tile)
            && (sub_ok || self.subpic_of(a) == self.subpic_of(b))
    }

    /// vvdec's `xGetLoopfilterParam`: (left edge, top edge).
    fn cu_edges(&self, id: u32) -> (bool, bool) {
        let c = self.cu(id);
        let ch = c.ch_type;
        let b = c.blk[Self::comp(ch)];
        let mut left = false;
        let mut top = false;
        if b.x > 0
            && let Some(l) = c.left.or_else(|| self.get_cu(b.x - 1, b.y, ch))
        {
            left = self.available(c, self.cu(l));
        }
        if b.y > 0
            && let Some(a) = c.above.or_else(|| self.get_cu(b.x, b.y - 1, ch))
        {
            top = self.available(c, self.cu(a));
        }
        (left, top)
    }

    fn virtual_boundaries(&self, x: i32, y: i32, w: i32, h: i32) -> (Vec<i32>, Vec<i32>) {
        let mut hor = Vec::new();
        let mut ver = Vec::new();
        if !self.ph.vb_present {
            return (ver, hor);
        }
        for &p in &self.ph.vb_pos_y {
            let p = p as i32;
            if y <= p && p <= y + h {
                hor.push(p);
            }
        }
        for &p in &self.ph.vb_pos_x {
            let p = p as i32;
            if x <= p && p <= x + w {
                ver.push(p);
            }
        }
        (ver, hor)
    }

    fn calc_strengths(&mut self, id: u32) {
        let c = self.cu(id).clone();
        let ch = c.ch_type;
        let area = c.blk[Self::comp(ch)];
        let (csx, csy) = self.scale(ch);
        let (vb_ver, vb_hor) =
            self.virtual_boundaries(area.x << csx, area.y << csy, area.w << csx, area.h << csy);
        let crossed = !vb_ver.is_empty() || !vb_hor.is_empty();
        let (left_edge, top_edge) = self.cu_edges(id);
        let refine = c.isp != 0;
        let mask_x = !((1i32 << (2 - csx)) - 1);
        let mask_y = !((1i32 << (2 - csy)) - 1);
        for t in c.first_tu..c.first_tu + c.num_tu {
            let at = self.tu(t).blk[Self::comp(ch)];
            let mut ver = if (at.x & mask_x) == area.x {
                left_edge
            } else {
                true
            };
            let mut hor = if (at.y & mask_y) == area.y {
                top_edge
            } else {
                true
            };
            if crossed {
                let (px, py) = ((at.x & mask_x) << csx, (at.y & mask_y) << csy);
                if vb_ver.contains(&px) {
                    ver = false;
                }
                if vb_hor.contains(&py) {
                    hor = false;
                }
            }
            self.set_max_filter_length(0, id, t, ver, !refine);
            self.set_max_filter_length(1, id, t, hor, !refine);
        }
        if !refine {
            return;
        }
        let step_x = 4 >> csx;
        let step_y = 4 >> csy;
        // vertical edges
        let mut cu_p = c.left;
        let mut y = 0;
        while y < area.h {
            cu_p = match cu_p {
                Some(p)
                    if {
                        let b = self.cu(p).blk[Self::comp(ch)];
                        b.y + b.h > area.y + y
                    } =>
                {
                    Some(p)
                }
                _ => self.get_cu(area.x - 1, area.y + y, ch),
            };
            let mut x = 0;
            while x < area.w {
                let i = self.idx(ch, area.x + x, area.y + y);
                if self.maps[0][i].edge[ch] {
                    let p = if x != 0 { Some(id) } else { cu_p };
                    if let Some(p) = p {
                        let mut l = self.maps[0][i];
                        self.bs_single(0, &mut l, id, area.x + x, area.y + y, p);
                        self.maps[0][i] = l;
                    }
                }
                self.maps[0][i].bs &= !MARK;
                x += step_x;
            }
            y += step_y;
        }
        // horizontal edges
        let mut cu_p = c.above;
        let mut y = 0;
        while y < area.h {
            let mut x = 0;
            while x < area.w {
                if y == 0 {
                    cu_p = match cu_p {
                        Some(p)
                            if {
                                let b = self.cu(p).blk[Self::comp(ch)];
                                b.x + b.w > area.x + x
                            } =>
                        {
                            Some(p)
                        }
                        _ => self.get_cu(area.x + x, area.y - 1, ch),
                    };
                }
                let i = self.idx(ch, area.x + x, area.y + y);
                if self.maps[1][i].edge[ch] {
                    let p = if y != 0 { Some(id) } else { cu_p };
                    if let Some(p) = p {
                        let mut l = self.maps[1][i];
                        self.bs_single(1, &mut l, id, area.x + x, area.y + y, p);
                        self.maps[1][i] = l;
                    }
                }
                self.maps[1][i].bs &= !MARK;
                x += step_x;
            }
            y += step_y;
        }
    }

    fn max_flpq(size_p: i32, size_q: i32) -> u8 {
        let v = if size_p <= 4 || size_q <= 4 {
            17
        } else {
            ((if size_p >= 32 { 7 } else { 3 }) << 4) + if size_q >= 32 { 7 } else { 3 }
        };
        v + 128
    }

    /// vvdec's `xSetMaxFilterLengthPQFromTransformSizes`.
    fn set_max_filter_length(&mut self, dir: Dir, id: u32, t: u32, bvalue: bool, derive: bool) {
        let c = self.cu(id).clone();
        let tu = self.tu(t).clone();
        let dt = c.is_sep_tree(self.dual(&c));
        let mut start = 0usize;
        let mut end = 1usize;
        if dt {
            if c.ch_type == 0 {
                end = 0;
            } else {
                start = 1;
            }
        }
        if start != end && (self.pic.fmt.chroma == 0 || !tu.blk[1].valid()) {
            end = 0;
        }
        let (csx, csy) = if start != end { self.scale(1) } else { (0, 0) };
        let area = tu.blk[Self::comp(end)];
        let stride = if dir == 0 { self.pic.map_w as isize } else { 1 };
        for ct in start..=end {
            let tb = tu.blk[Self::comp(ct)];
            if !tb.valid() || perp(dir, tb.x, tb.y) == 0 {
                continue;
            }
            let mut li = self.idx(ct, tb.x, tb.y) as isize;
            let (sx, sy) = self.scale(ct);
            let inc = if dir == 1 { 4 >> sx } else { 4 >> sy };
            let neigh = if dir == 1 { c.above } else { c.left };
            let cb = c.blk[Self::comp(ct)];
            let sb = c.blk[Self::comp(start)];
            let tsb = tu.blk[Self::comp(start)];
            let mut cu_p = match neigh {
                Some(n) if perp(dir, tb.x, tb.y) == perp(dir, cb.x, cb.y) => n,
                _ => id,
            };
            let mut cu_pf = match neigh {
                Some(n) if perp(dir, tsb.x, tsb.y) == perp(dir, sb.x, sb.y) => n,
                _ => id,
            };
            let (fsx, fsy) = self.scale(start);
            let inc_f = if dir == 1 { 4 >> fsx } else { 4 >> fsy };
            let off = |x: i32, y: i32| if dir == 0 { (x - 1, y) } else { (x, y - 1) };
            if cu_p == id && neigh.is_none() && perp(dir, cb.x, cb.y) > 0 {
                let (px, py) = off(tb.x, tb.y);
                let (fx, fy) = off(tsb.x, tsb.y);
                if !cb.contains(px, py)
                    && let Some(p) = self.get_cu(px, py, ct)
                {
                    cu_p = p;
                }
                if !self.cu(cu_pf).blk[Self::comp(start)].contains(fx, fy)
                    && let Some(p) = self.get_cu(fx, fy, start)
                {
                    cu_pf = p;
                }
            }
            let same_cu_tu = perp(dir, tb.x, tb.y) == perp(dir, cb.x, cb.y);
            let psize = parl(dir, tb.w, tb.h);
            let size_q = perp(dir, tb.w, tb.h);
            let refresh = |s: &Self, p: u32, x: i32, y: i32, ch: usize| -> u32 {
                let b = s.cu(p).blk[Self::comp(ch)];
                if parl(dir, b.x, b.y) + parl(dir, b.w, b.h) > parl(dir, x, y) {
                    p
                } else {
                    s.get_cu(x, y, ch).unwrap_or(p)
                }
            };
            if ct == end && derive {
                if start != end {
                    let mut d = 0;
                    let mut d_f = 0;
                    while d < psize {
                        let (qx, qy) = if dir == 0 {
                            (tb.x, tb.y + d)
                        } else {
                            (tb.x + d, tb.y)
                        };
                        let (px, py) = off(qx, qy);
                        cu_p = refresh(self, cu_p, px, py, ct);
                        let (fx, fy) = if dir == 1 {
                            (tsb.x + d_f, tsb.y - 1)
                        } else {
                            (tsb.x - 1, tsb.y + d_f)
                        };
                        cu_pf = refresh(self, cu_pf, fx, fy, start);
                        let tp = self.get_tu(cu_p, px, py, ct);
                        let size_p = {
                            let b = self.tu(tp).blk[Self::comp(ct)];
                            perp(dir, b.w, b.h)
                        };
                        let i = li as usize;
                        let mut l = self.maps[dir][i];
                        l.cmfl = size_q >= 8 && size_p >= 8;
                        if bvalue {
                            let (lx, ly) = if dir == 0 {
                                (area.x << csx, (area.y + d) << csy)
                            } else {
                                ((area.x + d) << csx, area.y << csy)
                            };
                            self.bs_single(dir, &mut l, id, lx, ly, cu_pf);
                        }
                        l.bs &= !MARK;
                        self.maps[dir][i] = l;
                        li += stride;
                        d += inc;
                        d_f += inc_f;
                    }
                } else {
                    let mut d = 0;
                    while d < psize {
                        let (qx, qy) = if dir == 0 {
                            (tb.x, tb.y + d)
                        } else {
                            (tb.x + d, tb.y)
                        };
                        let (px, py) = off(qx, qy);
                        cu_p = refresh(self, cu_p, px, py, ct);
                        let tp = self.get_tu(cu_p, px, py, ct);
                        let size_p = {
                            let b = self.tu(tp).blk[Self::comp(ct)];
                            perp(dir, b.w, b.h)
                        };
                        let i = li as usize;
                        let mut l = self.maps[dir][i];
                        l.edge[c.ch_type] = bvalue;
                        if (l.bs != 0 || same_cu_tu) && bvalue {
                            l.bs |= bs_set(3, 3);
                        } else {
                            l.bs |= bs_set(1, 3);
                        }
                        if ct == 0 {
                            l.side_max = Self::max_flpq(size_p, size_q);
                        } else {
                            l.cmfl = size_q >= 8 && size_p >= 8;
                        }
                        if bvalue {
                            let (lx, ly) = if dir == 0 {
                                (area.x, area.y + d)
                            } else {
                                (area.x + d, area.y)
                            };
                            self.bs_single(dir, &mut l, id, lx, ly, cu_p);
                        }
                        l.bs &= !MARK;
                        self.maps[dir][i] = l;
                        li += stride;
                        d += inc;
                    }
                }
            } else {
                let mut d = 0;
                while d < psize {
                    let (qx, qy) = if dir == 0 {
                        (tb.x, tb.y + d)
                    } else {
                        (tb.x + d, tb.y)
                    };
                    let (px, py) = off(qx, qy);
                    cu_p = refresh(self, cu_p, px, py, ct);
                    let tp = self.get_tu(cu_p, px, py, ct);
                    let tpb = self.tu(tp).blk[Self::comp(ct)];
                    let size_p = perp(dir, tpb.w, tpb.h);
                    let distance = (parl(dir, tpb.x, tpb.y) + parl(dir, tpb.w, tpb.h)
                        - parl(dir, qx, qy))
                    .min(psize - d);
                    if ct == 0 {
                        let i = li as usize;
                        let mut l = self.maps[dir][i];
                        l.edge[c.ch_type] = bvalue;
                        if bvalue {
                            if l.bs != 0 || same_cu_tu {
                                l.bs |= bs_set(3, 3);
                            } else {
                                l.bs |= bs_set(1, 3);
                            }
                            l.side_max = Self::max_flpq(size_p, size_q);
                        }
                        self.maps[dir][i] = l;
                        if distance > inc {
                            let mut j = inc;
                            while j < distance {
                                li += stride;
                                self.maps[dir][li as usize] = l;
                                j += inc;
                                d += inc;
                            }
                        }
                        d += inc;
                        li += stride;
                    } else {
                        let mut j = 0;
                        while j < distance {
                            let i = li as usize;
                            let mut l = self.maps[dir][i];
                            if ct == start {
                                l.edge[c.ch_type] = bvalue;
                                if bvalue {
                                    if l.bs != 0 || same_cu_tu {
                                        l.bs |= bs_set(3, 3);
                                    } else {
                                        l.bs |= bs_set(1, 3);
                                    }
                                }
                            }
                            l.cmfl = size_q >= 8 && size_p >= 8;
                            self.maps[dir][i] = l;
                            li += stride;
                            j += inc;
                            d += inc;
                        }
                    }
                }
            }
        }
    }

    /// vvdec's `xGetBoundaryStrengthSingle`; `(x, y)` is in the channel
    /// coordinates of the Q coding unit.
    fn bs_single(&self, dir: Dir, lfp: &mut Lfp, q: u32, x: i32, y: i32, p: u32) {
        let cq = self.cu(q);
        let cp = self.cu(p);
        let ch = cq.ch_type;
        let (px, py) = if dir == 0 { (x - 1, y) } else { (x, y - 1) };
        let tq = if cq.num_tu == 1 {
            cq.first_tu
        } else {
            self.get_tu(q, x, y, ch)
        };
        let tp = if cp.num_tu == 1 {
            cp.first_tu
        } else {
            self.get_tu(p, px, py, ch)
        };
        let tuq = self.tu(tq);
        let tup = self.tu(tp);
        let has_luma = cq.blk[0].valid();
        let has_chroma = self.pic.fmt.chroma != 0 && cq.blk[1].valid();
        let mut pc_intra = false;
        let mut chrm_bs = 2u8;
        if has_luma {
            lfp.qp[0] = (cq.qp + cp.qp + 1) >> 1;
        }
        if has_chroma {
            let bd2 = self.sps.qp_bd_offset << 1;
            let diff_ch = ch == 0 && cp.tree != Tree::D;
            let tqc = if cq.isp != 0 {
                self.tu(self.last_tu(q))
            } else {
                tuq
            };
            let (cpc, tpc) = if diff_ch {
                let (sx, sy) = self.scale(1);
                let (cx, cy) = (px >> sx, py >> sy);
                let pc = self.get_cu(cx, cy, 1).unwrap_or(p);
                (self.cu(pc), self.tu(self.get_tu(pc, cx, cy, 1)))
            } else {
                (
                    cp,
                    if cp.isp != 0 {
                        self.tu(self.last_tu(p))
                    } else {
                        tup
                    },
                )
            };
            pc_intra = cpc.pred == Pred::Intra;
            lfp.qp[1] = (tpc.cqp[0] + tqc.cqp[0] - bd2 + 1) >> 1;
            lfp.qp[2] = (tpc.cqp[1] + tqc.cqp[1] - bd2 + 1) >> 1;
            if pc_intra {
                chrm_bs = if cpc.bdpcm[1] != 0 && cq.pred == Pred::Intra && cq.bdpcm[1] != 0 {
                    0
                } else {
                    2
                };
            }
        }
        let mask = (if has_luma { bs_set(3, 0) } else { 0 })
            | bs_set(3, 3)
            | if has_chroma {
                bs_set(3, 1) | bs_set(3, 2)
            } else {
                0
            };
        if cp.pred == Pred::Intra || cq.pred == Pred::Intra {
            let edge_idx = (perp(dir, x, y) - perp(dir, cq.blk[ch].x, cq.blk[ch].y)) / 4;
            let bs_y = if cp.bdpcm[0] != 0 && cq.bdpcm[0] != 0 {
                0
            } else {
                2
            };
            if cq.isp != 0 && edge_idx != 0 {
                lfp.bs |= bs_set(bs_y, 0) & mask;
            } else {
                lfp.bs |= (bs_set(bs_y, 0) + bs_set(chrm_bs, 1) + bs_set(chrm_bs, 2)) & mask;
            }
            return;
        } else if pc_intra {
            lfp.bs |= bs_set(chrm_bs, 1) + bs_set(chrm_bs, 2);
        }
        let mut tmp = 0u8;
        if lfp.bs & mask != 0 {
            let cbf = tuq.cbf | tup.cbf;
            tmp += bs_set(cbf & 1, 0);
            if !(cp.pred != Pred::Intra && cq.pred != Pred::Intra && pc_intra) {
                let joint = u8::from(tuq.joint != 0 || tup.joint != 0);
                tmp += bs_set(((cbf >> 1) & 1) | joint, 1);
                tmp += bs_set(((cbf >> 2) & 1) | joint, 2);
            }
        }
        if bs_get(tmp, 0) == 1 {
            lfp.bs |= tmp & mask;
            return;
        }
        if !has_luma {
            lfp.bs |= tmp & mask;
            return;
        }
        let mark = bs_get(lfp.bs, 3);
        if mark != 0 && mark != 3 {
            lfp.bs |= tmp & mask;
            return;
        }
        if has_chroma {
            lfp.bs |= tmp & mask;
        }
        if cp.pred != cq.pred {
            lfp.bs |= 1 & mask;
            return;
        }
        // Both sides use intra block copy: compare block vectors (1/16 units).
        let (bq, bp) = (cq.bv, cp.bv);
        let moved = (bq.0 - bp.0).abs() >= 8 || (bq.1 - bp.1).abs() >= 8;
        lfp.bs |= (if moved { tmp + 1 } else { tmp }) & mask;
    }
}

fn clip3(lo: i32, hi: i32, v: i32) -> i32 {
    v.clamp(lo, hi)
}

struct Samples<'a> {
    data: &'a mut [i16],
}

impl Samples<'_> {
    #[inline]
    fn g(&self, i: isize) -> i32 {
        i32::from(self.data[i as usize])
    }
    #[inline]
    fn s(&mut self, i: isize, v: i32) {
        self.data[i as usize] = v as i16;
    }
}

const DB7: [i32; 7] = [59, 50, 41, 32, 23, 14, 5];
const DB5: [i32; 5] = [58, 45, 32, 19, 6];
const DB3: [i32; 3] = [53, 32, 11];

fn filtering_pq(
    s: &mut Samples,
    src: isize,
    step: isize,
    offset: isize,
    np: usize,
    nq: usize,
    tc: i32,
) {
    let cp: &[i32] = match np {
        7 => &DB7,
        5 => &DB5,
        _ => &DB3,
    };
    let cq: &[i32] = match nq {
        7 => &DB7,
        5 => &DB5,
        _ => &DB3,
    };
    const TC7: [i32; 7] = [6, 5, 4, 3, 2, 1, 1];
    const TC3: [i32; 3] = [6, 4, 2];
    let tcp: &[i32] = if np == 3 { &TC3 } else { &TC7 };
    let tcq: &[i32] = if nq == 3 { &TC3 } else { &TC7 };
    for i in 0..4isize {
        let p = src + step * i - offset;
        let q = src + step * i;
        let (npi, nqi) = (np as isize, nq as isize);
        let ref_p = (s.g(p - (npi - 1) * offset) + s.g(p - npi * offset) + 1) >> 1;
        let ref_q = (s.g(q + (nqi - 1) * offset) + s.g(q + nqi * offset) + 1) >> 1;
        let sp = |k: isize| s.g(p - k * offset);
        let sq = |k: isize| s.g(q + k * offset);
        let ref_m = if np == nq {
            if np == 5 {
                (2 * (sp(0) + sq(0) + sp(1) + sq(1) + sp(2) + sq(2))
                    + sp(3)
                    + sq(3)
                    + sp(4)
                    + sq(4)
                    + 8)
                    >> 4
            } else {
                (2 * (sp(0) + sq(0))
                    + sp(1)
                    + sq(1)
                    + sp(2)
                    + sq(2)
                    + sp(3)
                    + sq(3)
                    + sp(4)
                    + sq(4)
                    + sp(5)
                    + sq(5)
                    + sp(6)
                    + sq(6)
                    + 8)
                    >> 4
            }
        } else {
            // the longer side takes the role of P
            let (pt, qt, op, oq, n_p, n_q) = if nq > np {
                (q, p, offset, -offset, nq, np)
            } else {
                (p, q, -offset, offset, np, nq)
            };
            let a = |k: isize| s.g(pt + k * op);
            let b = |k: isize| s.g(qt + k * oq);
            if n_p == 7 && n_q == 5 {
                (2 * (sp(0) + sq(0) + sp(1) + sq(1))
                    + sp(2)
                    + sq(2)
                    + sp(3)
                    + sq(3)
                    + sp(4)
                    + sq(4)
                    + sp(5)
                    + sq(5)
                    + 8)
                    >> 4
            } else if n_p == 7 && n_q == 3 {
                (2 * (a(0) + b(0))
                    + b(0)
                    + 2 * (b(1) + b(2))
                    + a(1)
                    + b(1)
                    + a(2)
                    + a(3)
                    + a(4)
                    + a(5)
                    + a(6)
                    + 8)
                    >> 4
            } else {
                (sp(0) + sq(0) + sp(1) + sq(1) + sp(2) + sq(2) + sp(3) + sq(3) + 4) >> 3
            }
        };
        for k in 0..np {
            let pos = p - offset * k as isize;
            let v = s.g(pos);
            let cv = (tc * tcp[k]) >> 1;
            s.s(
                pos,
                clip3(
                    v - cv,
                    v + cv,
                    (ref_m * cp[k] + ref_p * (64 - cp[k]) + 32) >> 6,
                ),
            );
        }
        for k in 0..nq {
            let pos = q + offset * k as isize;
            let v = s.g(pos);
            let cv = (tc * tcq[k]) >> 1;
            s.s(
                pos,
                clip3(
                    v - cv,
                    v + cv,
                    (ref_m * cq[k] + ref_q * (64 - cq[k]) + 32) >> 6,
                ),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn pel_filter_luma(
    s: &mut Samples,
    src: isize,
    step: isize,
    off: isize,
    tc: i32,
    sw: bool,
    thr_cut: i32,
    fp: bool,
    fq: bool,
    max: i32,
) {
    for i in 0..4isize {
        let c = src + step * i;
        let m1 = s.g(c - 3 * off);
        let m2 = s.g(c - 2 * off);
        let m3 = s.g(c - off);
        let m4 = s.g(c);
        let m5 = s.g(c + off);
        let m6 = s.g(c + 2 * off);
        if sw {
            let m0 = s.g(c - 4 * off);
            let m7 = s.g(c + 3 * off);
            s.s(
                c - 3 * off,
                clip3(m1 - tc, m1 + tc, (2 * m0 + 3 * m1 + m2 + m3 + m4 + 4) >> 3),
            );
            s.s(
                c - 2 * off,
                clip3(m2 - 2 * tc, m2 + 2 * tc, (m1 + m2 + m3 + m4 + 2) >> 2),
            );
            s.s(
                c - off,
                clip3(
                    m3 - 3 * tc,
                    m3 + 3 * tc,
                    (m1 + 2 * m2 + 2 * m3 + 2 * m4 + m5 + 4) >> 3,
                ),
            );
            s.s(
                c,
                clip3(
                    m4 - 3 * tc,
                    m4 + 3 * tc,
                    (m2 + 2 * m3 + 2 * m4 + 2 * m5 + m6 + 4) >> 3,
                ),
            );
            s.s(
                c + off,
                clip3(m5 - 2 * tc, m5 + 2 * tc, (m3 + m4 + m5 + m6 + 2) >> 2),
            );
            s.s(
                c + 2 * off,
                clip3(m6 - tc, m6 + tc, (m3 + m4 + m5 + 3 * m6 + 2 * m7 + 4) >> 3),
            );
        } else {
            let mut delta = (9 * (m4 - m3) - 3 * (m5 - m2) + 8) >> 4;
            if delta.abs() < thr_cut {
                delta = clip3(-tc, tc, delta);
                let tc2 = tc >> 1;
                s.s(c - off, (m3 + delta).clamp(0, max));
                if fp {
                    let d1 = clip3(-tc2, tc2, (((m1 + m3 + 1) >> 1) - m2 + delta) >> 1);
                    s.s(c - 2 * off, (m2 + d1).clamp(0, max));
                }
                s.s(c, (m4 - delta).clamp(0, max));
                if fq {
                    let d2 = clip3(-tc2, tc2, (((m6 + m4 + 1) >> 1) - m5 - delta) >> 1);
                    s.s(c + off, (m5 + d2).clamp(0, max));
                }
            }
        }
    }
}

fn pel_filter_chroma(
    s: &mut Samples,
    c: isize,
    off: isize,
    tc: i32,
    sw: bool,
    max: i32,
    ctb_hor: bool,
) {
    let m2 = s.g(c - 2 * off);
    let m3 = s.g(c - off);
    let m4 = s.g(c);
    let m5 = s.g(c + off);
    if sw {
        let m6 = s.g(c + 2 * off);
        let m7 = s.g(c + 3 * off);
        if ctb_hor {
            s.s(
                c - off,
                clip3(m3 - tc, m3 + tc, (3 * m2 + 2 * m3 + m4 + m5 + m6 + 4) >> 3),
            );
            s.s(
                c,
                clip3(
                    m4 - tc,
                    m4 + tc,
                    (2 * m2 + m3 + 2 * m4 + m5 + m6 + m7 + 4) >> 3,
                ),
            );
            s.s(
                c + off,
                clip3(
                    m5 - tc,
                    m5 + tc,
                    (m2 + m3 + m4 + 2 * m5 + m6 + 2 * m7 + 4) >> 3,
                ),
            );
            s.s(
                c + 2 * off,
                clip3(m6 - tc, m6 + tc, (m3 + m4 + m5 + 2 * m6 + 3 * m7 + 4) >> 3),
            );
        } else {
            let m0 = s.g(c - 4 * off);
            let m1 = s.g(c - 3 * off);
            s.s(
                c - 3 * off,
                clip3(m1 - tc, m1 + tc, (3 * m0 + 2 * m1 + m2 + m3 + m4 + 4) >> 3),
            );
            s.s(
                c - 2 * off,
                clip3(
                    m2 - tc,
                    m2 + tc,
                    (2 * m0 + m1 + 2 * m2 + m3 + m4 + m5 + 4) >> 3,
                ),
            );
            s.s(
                c - off,
                clip3(
                    m3 - tc,
                    m3 + tc,
                    (m0 + m1 + m2 + 2 * m3 + m4 + m5 + m6 + 4) >> 3,
                ),
            );
            s.s(
                c,
                clip3(
                    m4 - tc,
                    m4 + tc,
                    (m1 + m2 + m3 + 2 * m4 + m5 + m6 + m7 + 4) >> 3,
                ),
            );
            s.s(
                c + off,
                clip3(
                    m5 - tc,
                    m5 + tc,
                    (m2 + m3 + m4 + 2 * m5 + m6 + 2 * m7 + 4) >> 3,
                ),
            );
            s.s(
                c + 2 * off,
                clip3(m6 - tc, m6 + tc, (m3 + m4 + m5 + 2 * m6 + 3 * m7 + 4) >> 3),
            );
        }
    } else {
        let delta = clip3(-tc, tc, (((m4 - m3) * 4) + m2 - m5 + 4) >> 3);
        s.s(c - off, (m3 + delta).clamp(0, max));
        s.s(c, (m4 - delta).clamp(0, max));
    }
}

fn calc_dp(s: &Samples, c: isize, off: isize, ctb_hor: bool) -> i32 {
    if ctb_hor {
        (s.g(c - 2 * off) - 2 * s.g(c - 2 * off) + s.g(c - off)).abs()
    } else {
        (s.g(c - 3 * off) - 2 * s.g(c - 2 * off) + s.g(c - off)).abs()
    }
}

fn calc_dq(s: &Samples, c: isize, off: isize) -> i32 {
    (s.g(c) - 2 * s.g(c + off) + s.g(c + 2 * off)).abs()
}

#[allow(clippy::too_many_arguments)]
fn strong(
    s: &Samples,
    c: isize,
    off: isize,
    d: i32,
    beta: i32,
    tc: i32,
    p_large: bool,
    q_large: bool,
    max_p: isize,
    max_q: isize,
    ctb_hor: bool,
) -> bool {
    let m3 = s.g(c - off);
    let m4 = s.g(c);
    if !(d < (beta >> 2) && (m3 - m4).abs() < ((tc * 5 + 1) >> 1)) {
        return false;
    }
    let m0 = s.g(c - 4 * off);
    let m7 = s.g(c + 3 * off);
    let m2 = s.g(c - 2 * off);
    let mut sp3 = (m0 - m3).abs();
    if ctb_hor {
        sp3 = (m2 - m3).abs();
    }
    let mut sq3 = (m7 - m4).abs();
    let d_strong = sp3 + sq3;
    if p_large || q_large {
        if p_large {
            let mp4 = s.g(c - off * max_p - off);
            if max_p == 7 {
                let mp5 = s.g(c - 5 * off);
                let mp6 = s.g(c - 6 * off);
                let mp7 = s.g(c - 7 * off);
                sp3 += (mp5 - mp6 - mp7 + mp4).abs();
            }
            sp3 = (sp3 + (m0 - mp4).abs() + 1) >> 1;
        }
        if q_large {
            let m11 = s.g(c + off * max_q);
            if max_q == 7 {
                let m8 = s.g(c + 4 * off);
                let m9 = s.g(c + 5 * off);
                let m10 = s.g(c + 6 * off);
                sq3 += (m8 - m9 - m10 + m11).abs();
            }
            sq3 = (sq3 + (m11 - m7).abs() + 1) >> 1;
        }
        (sp3 + sq3) < ((beta * 3) >> 5) && d < (beta >> 4) && (m3 - m4).abs() < ((tc * 5 + 1) >> 1)
    } else {
        d_strong < (beta >> 3)
    }
}

fn tc_from_table(idx: i32, bd: u32) -> i32 {
    let t = i32::from(TC_TABLE[idx as usize]);
    if bd < 10 {
        (t + (1 << (9 - bd))) >> (10 - bd)
    } else {
        t << (bd - 10)
    }
}

impl Ctx<'_> {
    fn ctu_slice(&self, x: i32, y: i32) -> &SliceHeader {
        let addr = self.pic.ctu_addr_of(x, y, 0) as usize;
        let s = self.pic.ctus[addr].slice.unwrap_or(0) as usize;
        &self.slices[s]
    }

    fn edge_filter_luma(&self, dir: Dir, plane: &mut super::pic::Plane, x: i32, y: i32, lfp: &Lfp) {
        let slice = self.ctu_slice(x, y);
        let bd = self.pic.bit_depth;
        let max = (1 << bd) - 1;
        let stride = plane.stride as isize;
        let mut s = Samples {
            data: &mut plane.data,
        };
        let src = y as isize * stride + x as isize;
        let (offset, step) = if dir == 0 { (1, stride) } else { (stride, 1) };
        let bs = bs_get(lfp.bs, 0) as i32;
        if bs == 0 {
            return;
        }
        let mut qp = lfp.qp[0];
        if self.sps.ladf {
            let level = if dir == 0 {
                (s.g(src) + s.g(src + 3 * stride) + s.g(src - 1) + s.g(src + 3 * stride - 1)) >> 2
            } else {
                (s.g(src) + s.g(src + 3) + s.g(src - stride) + s.g(src - stride + 3)) >> 2
            };
            let mut shift = self.sps.ladf_qp_offset[0];
            for k in 1..self.sps.ladf_qp_offset.len() {
                if level > self.sps.ladf_lower_bound[k] {
                    shift = self.sps.ladf_qp_offset[k];
                } else {
                    break;
                }
            }
            qp += shift;
        }
        let max_p = ((lfp.side_max >> 4) & 7) as isize;
        let max_q = (lfp.side_max & 7) as isize;
        let mut p_large = max_p > 3;
        let q_large = max_q > 3;
        if dir == 1 && (y & ((1 << self.pic.ctu_log2) - 1)) == 0 {
            p_large = false;
        }
        let idx_tc = clip3(0, 65, qp + 2 * (bs - 1) + slice.tc_offset_div2[0] * 2);
        let idx_b = clip3(0, 63, qp + slice.beta_offset_div2[0] * 2);
        let tc = tc_from_table(idx_tc, bd);
        let beta = i32::from(BETA_TABLE[idx_b as usize]) << (bd - 8);
        let side_thr = (beta + (beta >> 1)) >> 3;
        let thr_cut = tc * 10;
        let s0 = src;
        let s3 = src + 3 * step;
        let dp0 = calc_dp(&s, s0, offset, false);
        let dq0 = calc_dq(&s, s0, offset);
        let dp3 = calc_dp(&s, s3, offset, false);
        let dq3 = calc_dq(&s, s3, offset);
        let d0 = dp0 + dq0;
        let d3 = dp3 + dq3;
        if p_large || q_large {
            let off3 = 3 * offset;
            let dp0l = if p_large {
                (dp0 + calc_dp(&s, s0 - off3, offset, false) + 1) >> 1
            } else {
                dp0
            };
            let dq0l = if q_large {
                (dq0 + calc_dq(&s, s0 + off3, offset) + 1) >> 1
            } else {
                dq0
            };
            let dp3l = if p_large {
                (dp3 + calc_dp(&s, s3 - off3, offset, false) + 1) >> 1
            } else {
                dp3
            };
            let dq3l = if q_large {
                (dq3 + calc_dq(&s, s3 + off3, offset) + 1) >> 1
            } else {
                dq3
            };
            let d0l = dp0l + dq0l;
            let d3l = dp3l + dq3l;
            if d0l + d3l < beta {
                let swl = strong(
                    &s,
                    s0,
                    offset,
                    2 * d0l,
                    beta,
                    tc,
                    p_large,
                    q_large,
                    max_p,
                    max_q,
                    false,
                ) && strong(
                    &s,
                    s3,
                    offset,
                    2 * d3l,
                    beta,
                    tc,
                    p_large,
                    q_large,
                    max_p,
                    max_q,
                    false,
                );
                if swl {
                    filtering_pq(
                        &mut s,
                        src,
                        step,
                        offset,
                        if p_large { max_p as usize } else { 3 },
                        if q_large { max_q as usize } else { 3 },
                        tc,
                    );
                    return;
                }
            }
        }
        let dp = dp0 + dp3;
        let dq = dq0 + dq3;
        let d = d0 + d3;
        if d < beta {
            let (mut fp, mut fq) = (false, false);
            if max_p > 1 && max_q > 1 {
                fp = dp < side_thr;
                fq = dq < side_thr;
            }
            let mut sw = false;
            if max_p > 2 && max_q > 2 {
                sw = strong(&s, s0, offset, 2 * d0, beta, tc, false, false, 7, 7, false)
                    && strong(&s, s3, offset, 2 * d3, beta, tc, false, false, 7, 7, false);
            }
            pel_filter_luma(&mut s, src, step, offset, tc, sw, thr_cut, fp, fq, max);
        }
    }

    /// `(x, y)` in chroma coordinates; `lfp` is the luma-grid entry.
    fn edge_filter_chroma(
        &self,
        dir: Dir,
        planes: &mut [super::pic::Plane],
        x: i32,
        y: i32,
        lfp: &Lfp,
    ) {
        let (csx, csy) = self.scale(1);
        let slice = self.ctu_slice(x << csx, y << csy);
        let bd = self.pic.bit_depth;
        let max = (1 << bd) - 1;
        let loop_len = if dir == 0 { 4 >> csy } else { 4 >> csx };
        let bs = [bs_get(lfp.bs, 1), bs_get(lfp.bs, 2)];
        if bs[0] == 0 && bs[1] == 0 {
            return;
        }
        let large = lfp.cmfl;
        let ctb_hor = dir == 1 && (y & (((1 << self.pic.ctu_log2) - 1) >> csy)) == 0;
        for ci in 0..2 {
            if !(bs[ci] == 2 || (large && bs[ci] == 1)) {
                continue;
            }
            let plane = &mut planes[ci + 1];
            let stride = plane.stride as isize;
            let mut s = Samples {
                data: &mut plane.data,
            };
            let src = y as isize * stride + x as isize;
            let (offset, step) = if dir == 0 { (1, stride) } else { (stride, 1) };
            let qp = lfp.qp[ci + 1];
            let idx_tc = clip3(
                0,
                65,
                qp + 2 * (i32::from(bs[ci]) - 1) + slice.tc_offset_div2[ci + 1] * 2,
            );
            let tc = tc_from_table(idx_tc, bd);
            if large {
                let idx_b = clip3(0, 63, qp + slice.beta_offset_div2[ci + 1] * 2);
                let beta = i32::from(BETA_TABLE[idx_b as usize]) * (1 << (bd - 8));
                let dp0 = calc_dp(&s, src, offset, ctb_hor);
                let dq0 = calc_dq(&s, src, offset);
                let sub = if dir == 0 { csy } else { csx };
                let o3 = if sub == 1 { step } else { step * 3 };
                let dp3 = calc_dp(&s, src + o3, offset, ctb_hor);
                let dq3 = calc_dq(&s, src + o3, offset);
                let d0 = dp0 + dq0;
                let d3 = dp3 + dq3;
                if d0 + d3 < beta {
                    let sw = strong(
                        &s,
                        src,
                        offset,
                        2 * d0,
                        beta,
                        tc,
                        false,
                        false,
                        7,
                        7,
                        ctb_hor,
                    ) && strong(
                        &s,
                        src + o3,
                        offset,
                        2 * d3,
                        beta,
                        tc,
                        false,
                        false,
                        7,
                        7,
                        ctb_hor,
                    );
                    for k in 0..loop_len as isize {
                        pel_filter_chroma(&mut s, src + step * k, offset, tc, sw, max, ctb_hor);
                    }
                    continue;
                }
            }
            for k in 0..loop_len as isize {
                pel_filter_chroma(&mut s, src + step * k, offset, tc, false, max, ctb_hor);
            }
        }
    }
}

/// Filters the whole picture in place.
pub fn deblock(pic: &mut Picture, sps: &Sps, pps: &Pps, ph: &PicHeader, slices: &[SliceHeader]) {
    if slices.iter().all(|s| s.deblocking_disabled) {
        return;
    }
    let n = pic.map_w * pic.map_h;
    let mut planes = std::mem::take(&mut pic.planes);
    {
        let mut ctx = Ctx {
            pic,
            sps,
            pps,
            ph,
            slices,
            maps: [vec![Lfp::default(); n], vec![Lfp::default(); n]],
        };
        for id in 0..ctx.pic.cus.len() as u32 {
            let c = ctx.cu(id);
            if ctx.slices[c.slice as usize].deblocking_disabled {
                continue;
            }
            ctx.calc_strengths(id);
        }
        let (w, h) = (ctx.pic.width, ctx.pic.height);
        let ctu = 1i32 << ctx.pic.ctu_log2;
        let chroma = ctx.pic.fmt.chroma != 0;
        let (csx, csy) = ctx.scale(1);
        for dir in 0..2 {
            for cy in (0..h).step_by(ctu as usize) {
                for cx in (0..w).step_by(ctu as usize) {
                    if ctx.ctu_slice(cx, cy).deblocking_disabled {
                        continue;
                    }
                    let (cw, chh) = (ctu.min(w - cx), ctu.min(h - cy));
                    let mut dy = 0;
                    while dy < chh {
                        let mut dx = 0;
                        while dx < cw {
                            let l = ctx.maps[dir][((cy + dy) >> 2) as usize * ctx.pic.map_w
                                + ((cx + dx) >> 2) as usize];
                            if bs_get(l.bs, 0) != 0 {
                                ctx.edge_filter_luma(dir, &mut planes[0], cx + dx, cy + dy, &l);
                            }
                            dx += 4;
                        }
                        dy += 4;
                    }
                    if !chroma {
                        continue;
                    }
                    let (ccx, ccy, ccw, cch) = (cx >> csx, cy >> csy, cw >> csx, chh >> csy);
                    let cincy = if dir == 0 {
                        4 >> csy
                    } else {
                        (8 << csy) / 4 * (4 >> csy)
                    };
                    let cincx = if dir == 1 {
                        4 >> csx
                    } else {
                        (8 << csx) / 4 * (4 >> csx)
                    };
                    let mut cdy = 0;
                    while cdy < cch {
                        let mut cdx = 0;
                        while cdx < ccw {
                            let (lx, ly) = ((ccx + cdx) << csx, (ccy + cdy) << csy);
                            let l = ctx.maps[dir]
                                [(ly >> 2) as usize * ctx.pic.map_w + (lx >> 2) as usize];
                            if bs_get(l.bs, 1) | bs_get(l.bs, 2) != 0 {
                                ctx.edge_filter_chroma(dir, &mut planes, ccx + cdx, ccy + cdy, &l);
                            }
                            cdx += cincx;
                        }
                        cdy += cincy;
                    }
                }
            }
        }
    }
    pic.planes = planes;
}
