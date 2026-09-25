// SPDX-License-Identifier: LGPL-3.0-or-later
//! Intra CU reconstruction: prediction (angular, planar, DC, BDPCM, MIP,
//! CCLM, ISP), dequantization, LFNST, inverse transforms, joint Cb-Cr and
//! LMCS chroma residual scaling, following vvdec's `DecCu`,
//! `IntraPrediction`, `TrQuant` and `Quant`.
use super::Error;
use super::ctu::{SliceInfo, grouped_scan_cached, mip_size_id};
use super::pic::*;
use super::tables::*;

pub struct Lmcs {
    pub min_bin: usize,
    pub max_bin: usize,
    pub pivot: [i32; 17],
    pub input_pivot: [i32; 17],
    pub fwd_scale: [i32; 16],
    pub inv_scale: [i32; 16],
    pub chroma_adj: [i32; 16],
    pub inv_lut: Vec<i16>,
}

impl Lmcs {
    pub fn new(p: &super::ps::LmcsParam, bit_depth: u32) -> Result<Self, Error> {
        let lut_size = 1i32 << bit_depth;
        let init_cw = lut_size / 16;
        let mut bin_cw = [0i32; 16];
        for i in p.min_bin as usize..=p.max_bin as usize {
            bin_cw[i] = (p.bin_cw_delta[i] + init_cw) as u16 as i32;
        }
        let mut sum = 0u32;
        for &cw in bin_cw
            .iter()
            .take(p.max_bin as usize + 1)
            .skip(p.min_bin as usize)
        {
            if cw < init_cw >> 3 || cw > (init_cw << 3) - 1 {
                return Err(Error::Invalid(
                    "The value of lmcsCW[ i ] shall be in the range of OrgCW >> 3 to ( OrgCW << 3 ) - 1, inclusive.",
                ));
            }
            let c = cw + p.chroma_offset;
            if c < init_cw >> 3 || c > (init_cw << 3) - 1 {
                return Err(Error::Invalid("lmcsCW[ i ] + lmcsDeltaCrs out of range"));
            }
            sum += cw as u32;
        }
        if sum > (1u32 << bit_depth) - 1 {
            return Err(Error::Invalid(
                "sum( lmcsCW ) exceeds ( 1 << BitDepth ) - 1",
            ));
        }
        let mut l = Self {
            min_bin: p.min_bin as usize,
            max_bin: p.max_bin as usize,
            pivot: [0; 17],
            input_pivot: [0; 17],
            fwd_scale: [1 << 11; 16],
            inv_scale: [1 << 11; 16],
            chroma_adj: [1 << 11; 16],
            inv_lut: vec![0; lut_size as usize + 1],
        };
        let bin_len_log2 = 31 - ((lut_size / 16) as u32).leading_zeros();
        for i in 0..16 {
            l.pivot[i + 1] = l.pivot[i] + bin_cw[i];
            l.input_pivot[i + 1] = l.input_pivot[i] + init_cw;
            l.fwd_scale[i] = (bin_cw[i] * (1 << 11) + (1 << (bin_len_log2 - 1))) >> bin_len_log2;
            if bin_cw[i] == 0 {
                l.inv_scale[i] = 0;
                l.chroma_adj[i] = 1 << 11;
            } else {
                l.inv_scale[i] = init_cw * (1 << 11) / bin_cw[i];
                l.chroma_adj[i] = init_cw * (1 << 11) / (bin_cw[i] + p.chroma_offset);
            }
        }
        for i in l.min_bin..=l.max_bin {
            if l.pivot[i] % (1 << (bit_depth - 5)) != 0
                && (l.pivot[i] >> (bit_depth - 5)) == (l.pivot[i + 1] >> (bit_depth - 5))
            {
                return Err(Error::Invalid("LmcsPivot constraint"));
            }
        }
        for s in 0..lut_size {
            let idx = l.pwl_idx_inv(s);
            let inv =
                l.input_pivot[idx] + ((l.inv_scale[idx] * (s - l.pivot[idx]) + (1 << 10)) >> 11);
            l.inv_lut[s as usize] = inv.clamp(0, lut_size - 1) as i16;
        }
        Ok(l)
    }

    pub fn pwl_idx_inv(&self, v: i32) -> usize {
        let mut idx = self.min_bin;
        while idx <= self.max_bin {
            if v < self.pivot[idx + 1] {
                break;
            }
            idx += 1;
        }
        idx.min(15)
    }
}

/// Dequantization coefficients per scaling list type and size.
pub struct ScalingMatrices {
    /// [list 0..6][log2 w 0..6][log2 h 0..6] -> w*h coefficients.
    pub coef: Vec<Vec<i32>>,
}

/// vvdec's `g_scalingListId[size][list]`.
const SCALING_LIST_ID: [[usize; 6]; 7] = [
    [0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 1],
    [2, 3, 4, 5, 6, 7],
    [8, 9, 10, 11, 12, 13],
    [14, 15, 16, 17, 18, 19],
    [20, 21, 22, 23, 24, 25],
    [26, 21, 22, 27, 24, 25],
];

/// vvdec's `processScalingListDec` (after its buffer is cleared).
fn process_scaling_list(
    coeff: &[i32],
    out: &mut [i32],
    height: usize,
    width: usize,
    ratio: usize,
    size_num: usize,
    dc: i32,
) {
    let l2 = |v: usize| v.trailing_zeros() as i32;
    let loop_h = height.min(32);
    let loop_w = width.min(32);
    if height != width {
        let (hl2, wl2, sl2) = (l2(height), l2(width), l2(size_num));
        let ratio_wh = if height > width { hl2 - wl2 } else { wl2 - hl2 };
        let ratio_h = if height / size_num != 0 {
            hl2 - sl2
        } else {
            sl2 - hl2
        };
        let ratio_w = if width / size_num != 0 {
            wl2 - sl2
        } else {
            sl2 - wl2
        };
        if height > width {
            let mut j = 0;
            while j < loop_h {
                for i in 0..loop_w {
                    out[j * width + i] =
                        coeff[size_num * (j >> ratio_h) + ((i << ratio_wh) >> ratio_h)];
                }
                for jj in 1..(1usize << ratio_h) {
                    for i in 0..loop_w {
                        out[(j + jj) * width + i] = out[j * width + i];
                    }
                }
                j += 1 << ratio_h;
            }
        } else {
            for j in 0..loop_h {
                let mut i = 0;
                while i < loop_w {
                    let c = coeff[size_num * ((j << ratio_wh) >> ratio_w) + (i >> ratio_w)];
                    for ii in 0..(1usize << ratio_w) {
                        out[j * width + i + ii] = c;
                    }
                    i += 1 << ratio_w;
                }
            }
        }
        if width.max(height) > 8 {
            out[0] = dc;
        }
        return;
    }
    let rl2 = l2(ratio);
    let mut j = 0;
    while j < loop_h {
        let mut i = 0;
        while i < loop_w {
            let c = coeff[size_num * (j >> rl2) + (i >> rl2)];
            for ii in 0..(1usize << rl2) {
                out[j * width + i + ii] = c;
            }
            i += 1 << rl2;
        }
        for jj in 1..(1usize << rl2) {
            for i in 0..loop_w {
                out[(j + jj) * width + i] = out[j * width + i];
            }
        }
        j += 1 << rl2;
    }
    if ratio > 1 {
        out[0] = dc;
    }
}

impl ScalingMatrices {
    /// vvdec's `Quant::setScalingListDec`.
    pub fn new(list: &crate::vvc::ps::ScalingList) -> Self {
        let mut coef: Vec<Vec<i32>> = (0..6 * 49)
            .map(|i| vec![0; 1 << ((i / 7) % 7 + i % 7)])
            .collect();
        let idx = |l: usize, w: usize, h: usize| (l * 7 + w) * 7 + h;
        for size in 1..7usize {
            for l in 0..6 {
                if size == 1 && l < 4 {
                    continue;
                }
                let id = SCALING_LIST_ID[size][l];
                let n = 1usize << size;
                let size_num = n.min(8);
                process_scaling_list(
                    &list.coef[id],
                    &mut coef[idx(l, size, size)],
                    n,
                    n,
                    n / size_num,
                    size_num,
                    list.dc[id],
                );
            }
        }
        for sw in 0..7usize {
            for sh in 0..7usize {
                if sw == sh || (sw == 0 && sh < 2) || (sh == 0 && sw < 2) {
                    continue;
                }
                for l in 0..6 {
                    let large = sw.max(sh);
                    let id = SCALING_LIST_ID[large][l];
                    process_scaling_list(
                        &list.coef[id],
                        &mut coef[idx(l, sw, sh)],
                        1 << sh,
                        1 << sw,
                        if large > 3 { 2 } else { 1 },
                        if large >= 3 { 8 } else { 4 },
                        list.dc[id],
                    );
                }
            }
        }
        Self { coef }
    }

    pub fn get(&self, list: usize, lw: usize, lh: usize) -> &[i32] {
        &self.coef[(list * 7 + lw) * 7 + lh]
    }
}

fn log2(v: i32) -> i32 {
    31 - (v as u32).leading_zeros() as i32
}

/// Reference sample buffer: row 0 holds the top references (index 0 is the
/// top-left sample), column 0 the left references.
#[derive(Clone)]
struct RefBuf {
    data: Vec<i32>,
    stride: usize,
    off: usize,
}

impl RefBuf {
    fn new(stride: usize, rows: usize) -> Self {
        Self {
            data: vec![0; stride * rows + 1],
            stride,
            off: 0,
        }
    }
    #[inline]
    fn at(&self, x: usize, y: usize) -> i32 {
        self.data[self.off + y * self.stride + x]
    }
    #[inline]
    fn set(&mut self, x: usize, y: usize, v: i32) {
        let s = self.stride;
        let o = self.off;
        self.data[o + y * s + x] = v;
    }
}

struct Ctx<'p, 's> {
    pic: &'p mut Picture,
    si: &'s SliceInfo<'s>,
    cu_id: u32,
    cu: Cu,
    wpp: bool,
    top_len: usize,
    left_len: usize,
    unfiltered: RefBuf,
    filtered: RefBuf,
    isp_base: [RefBuf; 2],
    luma_pred: Vec<i32>,
    lm_stride: usize,
}

pub fn is_dm_chroma_mip(pic: &Picture, si: &SliceInfo, cu_id: u32) -> bool {
    let c = &pic.cus[cu_id as usize];
    if c.is_sep_tree(si.dual_tree()) || pic.fmt.chroma != 3 {
        return false;
    }
    c.mip
}

fn co_located_luma_cu(pic: &Picture, si: &SliceInfo, cu_id: u32) -> u32 {
    let c = &pic.cus[cu_id as usize];
    if !c.is_sep_tree(si.dual_tree()) {
        return cu_id;
    }
    let ch = c.ch_type;
    let (sx, sy) = pic.fmt.scale(ch);
    let b = c.blk[ch];
    let (lx, ly) = (b.x << sx, b.y << sy);
    let (lw, lh) = (b.w << sx, b.h << sy);
    pic.get_cu(lx + (lw >> 1), ly + (lh >> 1), 0)
        .unwrap_or(cu_id)
}

pub fn co_located_intra_luma_mode(pic: &Picture, si: &SliceInfo, cu_id: u32) -> u8 {
    let l = co_located_luma_cu(pic, si, cu_id);
    let c = &pic.cus[l as usize];
    if c.mip { PLANAR } else { c.intra_dir[0] }
}

fn final_intra_mode(pic: &Picture, si: &SliceInfo, cu: &Cu, cu_id: u32, ch: usize) -> u8 {
    let mut mode = cu.intra_dir[ch];
    if mode == DM_CHROMA && ch != 0 {
        mode = co_located_intra_luma_mode(pic, si, cu_id);
    }
    if pic.fmt.chroma == 2 && ch != 0 && mode < 67 {
        mode = CHROMA_422_MODE[mode as usize];
    }
    mode
}

fn get_wide_angle(w: i32, h: i32, mode: i32) -> i32 {
    if mode > DC as i32 && mode <= VDIA as i32 {
        const SHIFT: [i32; 6] = [0, 6, 10, 12, 14, 15];
        let delta = (log2(w) - log2(h)).abs();
        if w > h && mode < 2 + SHIFT[delta as usize] {
            return mode + (VDIA as i32 - 1);
        } else if h > w && mode > VDIA as i32 - SHIFT[delta as usize] {
            return mode - (VDIA as i32 - 1);
        }
    }
    mode
}

fn use_filtered_ref(
    pic: &Picture,
    si: &SliceInfo,
    cu: &Cu,
    cu_id: u32,
    comp: usize,
    area: Area,
) -> bool {
    let ch = if comp == 0 { 0 } else { 1 };
    if cu.mrl != 0 || cu.bdpcm[0] != 0 {
        return false;
    }
    let dir = final_intra_mode(pic, si, cu, cu_id, ch) as i32;
    if dir == DC as i32 {
        return false;
    }
    if dir == PLANAR as i32 {
        return area.area() > 32;
    }
    let pred = get_wide_angle(area.w, area.h, dir);
    let diff = (pred - HOR as i32).abs().min((pred - VER as i32).abs());
    let log2_size = ((log2(area.w) + log2(area.h)) >> 1) as usize;
    let ang_mode = if pred >= DIA as i32 {
        pred - VER as i32
    } else {
        -(pred - HOR as i32)
    };
    let abs_ang = ANG_TABLE[ang_mode.unsigned_abs() as usize];
    diff > INTRA_FILTER[ch][log2_size] && (abs_ang & 0x1f) == 0
}

impl<'p, 's> Ctx<'p, 's> {
    fn plane(&self, comp: usize) -> &Plane {
        &self.pic.planes[comp]
    }

    /// vvdec's `isAboveAvailable`.
    fn above_available(&self, tu: u32, ch: usize, x: i32, y: i32, units: i32, unit_w: i32) -> i32 {
        let max_dx = units * unit_w;
        let (mut rx, ry) = (x, y - 1);
        let mut first = true;
        let cur_idx = self.pic.tus[tu as usize].idx;
        let mut dx = 0;
        while dx < max_dx {
            let guess = if first { self.cu.above } else { None };
            let Some(a) = self
                .pic
                .get_cu_restricted(rx, ry, self.cu_id, ch, guess, self.wpp)
            else {
                break;
            };
            first = false;
            let t = self.pic.get_tu(a, rx, ry, ch);
            if self.pic.cus[a as usize].ctu == self.cu.ctu
                && self.pic.tus[t as usize].idx >= cur_idx
            {
                break;
            }
            let b = self.pic.tus[t as usize].blk[ch];
            let diff = b.w - rx + b.x;
            dx += diff;
            rx += diff;
        }
        (dx / unit_w).min(units)
    }

    fn left_available(&self, tu: u32, ch: usize, x: i32, y: i32, units: i32, unit_h: i32) -> i32 {
        let max_dy = units * unit_h;
        let (rx, mut ry) = (x - 1, y);
        let mut first = true;
        let cur_idx = self.pic.tus[tu as usize].idx;
        let mut dy = 0;
        while dy < max_dy {
            let guess = if first { self.cu.left } else { None };
            let Some(l) = self
                .pic
                .get_cu_restricted(rx, ry, self.cu_id, ch, guess, self.wpp)
            else {
                break;
            };
            first = false;
            let t = self.pic.get_tu(l, rx, ry, ch);
            if self.pic.cus[l as usize].ctu == self.cu.ctu
                && self.pic.tus[t as usize].idx >= cur_idx
            {
                break;
            }
            let b = self.pic.tus[t as usize].blk[ch];
            let diff = b.h - ry + b.y;
            dy += diff;
            ry += diff;
        }
        (dy / unit_h).min(units)
    }

    /// vvdec's `xFillReferenceSamples` for `area` (component coordinates).
    fn fill_reference(&mut self, comp: usize, area: Area, tu: u32, mrl: usize) -> RefBuf {
        let ch = if comp == 0 { 0 } else { 1 };
        let (sx, sy) = self.pic.fmt.scale(ch);
        let pred_size = self.top_len as i32;
        let pred_hsize = self.left_len as i32;
        let stride = (pred_size + 1) as usize + mrl;
        let unit_w = 4 >> sx;
        let unit_h = 4 >> sy;
        let total_above = (pred_size + unit_w - 1) / unit_w;
        let total_left = (pred_hsize + unit_h - 1) / unit_h;
        let total = total_above + total_left + 1;
        let num_above = area.w / unit_w;
        let num_left = area.h / unit_h;
        let num_ar = total_above - num_above;
        let num_lb = total_left - num_left;
        let ctu_mask_w = ((1i32 << self.pic.ctu_log2) - 1) >> sx;
        let ctu_mask_h = ((1i32 << self.pic.ctu_log2) - 1) >> sy;
        let same_ctu = (area.x & ctu_mask_w) != 0 && (area.y & ctu_mask_h) != 0;
        let n0 = if same_ctu {
            1
        } else {
            let guess = if self.cu.left.is_some() {
                self.cu.left
            } else {
                self.cu.above
            };
            i32::from(
                self.pic
                    .get_cu_restricted(area.x - 1, area.y - 1, self.cu_id, ch, guess, self.wpp)
                    .is_some(),
            )
        };
        let cb = self.cu.blk[ch];
        let n1 = if self.cu.above.is_some() || area.y > cb.y {
            num_above + self.above_available(tu, ch, area.x + area.w, area.y, num_ar, unit_w)
        } else {
            0
        };
        let n2 = if self.cu.left.is_some() || area.x > cb.x {
            num_left + self.left_available(tu, ch, area.x, area.y + area.h, num_lb, unit_h)
        } else {
            0
        };
        let num = n0 + n1 + n2;
        let rows = (pred_hsize as usize) + mrl + 1;
        let mut r = RefBuf::new(stride, rows);
        let dc = 1i32 << (self.pic.bit_depth - 1);
        let plane = self.plane(comp);
        let src = |x: i32, y: i32| i32::from(plane.at(area.x + x, area.y + y));
        let mrl_i = mrl as i32;
        if num == 0 {
            for j in 0..=(pred_size as usize + mrl) {
                r.set(j, 0, dc);
            }
            for i in 1..=(pred_hsize as usize + mrl) {
                r.set(0, i, dc);
            }
        } else if num == total {
            for j in 0..=(pred_size + mrl_i) {
                r.set(j as usize, 0, src(j - 1 - mrl_i, -1 - mrl_i));
            }
            for i in 1..=(pred_hsize + mrl_i) {
                r.set(0, i as usize, src(-1 - mrl_i, i - 1 - mrl_i));
            }
        } else if n2 > 0 {
            let tmp = (n2 * unit_h).min(pred_hsize);
            for i in 0..tmp {
                r.set(0, (1 + mrl_i + i) as usize, src(-1 - mrl_i, i));
            }
            let pad = r.at(0, (1 + mrl_i + tmp - 1) as usize);
            for i in tmp..pred_hsize {
                r.set(0, (1 + mrl_i + i) as usize, pad);
            }
            if n0 != 0 {
                for j in 0..=mrl_i {
                    r.set(j as usize, 0, src(j - 1 - mrl_i, -1 - mrl_i));
                }
                for i in 1..=mrl_i {
                    r.set(0, i as usize, src(-1 - mrl_i, i - 1 - mrl_i));
                }
            } else {
                let p = src(-1 - mrl_i, 0);
                r.set(0, 0, p);
                for i in 1..=mrl_i {
                    r.set(i as usize, 0, p);
                    r.set(0, i as usize, p);
                }
            }
            if n1 != 0 {
                let tmp = (n1 * unit_w).min(pred_size);
                for i in 0..tmp {
                    r.set((1 + mrl_i + i) as usize, 0, src(i, -1 - mrl_i));
                }
                let pad = r.at((1 + mrl_i + tmp - 1) as usize, 0);
                for i in tmp..pred_size {
                    r.set((1 + mrl_i + i) as usize, 0, pad);
                }
            } else {
                let pad = r.at(mrl_i as usize, 0);
                for i in 0..pred_size {
                    r.set((1 + mrl_i + i) as usize, 0, pad);
                }
            }
        } else {
            let tmp = (n1 * unit_w).min(pred_size);
            for i in 0..tmp {
                r.set((1 + mrl_i + i) as usize, 0, src(i, -1 - mrl_i));
            }
            let pad = r.at((1 + mrl_i + tmp - 1) as usize, 0);
            for i in tmp..pred_size {
                r.set((1 + mrl_i + i) as usize, 0, pad);
            }
            let p = src(0, -1 - mrl_i);
            r.set(0, 0, p);
            for i in 1..=mrl_i {
                r.set(i as usize, 0, p);
                r.set(0, i as usize, p);
            }
            for i in 0..pred_hsize {
                r.set(0, (1 + mrl_i + i) as usize, p);
            }
        }
        r
    }

    fn filter_reference(&self, src: &RefBuf, comp: usize, mrl: usize) -> RefBuf {
        let mrl = if comp != 0 { 0 } else { mrl };
        let pred_size = self.top_len + mrl;
        let pred_hsize = self.left_len + mrl;
        let mut dst = src.clone();
        dst.set(0, pred_hsize, src.at(0, pred_hsize));
        for i in (1..pred_hsize).rev() {
            let v = (src.at(0, i + 1) + 2 * src.at(0, i) + src.at(0, i - 1) + 2) >> 2;
            dst.set(0, i, v);
        }
        let v = (src.at(0, 1) + 2 * src.at(0, 0) + src.at(1, 0) + 2) >> 2;
        dst.set(0, 0, v);
        for i in 1..pred_size {
            let v = (src.at(i + 1, 0) + 2 * src.at(i, 0) + src.at(i - 1, 0) + 2) >> 2;
            dst.set(i, 0, v);
        }
        dst.set(pred_size, 0, src.at(pred_size, 0));
        dst
    }

    fn init_pattern(&mut self, tu: u32, comp: usize, area: Area, filter: bool) {
        self.left_len = (area.h << 1) as usize;
        self.top_len = (area.w << 1) as usize;
        let mrl = if comp == 0 { self.cu.mrl as usize } else { 0 };
        self.unfiltered = self.fill_reference(comp, area, tu, mrl);
        if filter {
            self.filtered = self.filter_reference(&self.unfiltered, comp, mrl);
        }
    }

    /// vvdec's `initIntraPatternChTypeISP`; `area` is the (prediction) area.
    fn init_pattern_isp(&mut self, area: Area) {
        let cu = self.cu.clone();
        let cb = cu.blk[0];
        let left_avail = self
            .pic
            .get_cu_restricted(
                area.x - 1,
                area.y,
                self.cu_id,
                0,
                if area.x == cb.x {
                    cu.left
                } else {
                    Some(self.cu_id)
                },
                self.wpp,
            )
            .is_some();
        let above_avail = self
            .pic
            .get_cu_restricted(
                area.x,
                area.y - 1,
                self.cu_id,
                0,
                if area.y == cb.y {
                    cu.left
                } else {
                    Some(self.cu_id)
                },
                self.wpp,
            )
            .is_some();
        if cb.x == area.x && cb.y == area.y {
            if cu.isp == HOR_ISP {
                self.left_len = (cb.h << 1) as usize;
                self.top_len = (cb.w + area.w) as usize;
            } else {
                self.left_len = (cb.h + area.h) as usize;
                self.top_len = (cb.w << 1) as usize;
            }
            let first_tu = cu.first_tu;
            let r = self.fill_reference(0, cb, first_tu, 0);
            self.isp_base[0] = r.clone();
            self.isp_base[1] = r;
            self.unfiltered = self.isp_base[0].clone();
            self.top_len = (cb.w + area.w) as usize;
            self.left_len = (cb.h + area.h) as usize;
        } else {
            let (dx, dy) = ((area.x - cb.x) as usize, (area.y - cb.y) as usize);
            let stride = self.isp_base[0].stride;
            let off = dy * stride + dx;
            self.top_len = (cb.w + area.w) as usize;
            self.left_len = (cb.h + area.h) as usize;
            let hor = self.top_len;
            let ver = self.left_len;
            let plane = &self.pic.planes[0];
            let base = &mut self.isp_base[0];
            base.off = off;
            if cu.isp == HOR_ISP {
                for i in 0..area.w as usize {
                    let v = i32::from(plane.at(area.x + i as i32, area.y - 1));
                    base.set(1 + i, 0, v);
                }
                let sample = i32::from(plane.at(area.x + area.w - 1, area.y - 1));
                for i in 0..hor - area.w as usize {
                    base.set(1 + area.w as usize + i, 0, sample);
                }
                if !left_avail {
                    let sample = i32::from(plane.at(area.x, area.y - 1));
                    for i in 0..=ver {
                        base.set(0, i, sample);
                    }
                }
            } else {
                for i in 0..area.h as usize {
                    let v = i32::from(plane.at(area.x - 1, area.y + i as i32));
                    base.set(0, 1 + i, v);
                }
                let sample = i32::from(plane.at(area.x - 1, area.y + area.h - 1));
                for i in 0..ver - area.h as usize {
                    base.set(0, 1 + area.h as usize + i, sample);
                }
                if !above_avail {
                    let sample = i32::from(plane.at(area.x - 1, area.y));
                    for i in 0..=hor {
                        base.set(i, 0, sample);
                    }
                }
            }
            self.unfiltered = base.clone();
            base.off = 0;
        }
    }

    /// vvdec's `predIntraAng` into `dst` (w x h, row-major).
    fn pred_intra_ang(&self, comp: usize, dst: &mut [i32], w: i32, h: i32, use_filtered: bool) {
        let ch = if comp == 0 { 0 } else { 1 };
        let cu = &self.cu;
        let cu_size = (cu.blk[comp].w, cu.blk[comp].h);
        let bd = if comp == 0 && cu.bdpcm[0] != 0 {
            cu.bdpcm[0]
        } else if comp != 0 && cu.bdpcm[1] != 0 {
            cu.bdpcm[1]
        } else {
            0
        };
        let dir = final_intra_mode(self.pic, self.si, cu, self.cu_id, ch);
        let mrl = if comp == 0 { cu.mrl as i32 } else { 0 };
        let use_isp = cu.isp != NOT_ISP && comp == 0;
        let max = (1i32 << self.pic.bit_depth) - 1;
        let mut do_pdpc = w >= 4 && h >= 4 && mrl == 0;
        let src = if use_filtered {
            &self.filtered
        } else {
            &self.unfiltered
        };
        if bd != 0 {
            for y in 0..h {
                for x in 0..w {
                    dst[(y * w + x) as usize] = if bd == 1 {
                        src.at(0, (y + 1) as usize)
                    } else {
                        src.at((x + 1) as usize, 0)
                    };
                }
            }
            return;
        }
        match dir {
            PLANAR => pred_planar(src, dst, w, h),
            DC => {
                let v = pred_dc(src, w, h, mrl);
                dst[..(w * h) as usize].fill(v);
            }
            _ => self.pred_angular(
                src,
                dst,
                w,
                h,
                ch,
                dir as i32,
                max,
                mrl,
                &mut do_pdpc,
                use_isp,
                cu_size,
            ),
        }
        if do_pdpc && (dir == PLANAR || dir == DC) {
            let scale = (log2(w) - 2 + log2(h) - 2 + 2) >> 2;
            for y in 0..h {
                let wt = 32 >> 31.min((y << 1) >> scale);
                let left = src.at(0, (y + 1) as usize);
                for x in 0..w {
                    let wl = 32 >> 31.min((x << 1) >> scale);
                    let top = src.at((x + 1) as usize, 0);
                    let i = (y * w + x) as usize;
                    let val = dst[i];
                    // Pel arithmetic in vvdec (int16 storage)
                    dst[i] =
                        (val + ((wl * (left - val) + wt * (top - val) + 32) >> 6)) as i16 as i32;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn pred_angular(
        &self,
        src: &RefBuf,
        dst: &mut [i32],
        w0: i32,
        h0: i32,
        ch: usize,
        dir: i32,
        max: i32,
        mrl: i32,
        do_pdpc: &mut bool,
        use_isp: bool,
        cu_size: (i32, i32),
    ) {
        let (mut width, mut height) = (w0, h0);
        let pred_mode = if use_isp {
            get_wide_angle(cu_size.0, cu_size.1, dir)
        } else {
            get_wide_angle(width, height, dir)
        };
        let is_ver = pred_mode >= DIA as i32;
        let ang_mode = if is_ver {
            pred_mode - VER as i32
        } else {
            -(pred_mode - HOR as i32)
        };
        let abs_ang_mode = ang_mode.unsigned_abs() as usize;
        let sign = if ang_mode < 0 { -1 } else { 1 };
        let inv_angle = INV_ANG_TABLE[abs_ang_mode];
        let abs_ang = ANG_TABLE[abs_ang_mode];
        let angle = sign * abs_ang;
        const BASE: usize = 2 * 64 + 3 + 33 * 3 + 256;
        let mut ref_above = [0i32; 2 * BASE];
        let mut ref_left = [0i32; 2 * BASE];
        // Offsets so negative indices are representable.
        let (main_off, side_off);
        let (main_is_above,) = (is_ver,);
        let top_len = self.top_len as i32;
        let left_len = self.left_len as i32;
        if angle < 0 {
            for x in 0..=(width + 1 + mrl) {
                ref_above[(x + height) as usize] = src.at(x as usize, 0);
            }
            for y in 0..=(height + 1 + mrl) {
                ref_left[(y + width) as usize] = src.at(0, y as usize);
            }
            if is_ver {
                main_off = height as usize;
                side_off = width as usize;
            } else {
                main_off = width as usize;
                side_off = height as usize;
            }
            let size_side = if is_ver { height } else { width };
            for k in -size_side..=-1 {
                let si = ((-k * inv_angle + 256) >> 9).min(size_side);
                let v = if main_is_above {
                    ref_left[side_off + si as usize]
                } else {
                    ref_above[side_off + si as usize]
                };
                let idx = (main_off as i32 + k) as usize;
                if main_is_above {
                    ref_above[idx] = v;
                } else {
                    ref_left[idx] = v;
                }
            }
        } else {
            for x in 0..=(top_len + mrl) {
                ref_above[x as usize] = src.at(x as usize, 0);
            }
            for y in 0..=(left_len + mrl) {
                ref_left[y as usize] = src.at(0, y as usize);
            }
            main_off = 0;
            side_off = 0;
            let log2_ratio = log2(width) - log2(height);
            let s = 0.max(if is_ver { log2_ratio } else { -log2_ratio });
            let max_index = (mrl << s) + 2;
            let ref_len = if is_ver { top_len } else { left_len };
            let main = if is_ver {
                &mut ref_above
            } else {
                &mut ref_left
            };
            let val = main[(ref_len + mrl) as usize];
            for z in 1..=max_index {
                main[(ref_len + mrl + z) as usize] = val;
            }
        }
        let (ref_main, ref_side): (&[i32], &[i32]) = if is_ver {
            (
                &ref_above[main_off + mrl as usize..],
                &ref_left[side_off + mrl as usize..],
            )
        } else {
            (
                &ref_left[main_off + mrl as usize..],
                &ref_above[side_off + mrl as usize..],
            )
        };
        // For negative angles refMain is accessed at negative indices;
        // index through a helper with the original base.
        let main_base = main_off + mrl as usize;
        let main_all: &[i32] = if is_ver { &ref_above } else { &ref_left };
        let rm = |i: i32| main_all[(main_base as i32 + i) as usize];
        let _ = ref_main;
        if !is_ver {
            std::mem::swap(&mut width, &mut height);
        }
        let mut tmp = vec![0i32; (width * height) as usize];
        let clip = |v: i32| v.clamp(0, max);
        if angle == 0 {
            if *do_pdpc {
                let scale = (log2(width) - 2 + log2(height) - 2 + 2) >> 2;
                let lev = [3.min(width), 6.min(width), 12.min(width), 24.min(width)];
                let top_left = src.at(0, 0);
                for y in 0..height {
                    let left = ref_side[(y + 1) as usize];
                    for x in 0..lev[scale as usize] {
                        let wl = 32 >> 31.min((x << 1) >> scale);
                        tmp[(y * width + x) as usize] =
                            clip((wl * (left - top_left) + rm(x + 1) * 64 + 32) >> 6);
                    }
                    for x in lev[scale as usize]..width {
                        tmp[(y * width + x) as usize] = rm(x + 1);
                    }
                }
            } else {
                for y in 0..height {
                    for x in 0..width {
                        tmp[(y * width + x) as usize] = rm(x + 1);
                    }
                }
            }
        } else {
            if (abs_ang & 0x1f) != 0 {
                let mut delta_pos = angle * (1 + mrl);
                if ch == 0 {
                    let diff = (pred_mode - HOR as i32)
                        .abs()
                        .min((pred_mode - VER as i32).abs());
                    let log2_size = ((log2(width) + log2(height)) >> 1) as usize;
                    let filter_flag = diff > INTRA_FILTER[ch][log2_size];
                    let mut interp = false;
                    if filter_flag {
                        let is_ref_filter = (abs_ang & 0x1f) == 0;
                        interp = !is_ref_filter;
                    }
                    let cubic = if use_isp { true } else { !interp || mrl > 0 };
                    for y in 0..height {
                        let d_int = delta_pos >> 5;
                        let d_frac = (delta_pos & 31) as usize;
                        let f = if cubic {
                            &CUBIC_FILTER[d_frac]
                        } else {
                            &GAUSS_FILTER[d_frac]
                        };
                        let mut idx = d_int + 1;
                        for x in 0..width {
                            let v = (f[0] * rm(idx - 1)
                                + f[1] * rm(idx)
                                + f[2] * rm(idx + 1)
                                + f[3] * rm(idx + 2)
                                + 32)
                                >> 6;
                            tmp[(y * width + x) as usize] = if cubic { clip(v) } else { v };
                            idx += 1;
                        }
                        delta_pos += angle;
                    }
                } else {
                    for y in 0..height {
                        let d_int = delta_pos >> 5;
                        let d_frac = delta_pos & 31;
                        let mut last = rm(d_int + 1);
                        for x in 0..width {
                            let this = rm(d_int + 2 + x);
                            tmp[(y * width + x) as usize] =
                                ((32 - d_frac) * last + d_frac * this + 16) >> 5;
                            last = this;
                        }
                        delta_pos += angle;
                    }
                }
            } else {
                let mut delta_pos = angle * (1 + mrl);
                for y in 0..height {
                    let d_int = delta_pos >> 5;
                    for x in 0..width {
                        tmp[(y * width + x) as usize] = rm(d_int + 1 + x);
                    }
                    delta_pos += angle;
                }
            }
            for y in 0..height {
                let mut ang_scale = 0;
                if angle < 0 {
                    *do_pdpc = false;
                } else if angle > 0 {
                    let side = if pred_mode >= DIA as i32 { h0 } else { w0 };
                    ang_scale = 2.min(log2(side) - (log2(3 * inv_angle - 2) - 8));
                    *do_pdpc &= ang_scale >= 0;
                }
                if *do_pdpc {
                    let mut inv_sum = 256;
                    for x in 0..(3 << ang_scale).min(width) {
                        inv_sum += inv_angle;
                        let wl = 32 >> ((2 * x) >> ang_scale);
                        let left = ref_side[(y + (inv_sum >> 9) + 1) as usize];
                        let i = (y * width + x) as usize;
                        tmp[i] = (tmp[i] + ((wl * (left - tmp[i]) + 32) >> 6)) as i16 as i32;
                    }
                }
            }
        }
        if is_ver {
            dst[..(width * height) as usize].copy_from_slice(&tmp);
        } else {
            // transpose: tmp is height' x width' (swapped)
            for y in 0..height {
                for x in 0..width {
                    dst[(x * height + y) as usize] = tmp[(y * width + x) as usize];
                }
            }
        }
    }
}

fn pred_planar(src: &RefBuf, dst: &mut [i32], w: i32, h: i32) {
    let log2w = log2(w);
    let log2h = log2(h);
    let mut top: Vec<i32> = (0..=w).map(|k| src.at((k + 1) as usize, 0)).collect();
    let mut left: Vec<i32> = (0..=h).map(|k| src.at(0, (k + 1) as usize)).collect();
    let bottom_left = left[h as usize];
    let top_right = top[w as usize];
    let mut bottom = vec![0i32; w as usize];
    let mut right = vec![0i32; h as usize];
    for k in 0..w as usize {
        bottom[k] = bottom_left - top[k];
        top[k] <<= log2h;
    }
    for k in 0..h as usize {
        right[k] = top_right - left[k];
        left[k] <<= log2w;
    }
    let offset = 1 << (log2w + log2h);
    let shift = 1 + log2w + log2h;
    for y in 0..h as usize {
        let mut hor = left[y];
        for x in 0..w as usize {
            hor += right[y];
            top[x] += bottom[x];
            dst[y * w as usize + x] = ((hor << log2h) + (top[x] << log2w) + offset) >> shift;
        }
    }
}

fn pred_dc(src: &RefBuf, w: i32, h: i32, mrl: i32) -> i32 {
    let denom = if w == h { w << 1 } else { w.max(h) };
    let shift = log2(denom);
    let mut sum = 0;
    if w >= h {
        for i in 0..w {
            sum += src.at((mrl + 1 + i) as usize, 0);
        }
    }
    if w <= h {
        for i in 0..h {
            sum += src.at(0, (mrl + 1 + i) as usize);
        }
    }
    (sum + (denom >> 1)) >> shift
}

pub fn reconstruct_cu(pic: &mut Picture, si: &SliceInfo, cu_id: u32) -> Result<(), Error> {
    reconstruct_cu_comps(pic, si, cu_id, 0b111)
}

/// [`reconstruct_cu`] for the components in `mask` (bit n: component n);
/// the encoder predicts single components during mode decisions.
pub fn reconstruct_cu_comps(
    pic: &mut Picture,
    si: &SliceInfo,
    cu_id: u32,
    mask: u8,
) -> Result<(), Error> {
    let cu = pic.cus[cu_id as usize].clone();
    // DecCu::predAndReco for inter coding units: motion compensation
    let mut inter_pred: Option<super::mc::PredUnit> = if cu.pred == Pred::Inter {
        let mut m = super::mc::McCtx::new(pic, si);
        let u = super::mc::Mcu::from_cu(&cu);
        let p = if cu.geo {
            m.motion_compensation_geo(&u, cu.geo_dir_ref, cu.geo_split, [cu.mv[0][1], cu.mv[1][1]])?
        } else {
            m.motion_compensation(&u)?
        };
        if let Some(d) = m.dmvr.take() {
            pic.dmvr.push(d);
            pic.cus[cu_id as usize].dmvr = true;
        }
        Some(p)
    } else {
        None
    };
    // vvdec's xIntraBlockCopy: integer block vector, chroma vectors floored.
    let ibc_pred: Vec<Vec<i32>> = if cu.pred == Pred::Ibc {
        let r = |v: i32| if v >= 0 { (v + 7) >> 4 } else { (v + 8) >> 4 };
        let (bx, by) = (r(cu.bv.0), r(cu.bv.1));
        (0..pic.fmt.num_comp())
            .map(|comp| {
                let b = cu.blk[comp];
                if !b.valid() {
                    return Vec::new();
                }
                let (sx, sy) = pic.fmt.scale(comp);
                let (rx, ry) = (b.x + (bx >> sx), b.y + (by >> sy));
                let p = &pic.planes[comp];
                let mut out = Vec::with_capacity((b.w * b.h) as usize);
                for y in 0..b.h {
                    for x in 0..b.w {
                        let (px, py) = (
                            (rx + x).clamp(0, p.width as i32 - 1),
                            (ry + y).clamp(0, p.height as i32 - 1),
                        );
                        out.push(i32::from(p.at(px, py)));
                    }
                }
                out
            })
            .collect()
    } else {
        Vec::new()
    };
    let lw = cu.blk[0].w.max(1) as usize;
    let lh = cu.blk[0].h.max(1) as usize;
    let mut ctx = Ctx {
        pic,
        si,
        cu_id,
        cu: cu.clone(),
        wpp: si.sps.entropy_coding_sync,
        top_len: 0,
        left_len: 0,
        unfiltered: RefBuf::new(1, 1),
        filtered: RefBuf::new(1, 1),
        isp_base: [RefBuf::new(1, 1), RefBuf::new(1, 1)],
        luma_pred: vec![0; lw * lh],
        lm_stride: 0,
    };
    let num_comp = ctx.pic.fmt.num_comp();
    if let Some(pred) = inter_pred.as_mut() {
        // forward luma mapping of the inter prediction (Reshape::rspBufFwd)
        let fwd = si.sh.lmcs_used && si.ph.lmcs_enabled && si.sh.slice_type != super::ps::I_SLICE;
        let lmcs = si.lmcs;
        let map = |pred: &mut super::mc::PredUnit| {
            if let (true, Some(l)) = (fwd, lmcs) {
                let bd = ctx_bd(si);
                let shift = bd - 4;
                for v in pred[0].data.iter_mut() {
                    let idx = (*v >> shift) as usize;
                    *v = (l.pivot[idx]
                        + ((l.fwd_scale[idx] * (*v - l.input_pivot[idx]) + (1 << 10)) >> 11))
                        .clamp(0, (1 << bd) - 1);
                }
            }
        };
        map(pred);
        if cu.ciip {
            ciip_blend(&mut ctx, pred)?;
            if !cu.root_cbf {
                map(pred);
            }
        }
    }
    if num_comp > 1 {
        for t in cu.first_tu..cu.first_tu + cu.num_tu {
            let tu = ctx.pic.tus[t as usize].clone();
            for c in 1..3 {
                if tu.blk[c].valid() {
                    ctx.pic.tus[t as usize].cqp[c - 1] = qp_param(&ctx, &tu, c, false).0;
                }
            }
        }
    }
    let bd = ctx.pic.bit_depth;
    let max = (1i32 << bd) - 1;
    for t in cu.first_tu..cu.first_tu + cu.num_tu {
        let tu = ctx.pic.tus[t as usize].clone();
        // residuals for this TU
        let mut resi: [Option<Vec<i32>>; 3] = [None, None, None];
        if cu.root_cbf {
            compute_residuals(&ctx, &tu, &mut resi, cu.act)?;
        }
        if cu.act {
            act_convert(&mut resi, &tu, bd);
        }
        for comp in 0..num_comp {
            let area = tu.blk[comp];
            if !area.valid() || mask >> comp & 1 == 0 {
                continue;
            }
            let ch = if comp == 0 { 0 } else { 1 };
            let (w, h) = (area.w, area.h);
            let n = (w * h) as usize;
            let mut pred = vec![0i32; n];
            let mut use_region_pred = false;
            if let Some(ip) = inter_pred.as_ref() {
                let b = cu.blk[comp];
                let src = &ip[comp];
                for y in 0..h {
                    for x in 0..w {
                        pred[(y * w + x) as usize] = src.data[((area.y - b.y + y) as usize)
                            * src.stride
                            + (area.x - b.x + x) as usize];
                    }
                }
            } else if cu.pred == Pred::Ibc {
                let b = cu.blk[comp];
                let src = &ibc_pred[comp];
                for y in 0..h {
                    for x in 0..w {
                        pred[(y * w + x) as usize] =
                            src[((area.y - b.y + y) * b.w + area.x - b.x + x) as usize];
                    }
                }
            } else if if ch == 0 {
                cu.mip
            } else {
                is_dm_chroma_mip(ctx.pic, si, cu_id) && cu.intra_dir[1] == DM_CHROMA
            } {
                ctx.init_pattern(t, comp, area, false);
                pred_mip(&ctx, comp, &mut pred, w, h, bd)?;
            } else if comp != 0
                && (LM_CHROMA..=MDLM_T).contains(&final_intra_mode(ctx.pic, si, &cu, cu_id, ch))
            {
                ctx.init_pattern(t, comp, area, false);
                pred_lm(&mut ctx, comp, t, area, cu.intra_dir[1], &mut pred)?;
            } else {
                let pred_reg_diff = comp == 0
                    && cu.isp == VER_ISP
                    && ((cu.blk[0].w == 8 && cu.blk[0].h > 4) || cu.blk[0].w == 4);
                let first_in_reg =
                    comp == 0 && cu.isp != NOT_ISP && (area.x - cu.blk[0].x) % 4 == 0;
                let use_filtered = comp == 0
                    && cu.isp == NOT_ISP
                    && use_filtered_ref(ctx.pic, si, &cu, cu_id, comp, area);
                let mut reg = area;
                if cu.isp != NOT_ISP && comp == 0 {
                    if pred_reg_diff {
                        if first_in_reg {
                            reg.w = reg.w.max(4);
                            ctx.init_pattern_isp(reg);
                        }
                    } else {
                        ctx.init_pattern_isp(area);
                    }
                } else {
                    ctx.init_pattern(t, comp, area, use_filtered);
                }
                if pred_reg_diff {
                    if first_in_reg {
                        let mut p = vec![0i32; (reg.w * reg.h) as usize];
                        ctx.pred_intra_ang(comp, &mut p, reg.w, reg.h, use_filtered);
                        // store into the CU luma prediction buffer
                        let (ox, oy) = (
                            (reg.x - cu.blk[0].x) as usize,
                            (reg.y - cu.blk[0].y) as usize,
                        );
                        for y in 0..reg.h as usize {
                            for x in 0..reg.w as usize {
                                ctx.luma_pred[(oy + y) * lw + ox + x] = p[y * reg.w as usize + x];
                            }
                        }
                    }
                    use_region_pred = true;
                } else {
                    ctx.pred_intra_ang(comp, &mut pred, w, h, use_filtered);
                }
            }
            if use_region_pred {
                let (ox, oy) = (
                    (area.x - cu.blk[0].x) as usize,
                    (area.y - cu.blk[0].y) as usize,
                );
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        pred[y * w as usize + x] = ctx.luma_pred[(oy + y) * lw + ox + x];
                    }
                }
            }
            // chroma residual scaling
            let coded = tu.cbf(comp) || (comp != 0 && tu.joint != 0);
            let has_resi = coded || (cu.act && resi[comp].is_some());
            if let Some(r) = resi[comp].as_mut()
                && comp != 0
                && si.sh.lmcs_used
                && si.ph.chroma_residual_scale
                && area.area() > 4
                && coded
            {
                let lmcs = si.lmcs.ok_or(Error::Invalid("LMCS APS missing"))?;
                let (lx, ly) = if tu.blk[0].valid() {
                    (tu.blk[0].x, tu.blk[0].y)
                } else {
                    let c = tu.blk[tu.ch_type];
                    let (sx, sy) = ctx.pic.fmt.scale(tu.ch_type);
                    (c.x << sx, c.y << sy)
                };
                let scale = chroma_scale(&ctx, lmcs, lx, ly);
                let m = (1 << bd) - 1;
                for v in r.iter_mut() {
                    let s = (*v).clamp(-m - 1, m);
                    let sign = if s >= 0 { 1 } else { -1 };
                    let abs = sign * s;
                    *v = (sign * ((abs * scale + (1 << 10)) >> 11)).clamp(-32768, 32767);
                }
            }
            let plane = &mut ctx.pic.planes[comp];
            if has_resi {
                let r = resi[comp]
                    .as_ref()
                    .ok_or(Error::Invalid("missing residual"))?;
                for y in 0..h {
                    for x in 0..w {
                        let i = (y * w + x) as usize;
                        plane.set(
                            area.x + x,
                            area.y + y,
                            (pred[i] + r[i]).clamp(0, max) as i16,
                        );
                    }
                }
            } else {
                for y in 0..h {
                    for x in 0..w {
                        let i = (y * w + x) as usize;
                        plane.set(area.x + x, area.y + y, pred[i] as i16);
                    }
                }
            }
        }
    }
    // Coefficients are no longer needed.
    for t in cu.first_tu..cu.first_tu + cu.num_tu {
        let tu = &mut ctx.pic.tus[t as usize];
        tu.coeff = [Vec::new(), Vec::new(), Vec::new()];
    }
    Ok(())
}

/// Intra predictions of one component of an unsplit, single-transform
/// coding unit for several regular modes (luma modes for component 0,
/// chroma modes otherwise), sharing the reference samples; the encoder's
/// mode decision. Requires no MIP, MRL, ISP or BDPCM.
pub fn predict_modes(
    pic: &mut Picture,
    si: &SliceInfo,
    cu_id: u32,
    comp: usize,
    modes: &[u8],
) -> Vec<Vec<i32>> {
    let cu = pic.cus[cu_id as usize].clone();
    let area = cu.blk[comp];
    let lw = cu.blk[0].w.max(1) as usize;
    let lh = cu.blk[0].h.max(1) as usize;
    let mut ctx = Ctx {
        pic,
        si,
        cu_id,
        cu: cu.clone(),
        wpp: si.sps.entropy_coding_sync,
        top_len: 0,
        left_len: 0,
        unfiltered: RefBuf::new(1, 1),
        filtered: RefBuf::new(1, 1),
        isp_base: [RefBuf::new(1, 1), RefBuf::new(1, 1)],
        luma_pred: vec![0; lw * lh],
        lm_stride: 0,
    };
    ctx.init_pattern(cu.first_tu, comp, area, comp == 0);
    let ch = usize::from(comp != 0);
    modes
        .iter()
        .map(|&m| {
            ctx.cu.intra_dir[ch] = m;
            let filtered = comp == 0 && use_filtered_ref(ctx.pic, si, &ctx.cu, cu_id, comp, area);
            let mut pred = vec![0i32; (area.w * area.h) as usize];
            ctx.pred_intra_ang(comp, &mut pred, area.w, area.h, filtered);
            pred
        })
        .collect()
}

fn ctx_bd(si: &SliceInfo) -> u32 {
    si.sps.bit_depth
}

/// `IntraPrediction::predBlendIntraCiip`: planar intra prediction over the
/// whole coding unit blended with the inter prediction.
fn ciip_blend(ctx: &mut Ctx, pred: &mut super::mc::PredUnit) -> Result<(), Error> {
    let cu = ctx.cu.clone();
    let cu_id = ctx.cu_id;
    let chroma = ctx.pic.fmt.chroma != 0 && cu.blk[1].w > 2;
    let comps = if chroma { 3 } else { 1 };
    for comp in 0..comps {
        let area = cu.blk[comp];
        let use_filter = comp == 0 && use_filtered_ref(ctx.pic, ctx.si, &cu, cu_id, 0, area);
        ctx.init_pattern(cu.first_tu, comp, area, use_filter);
        let mut ip = vec![0i32; (area.w * area.h) as usize];
        ctx.pred_intra_ang(comp, &mut ip, area.w, area.h, use_filter);
        let (lx, ly, lw, lh) = (cu.blk[0].x, cu.blk[0].y, cu.blk[0].w, cu.blk[0].h);
        let left = ctx
            .pic
            .get_cu_restricted(lx - 1, ly + lh - 1, cu_id, 0, cu.left, ctx.wpp);
        let above = ctx
            .pic
            .get_cu_restricted(lx + lw - 1, ly - 1, cu_id, 0, cu.above, ctx.wpp);
        let n0 = left.is_some_and(|c| ctx.pic.cus[c as usize].pred == Pred::Intra);
        let n1 = above.is_some_and(|c| ctx.pic.cus[c as usize].pred == Pred::Intra);
        let w_intra = 3 - i32::from(!n0) - i32::from(!n1);
        let w_merge = 3 - i32::from(n0) - i32::from(n1);
        let b = &mut pred[comp];
        for y in 0..area.h as usize {
            for x in 0..area.w as usize {
                let i = y * b.stride + x;
                b.data[i] = (w_merge * b.data[i] + w_intra * ip[y * area.w as usize + x] + 2) >> 2;
            }
        }
    }
    Ok(())
}

fn act_convert(resi: &mut [Option<Vec<i32>>; 3], tu: &Tu, bd: u32) {
    let n = (tu.blk[0].w * tu.blk[0].h) as usize;
    for r in resi.iter_mut() {
        if r.is_none() {
            *r = Some(vec![0; n]);
        }
    }
    let m = (1i32 << (bd + 1)) - 1;
    let [r0, r1, r2] = resi;
    let (r0, r1, r2) = (
        r0.as_mut().unwrap(),
        r1.as_mut().unwrap(),
        r2.as_mut().unwrap(),
    );
    for i in 0..n {
        let y0 = r0[i].clamp(-m - 1, m);
        let cg = r1[i].clamp(-m - 1, m);
        let co = r2[i].clamp(-m - 1, m);
        let t = y0 - (cg >> 1);
        r0[i] = (cg + t) as i16 as i32;
        let d1 = (t - (co >> 1)) as i16 as i32;
        r1[i] = d1;
        r2[i] = (co + d1) as i16 as i32;
    }
}

fn chroma_scale(ctx: &Ctx, lmcs: &Lmcs, x: i32, y: i32) -> i32 {
    let pic = &*ctx.pic;
    let ctu_size = 1i32 << pic.ctu_log2;
    let num = 64.min(ctu_size);
    let num_log = log2(num);
    let (mut xp, mut yp) = (x, y);
    if ctu_size == 128 {
        xp &= !63;
        yp &= !63;
    } else {
        xp &= !(ctu_size - 1);
        yp &= !(ctu_size - 1);
    }
    let Some(tl) = pic.get_cu(xp, yp, 0) else {
        return 1 << 11;
    };
    let tlc = &pic.cus[tl as usize];
    let above = pic.get_cu_restricted(
        tlc.lx(),
        tlc.ly() - 1,
        tl,
        0,
        if tlc.ly() == yp { Some(tl) } else { tlc.above },
        ctx.wpp,
    );
    let left = pic.get_cu_restricted(
        tlc.lx() - 1,
        tlc.ly(),
        tl,
        0,
        if tlc.lx() == xp { Some(tl) } else { tlc.left },
        ctx.wpp,
    );
    let (xp, yp) = (tlc.lx(), tlc.ly());
    let plane = &pic.planes[0];
    let mut sum = 0i32;
    let mut cnt = 0;
    if left.is_some() {
        for i in 0..num {
            let k = if yp + i >= pic.height {
                pic.height - yp - 1
            } else {
                i
            };
            sum += i32::from(plane.at(xp - 1, yp + k));
            cnt += 1;
        }
    }
    if above.is_some() {
        for i in 0..num {
            let k = if xp + i >= pic.width {
                pic.width - xp - 1
            } else {
                i
            };
            sum += i32::from(plane.at(xp + k, yp - 1));
            cnt += 1;
        }
    }
    let luma = if cnt == num {
        (sum + (1 << (num_log - 1))) >> num_log
    } else if cnt == num << 1 {
        (sum + (1 << num_log)) >> (num_log + 1)
    } else {
        1 << (pic.bit_depth - 1)
    };
    lmcs.chroma_adj[lmcs.pwl_idx_inv(luma)]
}

fn pred_mip(ctx: &Ctx, comp: usize, dst: &mut [i32], w: i32, h: i32, bd: u32) -> Result<(), Error> {
    let cu = &ctx.cu;
    let (mode, transpose) = if comp == 0 {
        (cu.intra_dir[0] as usize, cu.mip_transposed)
    } else {
        let l = co_located_luma_cu(ctx.pic, ctx.si, ctx.cu_id);
        let lc = &ctx.pic.cus[l as usize];
        (lc.intra_dir[0] as usize, lc.mip_transposed)
    };
    let src = &ctx.unfiltered;
    let size_id = mip_size_id(w, h);
    let red_bdry = if size_id == 0 { 2 } else { 4 };
    let red_pred = if size_id < 2 { 4 } else { 8 };
    let up_hor = w / red_pred;
    let up_ver = h / red_pred;
    let top: Vec<i32> = (0..w).map(|x| src.at((x + 1) as usize, 0)).collect();
    let left: Vec<i32> = (0..h).map(|y| src.at(0, (y + 1) as usize)).collect();
    let down = |full: &[i32], dst_len: i32| -> Vec<i32> {
        let src_len = full.len() as i32;
        if dst_len < src_len {
            let f = src_len / dst_len;
            let l2 = log2(f);
            let r = 1 << (l2 - 1);
            (0..dst_len)
                .map(|i| {
                    (full[(i * f) as usize..((i + 1) * f) as usize]
                        .iter()
                        .sum::<i32>()
                        + r)
                        >> l2
                })
                .collect()
        } else {
            full[..dst_len as usize].to_vec()
        }
    };
    let top_red = down(&top, red_bdry);
    let left_red = down(&left, red_bdry);
    let mut bdry: Vec<i32> = top_red.iter().chain(left_red.iter()).copied().collect();
    let mut bdry_t: Vec<i32> = left_red.iter().chain(top_red.iter()).copied().collect();
    let in_size = (2 * red_bdry) as usize;
    let off = bdry[0];
    let off_t = bdry_t[0];
    let has_first = size_id < 2;
    bdry[0] = if has_first { (1 << (bd - 1)) - off } else { 0 };
    bdry_t[0] = if has_first {
        (1 << (bd - 1)) - off_t
    } else {
        0
    };
    for i in 1..in_size {
        bdry[i] -= off;
        bdry_t[i] -= off_t;
    }
    let input = if transpose { &bdry_t } else { &bdry };
    let input_offset = if transpose { off_t } else { off };
    let (matrix, cols): (&[u8], usize) = match size_id {
        0 => (&MIP_4X4[mode * 16 * 4..(mode + 1) * 16 * 4], 4),
        1 => (&MIP_8X8[mode * 16 * 8..(mode + 1) * 16 * 8], 8),
        _ => (&MIP_16X16[mode * 64 * 7..(mode + 1) * 64 * 7], 7),
    };
    let red = size_id == 2;
    let sum: i32 = input.iter().sum();
    let offset = (1 << 5) - 32 * sum;
    let mut reduced = vec![0i32; (red_pred * red_pred) as usize];
    let max = (1i32 << bd) - 1;
    let mut widx = 0usize;
    for pos in 0..(red_pred * red_pred) as usize {
        let mut acc = 0i32;
        for i in 0..in_size {
            if red && i == 0 {
                continue;
            }
            acc += input[i] * i32::from(matrix[widx + i - usize::from(red)]);
        }
        reduced[pos] = (((acc + offset) >> 6) + input_offset).clamp(0, max);
        widx += cols;
        let _ = cols;
    }
    if transpose {
        let r = reduced.clone();
        for y in 0..red_pred as usize {
            for x in 0..red_pred as usize {
                reduced[y * red_pred as usize + x] = r[x * red_pred as usize + y];
            }
        }
    }
    let need_up = up_hor > 1 || up_ver > 1;
    if !need_up {
        dst[..(w * h) as usize].copy_from_slice(&reduced);
        return Ok(());
    }
    // predictionUpsampling
    let mut out = vec![0i32; (w * h) as usize];
    let up1d = |dst: &mut [i32],
                dst_base: usize,
                src: &[i32],
                src_base: usize,
                bndry: &[i32],
                src_up: i32,
                src_orth: i32,
                src_step: usize,
                src_stride: usize,
                dst_step: usize,
                dst_stride: usize,
                bndry_step: usize,
                factor: i32| {
        let l2 = log2(factor);
        let r = 1 << (l2 - 1);
        let mut src_line = src_base;
        let mut dst_line = dst_base;
        let mut bndry_line = bndry_step - 1;
        for _ in 0..src_orth {
            let mut before = bndry[bndry_line];
            let mut behind_idx = src_line;
            let mut cur = dst_line;
            for _ in 0..src_up {
                let behind = src[behind_idx];
                let diff = behind - before;
                let mut scaled = before * (1 << l2) + r;
                for _ in 0..factor {
                    scaled += diff;
                    dst[cur] = scaled >> l2;
                    cur += dst_step;
                }
                before = behind;
                behind_idx += src_step;
            }
            src_line += src_stride;
            dst_line += dst_stride;
            bndry_line += bndry_step;
        }
    };
    let mut ver_src_step = w as usize;
    let ver_src: Vec<i32>;
    let ver_src_base;
    if up_hor > 1 {
        let hor_base = ((up_ver - 1) * w) as usize;
        ver_src_step *= up_ver as usize;
        let mut tmp = vec![0i32; (w * h) as usize];
        up1d(
            &mut tmp,
            hor_base,
            &reduced,
            0,
            &left,
            red_pred,
            red_pred,
            1,
            red_pred as usize,
            1,
            ver_src_step,
            up_ver as usize,
            up_hor,
        );
        out.copy_from_slice(&tmp);
        ver_src = tmp;
        ver_src_base = hor_base;
    } else {
        ver_src = reduced.clone();
        ver_src_base = 0;
    }
    if up_ver > 1 {
        let src_copy = ver_src;
        up1d(
            &mut out,
            0,
            &src_copy,
            ver_src_base,
            &top,
            red_pred,
            w,
            ver_src_step,
            1,
            w as usize,
            1,
            1,
            up_ver,
        );
    }
    dst[..(w * h) as usize].copy_from_slice(&out);
    Ok(())
}

fn pred_lm(
    ctx: &mut Ctx,
    comp: usize,
    tu_id: u32,
    area: Area,
    mode: u8,
    dst: &mut [i32],
) -> Result<(), Error> {
    let pic_fmt = ctx.pic.fmt;
    let (sx, sy) = (pic_fmt.sx as i32, pic_fmt.sy as i32);
    let cu = ctx.cu.clone();
    let (cw, ch) = (area.w, area.h);
    let mdlm = mode == MDLM_L || mode == MDLM_T;
    let stride = if mdlm { 2 * 64 + 1 } else { 64 + 1 } as usize;
    ctx.lm_stride = stride;
    // temp buffer with one row/column of border: index (x+1, y+1)
    let rows = if mdlm { 2 * 64 + 2 } else { 64 + 2 } as usize;
    let mut temp = vec![0i32; stride * rows + 2];
    let t_at = |x: i32, y: i32| ((y + 1) as usize) * stride + (x + 1) as usize;
    let lx = area.x << sx;
    let ly = area.y << sy;
    let plane = &ctx.pic.planes[0];
    // vvdec fetches the full MDLM template length from its padded picture
    // buffer; samples outside the picture are never used for the model.
    let (pw, ph) = (ctx.pic.width, ctx.pic.height);
    let rec = |x: i32, y: i32| {
        let (x, y) = (lx + x, ly + y);
        if x < 0 || y < 0 || x >= pw || y >= ph {
            0
        } else {
            i32::from(plane.at(x, y))
        }
    };
    let base_unit = 4;
    let unit_w = base_unit >> sx;
    let unit_h = base_unit >> sy;
    let cu_ch = cu.ch_type;
    let (tu_w, tu_h) = if cu_ch == 1 {
        (cw, ch)
    } else {
        (cw << sx, ch << sy)
    };
    let tu_w_units = tu_w / (base_unit >> if cu_ch == 1 { sx } else { 0 });
    let tu_h_units = tu_h / (base_unit >> if cu_ch == 1 { sy } else { 0 });
    let chroma_unit_w = base_unit >> sx;
    let chroma_unit_h = base_unit >> sy;
    let top_n = 2 * cw;
    let left_n = 2 * ch;
    let total_above = if mode == MDLM_T {
        (top_n + chroma_unit_w - 1) / chroma_unit_w
    } else {
        tu_w_units
    };
    let total_left = if mode == MDLM_L {
        (left_n + chroma_unit_h - 1) / chroma_unit_h
    } else {
        tu_h_units
    };
    let cb = cu.blk[1];
    let avail_left_unit = if cu.left.is_some() || area.x > cb.x {
        total_left
    } else {
        0
    };
    let left_ok = avail_left_unit >= tu_h_units;
    let avail_above_unit = if cu.above.is_some() || area.y > cb.y {
        total_above
    } else {
        0
    };
    let above_ok = avail_above_unit >= tu_w_units;
    let _ = (unit_w, unit_h);
    let first_row = (ly & ((1 << ctx.pic.ctu_log2) - 1)) == 0;
    let str_off = if pic_fmt.chroma == 3 { 0 } else { 1 };
    let mult = 1 << sx;
    let (c3, c5, c6, o3, s3, o5, s5, o6, s6): (
        [i32; 3],
        [i32; 5],
        [i32; 6],
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) = match pic_fmt.chroma {
        2 => (
            [2, 1, 1],
            [0, 2, 1, 1, 0],
            [2, 1, 1, 0, 0, 0],
            2,
            2,
            2,
            2,
            2,
            2,
        ),
        3 => (
            [1, 0, 0],
            [0, 1, 0, 0, 0],
            [1, 0, 0, 0, 0, 0],
            0,
            0,
            0,
            0,
            0,
            0,
        ),
        _ => (
            [2, 1, 1],
            [1, 4, 1, 1, 1],
            [2, 1, 1, 2, 1, 1],
            2,
            2,
            4,
            3,
            4,
            3,
        ),
    };
    let colloc = ctx.si.sps.chroma_ver_collocated;
    let log_sub_w = sx;
    if above_ok {
        let n = avail_above_unit * chroma_unit_w;
        for i in 0..n {
            let v = if first_row {
                let y = -1;
                if (i == 0 && !left_ok) || i == cw + n - 1 + log_sub_w {
                    (rec(mult * i, y) * c3[0]
                        + rec(mult * i, y) * c3[1]
                        + rec(mult * i + 1, y) * c3[2]
                        + o3)
                        >> s3
                } else {
                    (rec(mult * i, y) * c3[0]
                        + rec(mult * i - 1, y) * c3[1]
                        + rec(mult * i + 1, y) * c3[2]
                        + o3)
                        >> s3
                }
            } else if colloc {
                let y = -(1 << sy);
                if (i == 0 && !left_ok) || i == cw + n - 1 + log_sub_w {
                    (rec(mult * i, y - str_off) * c5[0]
                        + rec(mult * i, y) * c5[1]
                        + rec(mult * i, y) * c5[2]
                        + rec(mult * i + 1, y) * c5[3]
                        + rec(mult * i, y + str_off) * c5[4]
                        + o5)
                        >> s5
                } else {
                    (rec(mult * i, y - str_off) * c5[0]
                        + rec(mult * i, y) * c5[1]
                        + rec(mult * i - 1, y) * c5[2]
                        + rec(mult * i + 1, y) * c5[3]
                        + rec(mult * i, y + str_off) * c5[4]
                        + o5)
                        >> s5
                }
            } else {
                let y = -(1 << sy);
                if (i == 0 && !left_ok) || i == cw + n - 1 + log_sub_w {
                    ((rec(mult * i, y) * c6[0]
                        + rec(mult * i, y) * c6[1]
                        + rec(mult * i + 1, y) * c6[2])
                        + (rec(mult * i, y + str_off) * c6[3]
                            + rec(mult * i, y + str_off) * c6[4]
                            + rec(mult * i + 1, y + str_off) * c6[5])
                        + o6)
                        >> s6
                } else {
                    ((rec(mult * i, y) * c6[0]
                        + rec(mult * i - 1, y) * c6[1]
                        + rec(mult * i + 1, y) * c6[2])
                        + (rec(mult * i, y + str_off) * c6[3]
                            + rec(mult * i - 1, y + str_off) * c6[4]
                            + rec(mult * i + 1, y + str_off) * c6[5])
                        + o6)
                        >> s6
                }
            };
            temp[t_at(i, -1)] = v;
        }
    }
    if left_ok {
        let n = avail_left_unit * chroma_unit_h;
        let bx = -2 - log_sub_w;
        for j in 0..n {
            let yy = j << sy;
            let s = |dx: i32, dy: i32| rec(bx + dx, yy + dy);
            let v = if colloc {
                if (j == 0 && !above_ok) || j == ch + n - 1 + log_sub_w {
                    (s(1, 0) * c5[0]
                        + s(1, 0) * c5[1]
                        + s(0, 0) * c5[2]
                        + s(2, 0) * c5[3]
                        + s(1, str_off) * c5[4]
                        + o5)
                        >> s5
                } else {
                    (s(1, -str_off) * c5[0]
                        + s(1, 0) * c5[1]
                        + s(0, 0) * c5[2]
                        + s(2, 0) * c5[3]
                        + s(1, str_off) * c5[4]
                        + o5)
                        >> s5
                }
            } else {
                ((s(1, 0) * c6[0] + s(0, 0) * c6[1] + s(2, 0) * c6[2])
                    + (s(1, str_off) * c6[3] + s(0, str_off) * c6[4] + s(2, str_off) * c6[5])
                    + o6)
                    >> s6
            };
            temp[t_at(-1, j)] = v;
        }
    }
    if colloc {
        for j in 0..ch {
            let yy = j << sy;
            for i in 0..cw {
                let r = |dx: i32, dy: i32| rec(mult * i + dx, yy + dy);
                let v = if i == 0 && !left_ok {
                    if j == 0 && !above_ok {
                        (r(0, 0) * c5[0]
                            + r(0, 0) * c5[1]
                            + r(0, 0) * c5[2]
                            + r(1, 0) * c5[3]
                            + r(0, str_off) * c5[4]
                            + o5)
                            >> s5
                    } else {
                        (r(0, -str_off) * c5[0]
                            + r(0, 0) * c5[1]
                            + r(0, 0) * c5[2]
                            + r(1, 0) * c5[3]
                            + r(0, str_off) * c5[4]
                            + o5)
                            >> s5
                    }
                } else if j == 0 && !above_ok {
                    (r(0, 0) * c5[0]
                        + r(0, 0) * c5[1]
                        + r(-1, 0) * c5[2]
                        + r(1, 0) * c5[3]
                        + r(0, str_off) * c5[4]
                        + o5)
                        >> s5
                } else {
                    (r(0, -str_off) * c5[0]
                        + r(0, 0) * c5[1]
                        + r(-1, 0) * c5[2]
                        + r(1, 0) * c5[3]
                        + r(0, str_off) * c5[4]
                        + o5)
                        >> s5
                };
                temp[t_at(i, j)] = v;
            }
        }
    } else {
        for j in 0..ch {
            let yy = j << sy;
            for i in 0..cw {
                let x0 = i << log_sub_w;
                let r = |dx: i32, dy: i32| rec(x0 + dx, yy + dy);
                let v = if !left_ok && i == 0 {
                    (r(0, 0) * c6[0]
                        + r(1, 0) * c6[1]
                        + r(0, 0) * c6[2]
                        + r(0, 1) * c6[3]
                        + r(1, 1) * c6[4]
                        + r(0, 1) * c6[5]
                        + o6)
                        >> s6
                } else {
                    (r(0, 0) * c6[0]
                        + r(1, 0) * c6[1]
                        + r(-1, 0) * c6[2]
                        + r(0, 1) * c6[3]
                        + r(1, 1) * c6[4]
                        + r(-1, 1) * c6[5]
                        + o6)
                        >> s6
                };
                temp[t_at(i, j)] = v;
            }
        }
    }
    // xGetLMParameters
    let bd = ctx.pic.bit_depth;
    let unit_wc = base_unit >> sx;
    let unit_hc = base_unit >> sx; // vvdec uses the horizontal scale here
    let tu_wu = cw / unit_wc;
    let tu_hu = ch / unit_hc;
    let mut above_right = (top_n + unit_wc - 1) / unit_wc - tu_wu;
    let mut left_below = (left_n + unit_hc - 1) / unit_hc - tu_hu;
    let tu = ctx.pic.get_tu(ctx.cu_id, area.x, area.y, 1);
    let _ = tu_id;
    let cur = &ctx.unfiltered;
    let (mut above_av, mut left_av) = (false, false);
    let (mut top_num, mut left_num) = (0, 0);
    if mode == MDLM_T {
        let mut units = 0;
        if cu.above.is_some() || area.y > cb.y {
            units = tu_wu;
            above_right = if above_right > ch / unit_wc {
                ch / unit_wc
            } else {
                above_right
            };
            units += ctx.above_available(tu, 1, area.x + cw, area.y, above_right, unit_wc);
        }
        above_av = units >= tu_wu;
        top_num = unit_wc * units;
    } else if mode == MDLM_L {
        let mut units = 0;
        if cu.left.is_some() || area.x > cb.x {
            units = tu_hu;
            left_below = if left_below > cw / unit_hc {
                cw / unit_hc
            } else {
                left_below
            };
            units += ctx.left_available(tu, 1, area.x, area.y + ch, left_below, unit_hc);
        }
        left_av = units >= tu_hu;
        left_num = unit_hc * units;
    } else {
        above_av = cu.above.is_some() || area.y > cb.y;
        left_av = cu.left.is_some() || area.x > cb.x;
        top_num = cw;
        left_num = ch;
    }
    let above_is4 = if left_av { 0 } else { 1 };
    let left_is4 = if above_av { 0 } else { 1 };
    let start = [top_num >> (2 + above_is4), left_num >> (2 + left_is4)];
    let step = [
        1.max(top_num >> (1 + above_is4)),
        1.max(left_num >> (1 + left_is4)),
    ];
    let mut sel_l = [0i32; 4];
    let mut sel_c = [0i32; 4];
    let mut cnt_t = 0;
    let mut cnt_l = 0;
    if above_av {
        cnt_t = top_num.min((1 + above_is4) << 1);
        let mut pos = start[0];
        for c in 0..cnt_t as usize {
            sel_l[c] = temp[t_at(pos, -1)];
            sel_c[c] = cur.at((pos + 1) as usize, 0);
            pos += step[0];
        }
    }
    if left_av {
        cnt_l = left_num.min((1 + left_is4) << 1);
        let mut pos = start[1];
        for c in 0..cnt_l as usize {
            sel_l[c + cnt_t as usize] = temp[t_at(-1, pos)];
            sel_c[c + cnt_t as usize] = cur.at(0, (pos + 1) as usize);
            pos += step[1];
        }
    }
    let cnt = cnt_l + cnt_t;
    if cnt == 2 {
        sel_l[3] = sel_l[0];
        sel_c[3] = sel_c[0];
        sel_l[2] = sel_l[1];
        sel_c[2] = sel_c[1];
        sel_l[0] = sel_l[1];
        sel_c[0] = sel_c[1];
        sel_l[1] = sel_l[3];
        sel_c[1] = sel_c[3];
    }
    let mut min_g = [0usize, 2];
    let mut max_g = [1usize, 3];
    if sel_l[min_g[0]] > sel_l[min_g[1]] {
        min_g.swap(0, 1);
    }
    if sel_l[max_g[0]] > sel_l[max_g[1]] {
        max_g.swap(0, 1);
    }
    if sel_l[min_g[0]] > sel_l[max_g[1]] {
        std::mem::swap(&mut min_g, &mut max_g);
    }
    if sel_l[min_g[1]] > sel_l[max_g[0]] {
        std::mem::swap(&mut min_g[1], &mut max_g[0]);
    }
    let min_luma = [
        (sel_l[min_g[0]] + sel_l[min_g[1]] + 1) >> 1,
        (sel_c[min_g[0]] + sel_c[min_g[1]] + 1) >> 1,
    ];
    let max_luma = [
        (sel_l[max_g[0]] + sel_l[max_g[1]] + 1) >> 1,
        (sel_c[max_g[0]] + sel_c[max_g[1]] + 1) >> 1,
    ];
    let (a, b, shift);
    if left_av || above_av {
        let diff = max_luma[0] - min_luma[0];
        if diff > 0 {
            let diff_c = max_luma[1] - min_luma[1];
            let mut x = log2(diff);
            const DIV_SIG: [i32; 16] = [0, 7, 6, 5, 5, 4, 4, 3, 3, 2, 2, 1, 1, 1, 1, 0];
            let norm = ((diff << 4) >> x) & 15;
            let v = DIV_SIG[norm as usize] | 8;
            x += i32::from(norm != 0);
            let y = if diff_c == 0 {
                0
            } else {
                log2(diff_c.abs()) + 1
            };
            let add = (1 << y) >> 1;
            let mut aa = (diff_c * v + add) >> y;
            let mut sh = 3 + x - y;
            if sh < 1 {
                sh = 1;
                aa = if aa == 0 {
                    0
                } else if aa < 0 {
                    -15
                } else {
                    15
                };
            }
            a = aa;
            shift = sh;
            b = min_luma[1] - ((aa * min_luma[0]) >> sh);
        } else {
            a = 0;
            b = min_luma[1];
            shift = 0;
        }
    } else {
        a = 0;
        b = 1 << (bd - 1);
        shift = 0;
    }
    let max = (1i32 << bd) - 1;
    for y in 0..ch {
        for x in 0..cw {
            let v = temp[t_at(x, y)];
            dst[(y * cw + x) as usize] = (((a * v) >> shift) + b).clamp(0, max);
        }
    }
    let _ = comp;
    Ok(())
}

// ---------------------------------------------------------------------------
// Residuals

fn qp_param(ctx: &Ctx, tu: &Tu, comp: usize, act: bool) -> (i32, i32) {
    let sps = ctx.si.sps;
    let pps = ctx.si.pps;
    let off = sps.qp_bd_offset;
    let cu = &ctx.cu;
    let jcc = comp != 0 && ictt_mode(tu, false) == 2;
    let jc = if jcc { 2 } else { comp.saturating_sub(1) };
    let qpy = cu.qp;
    let mut base = if comp == 0 {
        qpy + off
    } else {
        let pps_off = match jc {
            0 => pps.cb_qp_offset,
            1 => pps.cr_qp_offset,
            _ => pps.joint_cbcr_qp_offset,
        };
        let mut o = pps_off + ctx.si.sh.chroma_qp_delta[jc];
        o += pps
            .chroma_qp_offset_list
            .get(cu.chroma_qp_adj as usize)
            .map_or(0, |e| e[jc]);
        let qpi = qpy.clamp(-off, 63);
        let mapped = sps.chroma_qp_table[jc][(qpi + off) as usize];
        (mapped + o + off).clamp(0, 63 + off)
    };
    if act && cu.act {
        const DELTA: [i32; 4] = [-5, 1, 3, 1];
        let idx = if comp == 0 {
            0
        } else if jcc {
            3
        } else {
            comp
        };
        base += DELTA[idx];
        base = base.clamp(0, 63 + off);
    }
    let ts_base = base.max(4 + 6 * sps.internal_minus_input_bit_depth as i32);
    (base, ts_base)
}

fn ictt_mode(tu: &Tu, sign: bool) -> i32 {
    const MODES: [[i32; 4]; 2] = [[0, 3, 1, 2], [0, -3, -1, -2]];
    MODES[usize::from(sign)][tu.joint as usize]
}

fn compute_residuals(
    ctx: &Ctx,
    tu: &Tu,
    resi: &mut [Option<Vec<i32>>; 3],
    act: bool,
) -> Result<(), Error> {
    for comp in 0..ctx.pic.fmt.num_comp() {
        let area = tu.blk[comp];
        if !area.valid() {
            continue;
        }
        if tu.joint != 0 && comp != 0 {
            if comp == 1 {
                let n = (tu.blk[1].w * tu.blk[1].h) as usize;
                let (mut cb, mut cr) = (vec![0i32; n], vec![0i32; n]);
                if tu.joint >> 1 != 0 {
                    cb = inv_transform(ctx, tu, 1, act)?;
                } else {
                    cr = inv_transform(ctx, tu, 2, act)?;
                }
                let mode = ictt_mode(tu, ctx.si.ph.joint_cbcr_sign);
                for i in 0..n {
                    match mode {
                        1 => cr[i] = (cb[i] as i16 >> 1) as i32,
                        -1 => cr[i] = ((-cb[i]) >> 1) as i16 as i32,
                        2 => cr[i] = cb[i],
                        -2 => cr[i] = (-cb[i]) as i16 as i32,
                        3 => cb[i] = (cr[i] as i16 >> 1) as i32,
                        -3 => cb[i] = ((-cr[i]) >> 1) as i16 as i32,
                        _ => {}
                    }
                }
                resi[1] = Some(cb);
                resi[2] = Some(cr);
            }
        } else if tu.cbf(comp) {
            resi[comp] = Some(inv_transform(ctx, tu, comp, act)?);
        }
    }
    Ok(())
}

fn tr_types(ctx: &Ctx, tu: &Tu, comp: usize) -> (u8, u8) {
    // 0 = DCT2, 1 = DCT8, 2 = DST7
    let sps = ctx.si.sps;
    let cu = &ctx.cu;
    let luma = comp == 0;
    let intra = cu.pred == Pred::Intra;
    let implicit = intra && luma && sps.mts && !sps.explicit_mts_intra && cu.lfnst == 0 && !cu.mip;
    let isp = intra && luma && cu.isp != NOT_ISP;
    if isp && cu.lfnst != 0 {
        return (0, 0);
    }
    if !sps.mts {
        return (0, 0);
    }
    let (lw, lh) = (tu.blk[0].w, tu.blk[0].h);
    if implicit || isp {
        let h = if (4..=16).contains(&lw) { 2 } else { 0 };
        let v = if (4..=16).contains(&lh) { 2 } else { 0 };
        return (h, v);
    }
    let inter = cu.pred == Pred::Inter && luma;
    let explicit = if intra {
        sps.explicit_mts_intra && luma
    } else {
        sps.explicit_mts_inter && inter
    };
    if inter && cu.sbt != 0 {
        let idx = cu.sbt & 0xf;
        let pos0 = (cu.sbt >> 4) & 3 == 0;
        // SBT_VER_HALF / SBT_VER_QUAD
        return if idx == 1 || idx == 3 {
            if lh > 32 {
                (0, 0)
            } else if pos0 {
                (1, 2)
            } else {
                (2, 2)
            }
        } else if lw > 32 {
            (0, 0)
        } else if pos0 {
            (2, 1)
        } else {
            (2, 2)
        };
    }
    if explicit && tu.mts[comp] > MTS_SKIP {
        let ind_h = (tu.mts[comp] - MTS_DST7_DST7) & 1;
        let ind_v = (tu.mts[comp] - MTS_DST7_DST7) >> 1;
        return (
            if ind_h != 0 { 1 } else { 2 },
            if ind_v != 0 { 1 } else { 2 },
        );
    }
    (0, 0)
}

pub(super) fn matrix(tr: u8, n: usize) -> &'static [i16] {
    match (tr, n) {
        (0, 2) => &DCT2_2,
        (0, 4) => &DCT2_4,
        (0, 8) => &DCT2_8,
        (0, 16) => &DCT2_16,
        (0, 32) => &DCT2_32,
        (0, 64) => &DCT2_64,
        (1, 4) => &DCT8_4,
        (1, 8) => &DCT8_8,
        (1, 16) => &DCT8_16,
        (1, 32) => &DCT8_32,
        (2, 4) => &DST7_4,
        (2, 8) => &DST7_8,
        (2, 16) => &DST7_16,
        _ => &DST7_32,
    }
}

fn inv_transform(ctx: &Ctx, tu: &Tu, comp: usize, act: bool) -> Result<Vec<i32>, Error> {
    let area = tu.blk[comp];
    let (w, h) = (area.w as usize, area.h as usize);
    let mut coeff = dequant(ctx, tu, comp, act)?;
    let mut max_scan = tu.max_scan[comp];
    if ctx.si.sps.lfnst {
        inv_lfnst(ctx, tu, comp, &mut coeff, &mut max_scan);
    }
    if tu.mts[comp] == MTS_SKIP {
        return Ok(coeff.iter().map(|&v| v as i16 as i32).collect());
    }
    let bd = ctx.pic.bit_depth as i32;
    let (tr_h, tr_v) = tr_types(ctx, tu, comp);
    let clip_min = -(1 << 15);
    let clip_max = (1 << 15) - 1;
    let mut out = vec![0i32; w * h];
    if max_scan == (0, 0) && tr_h == 0 && tr_v == 0 {
        let dc = if w > 1 && h > 1 {
            let s1 = 7;
            let s2 = 20 - bd;
            let v = ((coeff[0] * 64) + (1 << (s1 - 1))) >> s1;
            ((v * 64) + (1 << (s2 - 1))) >> s2
        } else {
            let s = 21 - bd;
            ((coeff[0] * 64) + (1 << (s - 1))) >> s
        };
        out.fill(dc);
        return Ok(out);
    }
    if w > 1 && h > 1 {
        let s1 = 7;
        let s2 = 20 - bd;
        // Columns and rows past the last non-zero coefficient contribute
        // nothing; 32-bit accumulation cannot overflow (vvdec uses int).
        let (mut mx, mut my) = (0usize, 0usize);
        for (i, &c) in coeff.iter().enumerate() {
            if c != 0 {
                mx = mx.max(i % w);
                my = my.max(i / w);
            }
        }
        // vertical first: tmp[x][j]
        let mv = matrix(tr_v, h);
        let mut tmp = vec![0i32; w * h];
        let mut acc = vec![0i32; h.max(w)];
        for x in 0..=mx {
            acc[..h].fill(0);
            for k in 0..=my {
                let c = coeff[k * w + x];
                if c != 0 {
                    let row = &mv[k * h..k * h + h];
                    for (a, &m) in acc[..h].iter_mut().zip(row) {
                        *a += c * i32::from(m);
                    }
                }
            }
            for (t, &a) in tmp[x * h..x * h + h].iter_mut().zip(&acc[..h]) {
                *t = ((a + (1 << (s1 - 1))) >> s1).clamp(clip_min, clip_max);
            }
        }
        let mh = matrix(tr_h, w);
        for y in 0..h {
            acc[..w].fill(0);
            for k in 0..=mx {
                let c = tmp[k * h + y];
                if c != 0 {
                    let row = &mh[k * w..k * w + w];
                    for (a, &m) in acc[..w].iter_mut().zip(row) {
                        *a += c * i32::from(m);
                    }
                }
            }
            for (o, &a) in out[y * w..y * w + w].iter_mut().zip(&acc[..w]) {
                *o = ((a + (1 << (s2 - 1))) >> s2).clamp(clip_min, clip_max);
            }
        }
    } else {
        let s = 21 - bd;
        let (n, tr) = if w == 1 { (h, tr_v) } else { (w, tr_h) };
        let m = matrix(tr, n);
        for j in 0..n {
            let mut acc = 0i64;
            for k in 0..n {
                acc += i64::from(coeff[k]) * i64::from(m[k * n + j]);
            }
            out[j] = ((acc + (1 << (s - 1))) >> s).clamp(i64::from(clip_min), i64::from(clip_max))
                as i32;
        }
    }
    Ok(out)
}

fn inv_lfnst(ctx: &Ctx, tu: &Tu, comp: usize, coeff: &mut [i32], max_scan: &mut (i32, i32)) {
    let cu = &ctx.cu;
    let area = tu.blk[comp];
    let (w, h) = (area.w, area.h);
    let lfnst = cu.lfnst as usize;
    let sep = cu.is_sep_tree(ctx.si.dual_tree());
    if !(lfnst != 0 && tu.mts[comp] != MTS_SKIP && (if sep { true } else { comp == 0 })) {
        return;
    }
    let whge3 = w >= 8 && h >= 8;
    let scan: Vec<u16> = if whge3 {
        // top-left 8x8 in grouped 4x4 order with stride w
        let mut s = Vec::with_capacity(16);
        let d = super::ps::diag_scan(4, 4);
        for &p in &d {
            let (x, y) = (p as i32 % 4, p as i32 / 4);
            s.push((y * w + x) as u16);
        }
        s
    } else {
        grouped_scan_cached(w, h)
    };
    let ch = if comp == 0 { 0 } else { 1 };
    let is_mip = if ch == 0 {
        cu.mip
    } else {
        is_dm_chroma_mip(ctx.pic, ctx.si, ctx.cu_id) && cu.intra_dir[1] == DM_CHROMA
    };
    let mut mode = if is_mip {
        PLANAR as i32
    } else if (LM_CHROMA..=MDLM_T).contains(&cu.intra_dir[ch]) {
        co_located_intra_luma_mode(ctx.pic, ctx.si, ctx.cu_id) as i32
    } else {
        final_intra_mode(ctx.pic, ctx.si, cu, ctx.cu_id, ch) as i32
    };
    // PU::getWideAngIntraMode (uses the CU size for ISP luma)
    let (aw, ah) = if cu.isp != NOT_ISP && comp == 0 {
        (cu.blk[0].w, cu.blk[0].h)
    } else {
        (w, h)
    };
    if mode >= 2 {
        const SHIFT: [i32; 6] = [0, 6, 10, 12, 14, 15];
        let delta = (log2(aw) - log2(ah)).unsigned_abs() as usize;
        if aw > ah && mode < 2 + SHIFT[delta] {
            mode += VDIA as i32 - 1;
        } else if ah > aw && mode > VDIA as i32 - SHIFT[delta] {
            mode -= VDIA as i32 + 1;
        }
    }
    let intra_mode = if mode < 0 {
        (mode + 14 + 67) as usize
    } else if mode >= 67 {
        (mode + 14) as usize
    } else {
        mode as usize
    };
    let transpose = intra_mode >= 67 + 14 || (intra_mode < 67 && intra_mode > DIA as usize);
    let sb = if whge3 { 8 } else { 4 };
    let small = (w == 4 && h == 4) || (w == 8 && h == 8);
    let zero_out = if small { 8 } else { 16 };
    let input: Vec<i32> = (0..16).map(|i| coeff[scan[i] as usize]).collect();
    let set = LFNST_LUT[intra_mode] as usize;
    let idx = lfnst - 1;
    let tr_size = if sb > 4 { 48 } else { 16 };
    let mut out = vec![0i32; tr_size];
    for (j, o) in out.iter_mut().enumerate() {
        let mut acc = 0i32;
        for (i, &v) in input.iter().enumerate().take(zero_out) {
            let m = if sb > 4 {
                LFNST_8X8[((set * 2 + idx) * 48 + j) * 16 + i]
            } else {
                LFNST_4X4[((set * 2 + idx) * 16 + j) * 16 + i]
            };
            acc += v * i32::from(m);
        }
        *o = ((acc + 64) >> 7).clamp(-(1 << 15), (1 << 15) - 1);
    }
    let wu = w as usize;
    if transpose {
        if sb == 4 {
            for y in 0..4 {
                coeff[y * wu] = out[y];
                coeff[y * wu + 1] = out[y + 4];
                coeff[y * wu + 2] = out[y + 8];
                coeff[y * wu + 3] = out[y + 12];
            }
        } else {
            for y in 0..8 {
                coeff[y * wu] = out[y];
                coeff[y * wu + 1] = out[y + 8];
                coeff[y * wu + 2] = out[y + 16];
                coeff[y * wu + 3] = out[y + 24];
                if y < 4 {
                    coeff[y * wu + 4] = out[y + 32];
                    coeff[y * wu + 5] = out[y + 36];
                    coeff[y * wu + 6] = out[y + 40];
                    coeff[y * wu + 7] = out[y + 44];
                }
            }
        }
    } else {
        let mut k = 0;
        for y in 0..sb {
            let s = if y < 4 { sb } else { 4 };
            for x in 0..s {
                coeff[y * wu + x] = out[k];
                k += 1;
            }
        }
    }
    max_scan.0 = max_scan.0.max((w - 1).min(7));
    max_scan.1 = max_scan.1.max((h - 1).min(7));
}

fn dequant(ctx: &Ctx, tu: &Tu, comp: usize, act: bool) -> Result<Vec<i32>, Error> {
    let sps = ctx.si.sps;
    let area = tu.blk[comp];
    let (w, h) = (area.w as usize, area.h as usize);
    let src = &tu.coeff[comp];
    let mut out = vec![0i32; w * h];
    if src.is_empty() {
        return Ok(out);
    }
    let cu = &ctx.cu;
    let is_ts = tu.mts[comp] == MTS_SKIP;
    let scaling_used = ctx.si.sh.explicit_scaling_list_used;
    let disable_lfnst = if scaling_used {
        sps.scaling_matrix_for_lfnst_disabled
    } else {
        false
    };
    let lfnst_applied = cu.lfnst > 0
        && (if cu.is_sep_tree(ctx.si.dual_tree()) {
            true
        } else {
            comp == 0
        });
    let disable_act = sps.scaling_matrix_for_alt_colour_space_disabled
        && sps.scaling_matrix_designated_colour_space == cu.act;
    let enable_sl = scaling_used && !is_ts && (!lfnst_applied || !disable_lfnst) && !disable_act;
    let list_type = if cu.pred == Pred::Intra {
        comp
    } else {
        3 + comp
    };
    let bd = ctx.pic.bit_depth as i32;
    let bdpcm = (cu.bdpcm[0] != 0 && comp == 0) || (cu.bdpcm[1] != 0 && comp != 0);
    let (max_x, max_y);
    let mut levels: Vec<i32>;
    if bdpcm {
        levels = vec![0i32; w * h];
        let (mn, mx) = (-(1i32 << 15), (1i32 << 15) - 1);
        let mode = if comp == 0 { cu.bdpcm[0] } else { cu.bdpcm[1] };
        if mode == 1 {
            for y in 0..h {
                levels[y * w] = src[y * w];
                for x in 1..w {
                    levels[y * w + x] = (levels[y * w + x - 1] + src[y * w + x]).clamp(mn, mx);
                }
            }
        } else {
            levels[..w].copy_from_slice(&src[..w]);
            for y in 0..h - 1 {
                for x in 0..w {
                    levels[(y + 1) * w + x] =
                        (levels[y * w + x] + src[(y + 1) * w + x]).clamp(mn, mx);
                }
            }
        }
        max_x = w as i32 - 1;
        max_y = h as i32 - 1;
    } else {
        levels = src.clone();
        max_x = tu.max_scan[comp].0;
        max_y = tu.max_scan[comp].1;
    }
    let log2w = log2(w as i32);
    let log2h = log2(h as i32);
    let transform_shift = 15 - bd - ((log2w + log2h) >> 1);
    let sqrt_adj = !is_ts && ((log2w + log2h) & 1) == 1;
    let tshift = transform_shift + if sqrt_adj { -1 } else { 0 };
    let dep = ctx.si.sh.dep_quant && !is_ts;
    let (qp, qp_ts) = qp_param(ctx, tu, comp, act);
    let q = if is_ts { qp_ts } else { qp };
    let (per, rem) = if dep {
        ((q + 1) / 6, q + 1 - 6 * ((q + 1) / 6))
    } else {
        (q / 6, q % 6)
    };
    let right_shift = 6 + i32::from(dep) - ((if is_ts { 0 } else { tshift }) + per)
        + if enable_sl { 4 } else { 0 };
    let scale_qp = INV_QUANT_SCALES[usize::from(sqrt_adj)][rem as usize];
    let scale_bits = 7;
    let target = 16u32.min((32 + right_shift - scale_bits) as u32);
    if target < 8 {
        return Err(Error::Invalid("Invalid bit depth"));
    }
    let input_max = (1i64 << (target - 1)) - 1;
    let input_min = -(input_max + 1);
    let tmax = (1i64 << 15) - 1;
    let tmin = -(tmax + 1);
    let sl = if enable_sl {
        Some(
            ctx.si
                .scaling
                .ok_or(Error::Invalid("scaling list missing"))?
                .get(list_type, log2w as usize, log2h as usize),
        )
    } else {
        None
    };
    for y in 0..=max_y as usize {
        for x in 0..=max_x as usize {
            if y >= h || x >= w {
                continue;
            }
            let n = y * w + x;
            let level = levels[n];
            if level == 0 {
                continue;
            }
            let scale = i64::from(match sl {
                Some(m) => m[n] * scale_qp,
                None => scale_qp,
            });
            let c = i64::from(level).clamp(input_min, input_max);
            let v = if right_shift > 0 {
                (c * scale + (1i64 << (right_shift - 1))) >> right_shift
            } else {
                (c * scale) << (-right_shift)
            };
            out[n] = v.clamp(tmin, tmax) as i32;
        }
    }
    Ok(out)
}
