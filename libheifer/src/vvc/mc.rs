// SPDX-License-Identifier: LGPL-3.0-or-later
//! Motion compensated prediction following vvdec's `InterPrediction`,
//! `InterpolationFilter` and `WeightPrediction`: interpolation, weighted and
//! BCW averaging, BDOF, DMVR, affine prediction with PROF, subblock motion
//! and geometric partitioning.
//!
//! Reference samples are read with coordinates clamped to the picture, which
//! reproduces vvdec's border-extended reference buffers for the clipped
//! motion vectors it uses.
use super::Error;
use super::ctu::SliceInfo;
use super::dpb::{DmvrRefinement, RefPic};
use super::mv::*;
use super::mvpred::subblock_spread_over_limit;
use super::pic::*;
use super::ps::{B_SLICE, P_SLICE, WpParam};
use super::tables_inter::*;

const IF_INTERNAL_OFFS: i32 = 1 << 13;
const IF_INTERNAL_PREC: i32 = 14;

/// A block of samples with its own stride.
#[derive(Clone, Default)]
pub struct Buf {
    pub data: Vec<i32>,
    pub stride: usize,
    pub w: usize,
    pub h: usize,
}

impl Buf {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            data: vec![0; w * h],
            stride: w,
            w,
            h,
        }
    }
    #[inline]
    fn at(&self, x: usize, y: usize) -> i32 {
        self.data[y * self.stride + x]
    }
    #[inline]
    fn set(&mut self, x: usize, y: usize, v: i32) {
        self.data[y * self.stride + x] = v;
    }
}

/// Prediction of one coding unit, per component.
pub type PredUnit = [Buf; 3];

#[inline]
fn pel(v: i32) -> i32 {
    // vvdec stores intermediate samples as 16-bit `Pel`
    v as i16 as i32
}

/// Reference samples of a rectangle, read with clamped coordinates.
fn fetch(plane: &Plane, x0: i32, y0: i32, w: usize, h: usize) -> Buf {
    let mut b = Buf::new(w, h);
    let (pw, ph) = (plane.width as i32, plane.height as i32);
    for y in 0..h {
        let py = (y0 + y as i32).clamp(0, ph - 1) as usize;
        let row = &plane.data[py * plane.stride..py * plane.stride + plane.width];
        let out = &mut b.data[y * w..(y + 1) * w];
        for (x, o) in out.iter_mut().enumerate() {
            let px = (x0 + x as i32).clamp(0, pw - 1) as usize;
            *o = i32::from(row[px]);
        }
    }
    b
}

/// vvdec's `InterpolationFilter::filter<N, isVertical, isFirst, isLast>`.
/// `src` holds the block origin at `(ox, oy)`.
#[allow(clippy::too_many_arguments)]
fn filter(
    src: &Buf,
    ox: usize,
    oy: usize,
    dst: &mut Buf,
    dx: usize,
    dy: usize,
    w: usize,
    h: usize,
    coeff: &[i32],
    vertical: bool,
    first: bool,
    last: bool,
    bd: u32,
) {
    let n = coeff.len();
    let bd = bd as i32;
    let head = 2.max(IF_INTERNAL_PREC - bd);
    let (shift, offset) = if n == 2 {
        if first {
            let s = 4 - (10 - bd);
            (s, 1 << (s - 1))
        } else {
            (4, 1 << 3)
        }
    } else if last {
        let s = 6 + if first { 0 } else { head };
        (
            s,
            (1 << (s - 1)) + if first { 0 } else { IF_INTERNAL_OFFS << 6 },
        )
    } else {
        let s = 6 - if first { head } else { 0 };
        (
            s,
            if first {
                -IF_INTERNAL_OFFS * (1 << s)
            } else {
                0
            },
        )
    };
    let max = (1 << bd) - 1;
    let back = n / 2 - 1;
    for r in 0..h {
        for c in 0..w {
            let mut sum = 0i32;
            for (k, &co) in coeff.iter().enumerate() {
                let v = if vertical {
                    src.at(ox + c, oy + r + k - back)
                } else {
                    src.at(ox + c + k - back, oy + r)
                };
                sum += v * co;
            }
            let mut v = pel((sum + offset) >> shift);
            if last {
                v = v.clamp(0, max);
            }
            dst.set(dx + c, dy + r, v);
        }
    }
}

/// vvdec's `filterCopy<isFirst, isLast>`.
#[allow(clippy::too_many_arguments)]
fn filter_copy(
    src: &Buf,
    ox: usize,
    oy: usize,
    dst: &mut Buf,
    dx: usize,
    dy: usize,
    w: usize,
    h: usize,
    first: bool,
    last: bool,
    bd: u32,
    bilinear: bool,
) {
    let bd = bd as i32;
    let shift = 2.max(IF_INTERNAL_PREC - bd);
    let max = (1 << bd) - 1;
    for r in 0..h {
        for c in 0..w {
            let s = src.at(ox + c, oy + r);
            let v = if first == last {
                s
            } else if first {
                if bilinear {
                    s * (1 << (10 - bd))
                } else {
                    pel(s * (1 << shift) - IF_INTERNAL_OFFS)
                }
            } else {
                ((s + IF_INTERNAL_OFFS + (1 << (shift - 1))) >> shift).clamp(0, max)
            };
            dst.set(dx + c, dy + r, v);
        }
    }
}

/// Interpolation filter choice for luma and chroma (vvdec's `filterHor`,
/// `filterVer`, `filter4x4`, `filter8xH`, `filter16xH`).
fn luma_coeff(frac: usize, alt: bool, small: bool) -> &'static [i32] {
    if frac == 8 && alt {
        &LUMA_ALT_HPEL
    } else if small {
        &LUMA_FILTER_4X4[frac]
    } else {
        &LUMA_FILTER[frac]
    }
}

/// Parameters of one call of vvdec's `xPredInterBlk`.
pub struct BlkParams {
    pub comp: usize,
    pub bi: bool,
    pub alt_hpel: bool,
    pub bilinear: bool,
    pub bd: u32,
    pub sx: u32,
    pub sy: u32,
}

/// Interpolates a `w`x`h` block whose integer origin in `src` is at
/// `(ox, oy)` with fractional offsets `(fx, fy)`.
#[allow(clippy::too_many_arguments)]
fn interp(
    src: &Buf,
    ox: usize,
    oy: usize,
    fx: usize,
    fy: usize,
    w: usize,
    h: usize,
    p: &BlkParams,
    dst: &mut Buf,
    dx: usize,
    dy: usize,
) {
    let luma = p.comp == 0;
    let last = !p.bi;
    let small = w == 4 && h == 4;
    let chroma_x = fx << (1 - p.sx);
    let chroma_y = fy << (1 - p.sy);
    let coeff_x: &[i32] = if p.bilinear {
        &BILINEAR_PREC4[fx]
    } else if luma {
        luma_coeff(fx, p.alt_hpel, small)
    } else {
        &CHROMA_FILTER[chroma_x]
    };
    let coeff_y: &[i32] = if p.bilinear {
        &BILINEAR_PREC4[fy]
    } else if luma {
        luma_coeff(fy, p.alt_hpel, small)
    } else {
        &CHROMA_FILTER[chroma_y]
    };
    if fy == 0 {
        if fx == 0 {
            filter_copy(src, ox, oy, dst, dx, dy, w, h, true, last, p.bd, p.bilinear);
        } else {
            filter(
                src, ox, oy, dst, dx, dy, w, h, coeff_x, false, true, last, p.bd,
            );
        }
    } else if fx == 0 {
        filter(
            src, ox, oy, dst, dx, dy, w, h, coeff_y, true, true, last, p.bd,
        );
    } else {
        let taps = coeff_y.len();
        let back = taps / 2 - 1;
        let mut tmp = Buf::new(w, h + taps - 1);
        let (cx, cy) = if p.bilinear {
            (&BILINEAR_PREC4[fx][..], &BILINEAR_PREC4[fy][..])
        } else if luma {
            // the separable path never uses the 4x4 filter except for 4x4 blocks
            (
                luma_coeff(fx, p.alt_hpel, small),
                luma_coeff(fy, p.alt_hpel, small),
            )
        } else {
            (&CHROMA_FILTER[chroma_x][..], &CHROMA_FILTER[chroma_y][..])
        };
        filter(
            src,
            ox,
            oy - back,
            &mut tmp,
            0,
            0,
            w,
            h + taps - 1,
            cx,
            false,
            true,
            false,
            p.bd,
        );
        filter(
            &tmp, 0, back, dst, dx, dy, w, h, cy, true, false, last, p.bd,
        );
    }
}

/// Margins read around a block by the interpolation filters.
const MARGIN: i32 = 4;

/// Reference samples clamped first to `region` (x, y, w, h) and then to
/// the picture: vvdec's DMVR prefetch buffer with its edge padding.
fn fetch_region(
    plane: &Plane,
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    region: (i32, i32, i32, i32),
) -> Buf {
    let mut b = Buf::new(w, h);
    let (pw, ph) = (plane.width as i32, plane.height as i32);
    let (rx, ry, rw, rh) = region;
    for y in 0..h {
        let py = (y0 + y as i32).clamp(ry, ry + rh - 1).clamp(0, ph - 1) as usize;
        for x in 0..w {
            let px = (x0 + x as i32).clamp(rx, rx + rw - 1).clamp(0, pw - 1) as usize;
            b.data[y * w + x] = i32::from(plane.data[py * plane.stride + px]);
        }
    }
    b
}

/// vvdec's `xPredInterBlk` for a reference picture: predicts the block at
/// `(bx, by)` in component coordinates. With `bdof`, luma is returned in
/// the extended BDOF layout ((w + 2) x (h + 2)).
#[allow(clippy::too_many_arguments)]
pub fn pred_blk(
    refpic: &RefPic,
    bx: i32,
    by: i32,
    w: usize,
    h: usize,
    mv: Mv,
    p: &BlkParams,
    bdof: bool,
) -> Buf {
    pred_blk_region(refpic, bx, by, w, h, mv, p, bdof, None)
}

/// `pred_blk` reading through a DMVR padding region.
#[allow(clippy::too_many_arguments)]
pub fn pred_blk_region(
    refpic: &RefPic,
    bx: i32,
    by: i32,
    w: usize,
    h: usize,
    mv: Mv,
    p: &BlkParams,
    bdof: bool,
    region: Option<(i32, i32, i32, i32)>,
) -> Buf {
    let plane = &refpic.planes[p.comp];
    let shift_h = 4 + p.sx as i32;
    let shift_v = 4 + p.sy as i32;
    let fx = (mv.x & ((1 << shift_h) - 1)) as usize;
    let fy = (mv.y & ((1 << shift_v) - 1)) as usize;
    let x0 = bx + (mv.x >> shift_h);
    let y0 = by + (mv.y >> shift_v);
    let src = match region {
        Some(r) => fetch_region(
            plane,
            x0 - MARGIN,
            y0 - MARGIN,
            w + 2 * MARGIN as usize + 1,
            h + 2 * MARGIN as usize + 1,
            r,
        ),
        None => fetch(
            plane,
            x0 - MARGIN,
            y0 - MARGIN,
            w + 2 * MARGIN as usize + 1,
            h + 2 * MARGIN as usize + 1,
        ),
    };
    let (ox, oy) = (MARGIN as usize, MARGIN as usize);
    if bdof && p.comp == 0 {
        let mut ext = Buf::new(w + 2, h + 2);
        interp(&src, ox, oy, fx, fy, w, h, p, &mut ext, 1, 1);
        let shift = 2.max(IF_INTERNAL_PREC - p.bd as i32);
        let xo = usize::from(fx < 8);
        let yo = usize::from(fy < 8);
        let conv = |v: i32| pel(v * (1 << shift) - IF_INTERNAL_OFFS);
        for r in 0..h {
            let sy = oy + r + 1 - yo;
            ext.set(0, r + 1, conv(src.at(ox - xo, sy)));
            ext.set(w + 1, r + 1, conv(src.at(ox - xo + w + 1, sy)));
        }
        for c in 0..w + 2 {
            ext.set(c, 0, conv(src.at(ox - xo + c, oy - yo)));
            ext.set(c, h + 1, conv(src.at(ox - xo + c, oy + h + 1 - yo)));
        }
        return ext;
    }
    let mut dst = Buf::new(w, h);
    interp(&src, ox, oy, fx, fy, w, h, p, &mut dst, 0, 0);
    dst
}

/// `AreaBuf::addAvg`.
fn add_avg(s0: &Buf, s1: &Buf, dst: &mut Buf, bd: u32) {
    let shift = 2.max(IF_INTERNAL_PREC - bd as i32) + 1;
    let offset = (1 << (shift - 1)) + 2 * IF_INTERNAL_OFFS;
    let max = (1 << bd) - 1;
    for y in 0..dst.h {
        for x in 0..dst.w {
            let v = ((s0.at(x, y) + s1.at(x, y) + offset) >> shift).clamp(0, max);
            dst.set(x, y, v);
        }
    }
}

/// `AreaBuf::addWeightedAvg` with a BCW index.
fn add_weighted_avg(s0: &Buf, s1: &Buf, dst: &mut Buf, bd: u32, bcw: u8) {
    let w1 = BCW_WEIGHTS[BCW_INTERN_BCW[bcw as usize]];
    let w0 = 8 - w1;
    let shift = 2.max(IF_INTERNAL_PREC - bd as i32) + 3;
    let offset = (1 << (shift - 1)) + (IF_INTERNAL_OFFS << 3);
    let max = (1 << bd) - 1;
    for y in 0..dst.h {
        for x in 0..dst.w {
            let v = ((s0.at(x, y) * w0 + s1.at(x, y) * w1 + offset) >> shift).clamp(0, max);
            dst.set(x, y, v);
        }
    }
}

/// `WeightPrediction::addWeightUni` for one component.
fn weight_uni(src: &Buf, dst: &mut Buf, wp: &WpParam, bd: u32) {
    let shift_num = 2.max(IF_INTERNAL_PREC - bd as i32);
    let offset = wp.offset * (1 << (bd - 8));
    let shift = wp.log2_denom as i32 + shift_num;
    let max = (1 << bd) - 1;
    for y in 0..dst.h {
        for x in 0..dst.w {
            let p = src.at(x, y) + IF_INTERNAL_OFFS;
            let v = if wp.weight != 1 << wp.log2_denom {
                let round = if shift > 0 { 1 << (shift - 1) } else { 0 };
                ((wp.weight * p + round) >> shift) + offset
            } else {
                let round = if shift_num > 0 {
                    1 << (shift_num - 1)
                } else {
                    0
                };
                ((p + round) >> shift_num) + offset
            };
            dst.set(x, y, v.clamp(0, max));
        }
    }
}

/// `WeightPrediction::addWeightBi` for one component.
fn weight_bi(s0: &Buf, s1: &Buf, dst: &mut Buf, w0p: &WpParam, w1p: &WpParam, bd: u32) {
    let shift_num = 2.max(IF_INTERNAL_PREC - bd as i32);
    let scale = 1 << (bd - 8);
    let offset = w0p.offset * scale + w1p.offset * scale;
    let shift = w0p.log2_denom as i32 + 1 + shift_num;
    let round = (1 << shift) >> 1;
    let (w0, w1) = (w0p.weight, w1p.weight);
    let apply = round + offset * (1 << (shift - 1)) + (w0 + w1) * IF_INTERNAL_OFFS;
    let max = (1 << bd) - 1;
    for y in 0..dst.h {
        for x in 0..dst.w {
            let v = ((s0.at(x, y) * w0 + s1.at(x, y) * w1 + apply) >> shift).clamp(0, max);
            dst.set(x, y, v);
        }
    }
}

/// vvdec's `gradFilterCore<PAD>` over an extended block; returns the
/// gradients and pads the block's border (as vvdec does in place).
fn bdof_gradients(ext: &mut Buf, pad: bool) -> (Buf, Buf) {
    let (w, h) = (ext.w, ext.h);
    let mut gx = Buf::new(w, h);
    let mut gy = Buf::new(w, h);
    let (x0, y0, x1, y1) = if pad {
        (1, 1, w - 1, h - 1)
    } else {
        (0, 0, w, h)
    };
    for y in y0..y1 {
        for x in x0..x1 {
            gy.set(x, y, pel((ext.at(x, y + 1) >> 6) - (ext.at(x, y - 1) >> 6)));
            gx.set(x, y, pel((ext.at(x + 1, y) >> 6) - (ext.at(x - 1, y) >> 6)));
        }
    }
    if pad {
        for y in 1..h - 1 {
            for b in [&mut gx, &mut gy, &mut *ext] {
                let v = b.at(1, y);
                b.set(0, y, v);
                let v = b.at(w - 2, y);
                b.set(w - 1, y, v);
            }
        }
        for b in [&mut gx, &mut gy, &mut *ext] {
            for x in 0..w {
                let v = b.at(x, 1);
                b.set(x, 0, v);
                let v = b.at(x, h - 2);
                b.set(x, h - 1, v);
            }
        }
    }
    (gx, gy)
}

/// `rightShiftMSB`.
fn right_shift_msb(numer: i32, denom: i32) -> i32 {
    let mut msb = 0;
    while msb < 32 && denom >= (1i64 << msb) as i32 {
        msb += 1;
    }
    numer >> (msb - 1)
}

/// `InterPrediction::applyBiOptFlow` + `BiOptFlowCore` on the luma block.
fn bdof(e0: &mut Buf, e1: &mut Buf, dst: &mut Buf, bd: u32) {
    let (w, h) = (dst.w, dst.h);
    let (gx0, gy0) = bdof_gradients(e0, true);
    let (gx1, gy1) = bdof_gradients(e1, true);
    let shift = IF_INTERNAL_PREC + 1 - bd as i32;
    let offset = (1 << (shift - 1)) + 2 * IF_INTERNAL_OFFS;
    let limit = (1 << 4) - 1;
    let max = (1 << bd) - 1;
    for yu in 0..h >> 2 {
        for xu in 0..w >> 2 {
            let (bx, by) = (xu * 4, yu * 4);
            let (mut sgx, mut sgy, mut sdx, mut sdy, mut sgxgy) = (0i32, 0i32, 0i32, 0i32, 0i32);
            for y in by..by + 6 {
                for x in bx..bx + 6 {
                    let tgx = (gx0.at(x, y) + gx1.at(x, y)) >> 1;
                    let tgy = (gy0.at(x, y) + gy1.at(x, y)) >> 1;
                    let tdi = (e1.at(x, y) >> 4) - (e0.at(x, y) >> 4);
                    sgx += tgx.abs();
                    sgy += tgy.abs();
                    sdx += tgx.signum() * tdi;
                    sdy += tgy.signum() * tdi;
                    sgxgy += tgy.signum() * tgx;
                }
            }
            let mut tx = if sgx == 0 {
                0
            } else {
                right_shift_msb(sdx * 4, sgx)
            };
            tx = tx.clamp(-limit, limit);
            let main = sgxgy >> 12;
            let sec = sgxgy & ((1 << 12) - 1);
            let t = ((tx * main) * (1 << 12) + tx * sec) >> 1;
            let mut ty = if sgy == 0 {
                0
            } else {
                right_shift_msb(sdy * 4 - t, sgy)
            };
            ty = ty.clamp(-limit, limit);
            for y in 0..4 {
                for x in 0..4 {
                    let (ex, ey) = (bx + x + 1, by + y + 1);
                    let b = tx * (gx0.at(ex, ey) - gx1.at(ex, ey))
                        + ty * (gy0.at(ex, ey) - gy1.at(ex, ey));
                    let v = pel((e0.at(ex, ey) + e1.at(ex, ey) + b + offset) >> shift);
                    dst.set(bx + x, by + y, v.clamp(0, max));
                }
            }
        }
    }
}

/// The motion of a coding unit or a sub-block being predicted.
#[derive(Clone, Debug, Default)]
pub struct Mcu {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub inter_dir: u8,
    pub ref_idx: [i8; 2],
    pub mv: [[Mv; 3]; 2],
    pub affine: bool,
    pub affine_type: u8,
    pub merge: bool,
    pub merge_type: u8,
    pub mmvd: bool,
    pub ciip: bool,
    pub geo: bool,
    pub bcw: u8,
    pub imv: u8,
    pub smvd: u8,
}

impl Mcu {
    pub fn from_cu(c: &Cu) -> Self {
        Self {
            x: c.lx(),
            y: c.ly(),
            w: c.lw(),
            h: c.lh(),
            inter_dir: c.inter_dir,
            ref_idx: c.ref_idx,
            mv: c.mv,
            affine: c.affine,
            affine_type: c.affine_type,
            merge: c.merge,
            merge_type: c.merge_type,
            mmvd: c.mmvd,
            ciip: c.ciip,
            geo: c.geo,
            bcw: c.bcw,
            imv: c.imv,
            smvd: c.smvd,
        }
    }

    /// `CodingUnit::operator=( const MotionInfo& )`.
    fn set_motion(&mut self, mi: &MotionInfo) {
        self.inter_dir = mi.inter_dir();
        for l in 0..2 {
            self.ref_idx[l] = mi.ref_idx[l];
            self.mv[l][0] = mi.mv[l];
        }
    }
}

pub struct McCtx<'p, 's> {
    pub si: &'p SliceInfo<'s>,
    pub pic: &'p Picture,
    pub fmt: Format,
    pub bd: u32,
    /// Collected DMVR refinements (vvdec's `m_dmvrMvCache`).
    pub dmvr: Option<DmvrRefinement>,
    sub_pu: bool,
}

impl<'p, 's> McCtx<'p, 's> {
    pub fn new(pic: &'p Picture, si: &'p SliceInfo<'s>) -> Self {
        Self {
            si,
            pic,
            fmt: pic.fmt,
            bd: pic.bit_depth,
            dmvr: None,
            sub_pu: false,
        }
    }

    fn num_comp(&self) -> usize {
        self.fmt.num_comp()
    }

    fn blk(&self, u: &Mcu, comp: usize) -> (i32, i32, usize, usize) {
        let (sx, sy) = self.fmt.scale(comp);
        (
            u.x >> sx,
            u.y >> sy,
            (u.w >> sx) as usize,
            (u.h >> sy) as usize,
        )
    }

    fn new_pred(&self, u: &Mcu) -> PredUnit {
        let mut p: PredUnit = Default::default();
        for comp in 0..self.num_comp() {
            let (_, _, w, h) = self.blk(u, comp);
            p[comp] = Buf::new(w, h);
        }
        p
    }

    fn refpic(&self, l: usize, idx: i8) -> Result<&'p RefPic, Error> {
        self.si.inter.refs[l]
            .get(idx.max(0) as usize)
            .map(|r| &**r)
            .ok_or(Error::Invalid("xPredInterUni missing ref pic"))
    }

    fn wp(&self, l: usize, idx: i8) -> &'p [WpParam; 3] {
        &self.si.sh.wp[l][idx.max(0) as usize]
    }

    fn wp_present(&self, u: &Mcu) -> bool {
        let (a, b) = (self.wp(0, u.ref_idx[0]), self.wp(1, u.ref_idx[1]));
        (0..3).any(|k| a[k].present || b[k].present)
    }

    /// `PU::isBiPredFromDifferentDirEqDistPoc`.
    fn bi_diff_dir_eq_dist(&self, u: &Mcu) -> bool {
        if u.ref_idx[0] < 0 || u.ref_idx[1] < 0 {
            return false;
        }
        let info = &self.si.inter.info;
        let (r0, r1) = (u.ref_idx[0] as usize, u.ref_idx[1] as usize);
        if info.ref_lt[0][r0] || info.ref_lt[1][r1] {
            return false;
        }
        let poc = info.poc;
        poc - info.ref_poc[0][r0] == info.ref_poc[1][r1] - poc
    }

    /// `InterPrediction::xCheckIdenticalMotion`.
    fn identical_motion(&self, u: &Mcu) -> bool {
        if self.si.sh.slice_type != B_SLICE || self.si.pps.weighted_bipred {
            return false;
        }
        if u.ref_idx[0] < 0 || u.ref_idx[1] < 0 {
            return false;
        }
        let info = &self.si.inter.info;
        if info.ref_poc[0][u.ref_idx[0] as usize] != info.ref_poc[1][u.ref_idx[1] as usize] {
            return false;
        }
        if !u.affine {
            u.mv[0][0] == u.mv[1][0]
        } else {
            u.mv[0][0] == u.mv[1][0]
                && u.mv[0][1] == u.mv[1][1]
                && (u.affine_type == 0 || u.mv[0][2] == u.mv[1][2])
        }
    }

    fn params(&self, comp: usize, bi: bool, u: &Mcu) -> BlkParams {
        let (sx, sy) = self.fmt.scale(comp);
        BlkParams {
            comp,
            bi,
            alt_hpel: u.imv == IMV_HPEL,
            bilinear: false,
            bd: self.bd,
            sx,
            sy,
        }
    }

    /// `InterPrediction::xPredInterUni`; with `bio` luma is returned in the
    /// extended BDOF layout.
    fn pred_uni(&self, u: &Mcu, l: usize, bi: bool, bio: bool) -> Result<PredUnit, Error> {
        let refpic = self.refpic(l, u.ref_idx[l])?;
        let mut out: PredUnit = Default::default();
        for comp in 0..self.num_comp() {
            if u.affine {
                out[comp] = self.pred_affine(u, l, comp, refpic, bi)?;
            } else {
                let (bx, by, w, h) = self.blk(u, comp);
                let p = self.params(comp, bi, u);
                out[comp] = pred_blk(refpic, bx, by, w, h, u.mv[l][0], &p, bio);
            }
        }
        Ok(out)
    }

    /// `xWeightedAverage` (BCW or plain average; BDOF on luma).
    fn weighted_average(
        &self,
        u: &Mcu,
        p0: &mut PredUnit,
        p1: &mut PredUnit,
        bio: bool,
    ) -> PredUnit {
        let mut dst = self.new_pred(u);
        if u.bcw != 0 && !u.ciip {
            for comp in 0..self.num_comp() {
                add_weighted_avg(&p0[comp], &p1[comp], &mut dst[comp], self.bd, u.bcw);
            }
            return dst;
        }
        let start = if bio {
            bdof(&mut p0[0], &mut p1[0], &mut dst[0], self.bd);
            1
        } else {
            0
        };
        for comp in start..self.num_comp() {
            add_avg(&p0[comp], &p1[comp], &mut dst[comp], self.bd);
        }
        dst
    }

    /// `InterPrediction::xPredInterBi`.
    fn pred_bi(&self, u: &Mcu) -> Result<PredUnit, Error> {
        let slice_type = self.si.sh.slice_type;
        let pps = self.si.pps;
        let is_bi = u.ref_idx[0] >= 0 && u.ref_idx[1] >= 0;
        let wp_uni = (pps.weighted_pred && slice_type == P_SLICE)
            || (pps.weighted_bipred && slice_type == B_SLICE);
        if is_bi {
            let mut p0 = self.pred_uni(u, 0, true, false)?;
            let mut p1 = self.pred_uni(u, 1, true, false)?;
            if pps.weighted_bipred && slice_type == B_SLICE && u.bcw == 0 {
                let mut dst = self.new_pred(u);
                let (w0, w1) = (self.wp(0, u.ref_idx[0]), self.wp(1, u.ref_idx[1]));
                for comp in 0..self.num_comp() {
                    weight_bi(
                        &p0[comp],
                        &p1[comp],
                        &mut dst[comp],
                        &w0[comp],
                        &w1[comp],
                        self.bd,
                    );
                }
                return Ok(dst);
            }
            if pps.weighted_pred && slice_type == P_SLICE {
                let mut dst = self.new_pred(u);
                let w0 = self.wp(0, u.ref_idx[0]);
                for comp in 0..self.num_comp() {
                    weight_uni(&p0[comp], &mut dst[comp], &w0[comp], self.bd);
                }
                return Ok(dst);
            }
            return Ok(self.weighted_average(u, &mut p0, &mut p1, false));
        }
        let l = if u.ref_idx[0] >= 0 { 0 } else { 1 };
        if !u.geo && wp_uni {
            let p0 = self.pred_uni(u, l, true, false)?;
            let mut dst = self.new_pred(u);
            let w = if pps.weighted_bipred && slice_type == B_SLICE {
                self.wp(l, u.ref_idx[l])
            } else {
                self.wp(0, u.ref_idx[0])
            };
            for comp in 0..self.num_comp() {
                weight_uni(&p0[comp], &mut dst[comp], &w[comp], self.bd);
            }
            return Ok(dst);
        }
        self.pred_uni(u, l, u.geo, false)
    }

    /// `InterPrediction::xSubPuBio`.
    fn sub_pu_bio(&mut self, u: &Mcu) -> Result<PredUnit, Error> {
        let mut dst = self.new_pred(u);
        let sh = u.h.min(16);
        let sw = u.w.min(16);
        let mut y = u.y;
        while y < u.y + u.h {
            let mut x = u.x;
            while x < u.x + u.w {
                let mut s = u.clone();
                s.x = x;
                s.y = y;
                s.w = sw;
                s.h = sh;
                let mi = self.pic.mi(x, y);
                s.set_motion(&mi);
                let mut p0 = self.pred_uni(&s, 0, true, true)?;
                let mut p1 = self.pred_uni(&s, 1, true, true)?;
                let r = self.weighted_average(&s, &mut p0, &mut p1, true);
                self.copy_into(&mut dst, u, &r, &s);
                x += sw;
            }
            y += sh;
        }
        Ok(dst)
    }

    fn copy_into(&self, dst: &mut PredUnit, u: &Mcu, src: &PredUnit, s: &Mcu) {
        for comp in 0..self.num_comp() {
            let (sx, sy) = self.fmt.scale(comp);
            let (ox, oy) = (((s.x - u.x) >> sx) as usize, ((s.y - u.y) >> sy) as usize);
            let b = &src[comp];
            for yy in 0..b.h {
                for xx in 0..b.w {
                    dst[comp].set(ox + xx, oy + yy, b.at(xx, yy));
                }
            }
        }
    }

    /// `InterPrediction::xSubPuMC`: sub-blocks with equal motion are joined.
    fn sub_pu_mc(&mut self, u: &Mcu) -> Result<PredUnit, Error> {
        let mut dst = self.new_pred(u);
        let part_line = (u.w >> 3).max(1);
        let part_col = (u.h >> 3).max(1);
        let pu_h = if part_col == 1 { u.h } else { 8 };
        let pu_w = if part_line == 1 { u.w } else { 8 };
        let ver = u.h > u.w;
        let (fst_start, sec_start) = if !ver { (u.y, u.x) } else { (u.x, u.y) };
        let (fst_end, sec_end) = if !ver {
            (u.y + u.h, u.x + u.w)
        } else {
            (u.x + u.w, u.y + u.h)
        };
        let (fst_step, sec_step) = if !ver { (pu_h, pu_w) } else { (pu_w, pu_h) };
        let mut base = u.clone();
        base.merge_type = MRG_TYPE_DEFAULT_N;
        base.affine = false;
        base.geo = false;
        base.merge = false;
        base.mmvd = false;
        base.ciip = false;
        self.sub_pu = true;
        let mut fst = fst_start;
        while fst < fst_end {
            let mut sec = sec_start;
            while sec < sec_end {
                let (mut x, mut y) = if !ver { (sec, fst) } else { (fst, sec) };
                let cur = self.pic.mi(x, y);
                let mut length = sec_step;
                let mut later = sec + sec_step;
                while later < sec_end {
                    let lm = if !ver {
                        self.pic.mi(later, fst)
                    } else {
                        self.pic.mi(fst, later)
                    };
                    if lm == cur {
                        length += sec_step;
                    } else {
                        break;
                    }
                    later += sec_step;
                }
                let mut dx = if !ver { length } else { pu_w };
                let mut dy = if !ver { pu_h } else { length };
                let mut s = base.clone();
                s.set_motion(&cur);
                if !ver && (dx & 15) != 0 && dx > 16 {
                    let part = dx & !15;
                    (s.x, s.y, s.w, s.h) = (x, y, part, dy);
                    let r = self.motion_compensation(&s)?;
                    self.copy_into(&mut dst, u, &r, &s);
                    x += part;
                    dx -= part;
                } else if ver && (dy & 15) != 0 && dy > 16 {
                    let part = dy & !15;
                    (s.x, s.y, s.w, s.h) = (x, y, dx, part);
                    let r = self.motion_compensation(&s)?;
                    self.copy_into(&mut dst, u, &r, &s);
                    y += part;
                    dy -= part;
                }
                (s.x, s.y, s.w, s.h) = (x, y, dx, dy);
                let r = self.motion_compensation(&s)?;
                self.copy_into(&mut dst, u, &r, &s);
                sec = later;
            }
            fst += fst_step;
        }
        self.sub_pu = false;
        Ok(dst)
    }

    /// `PU::checkDMVRCondition`.
    fn dmvr_condition(&self, u: &Mcu) -> bool {
        let (sps, ph) = (self.si.sps, self.si.ph);
        sps.dmvr
            && !ph.dis_dmvr
            && u.merge
            && u.merge_type == MRG_TYPE_DEFAULT_N
            && !u.ciip
            && !u.affine
            && !u.mmvd
            && self.bi_diff_dir_eq_dist(u)
            && u.h >= 8
            && u.w >= 8
            && u.w * u.h >= 128
            && u.bcw == 0
            && !self.wp_present(u)
    }

    /// `InterPrediction::motionCompensation` for inter (non-IBC) units.
    pub fn motion_compensation(&mut self, u: &Mcu) -> Result<PredUnit, Error> {
        let (sps, ph, pps) = (self.si.sps, self.si.ph, self.si.pps);
        let slice_type = self.si.sh.slice_type;
        let mut bio = false;
        if sps.bdof && !ph.dis_bdof {
            if u.affine || self.sub_pu || u.ciip || u.smvd != 0 || (sps.bcw && u.bcw != 0) {
                bio = false;
            } else {
                let check0 = !(self.wp_present(u) && slice_type == B_SLICE);
                let check1 = !(pps.weighted_pred && slice_type == P_SLICE);
                bio = check0
                    && check1
                    && self.bi_diff_dir_eq_dist(u)
                    && u.h >= 8
                    && u.w >= 8
                    && u.w * u.h >= 128;
            }
        }
        let dmvr = !self.sub_pu && self.dmvr_condition(u);
        if u.merge_type != MRG_TYPE_SUBPU_ATMVP && bio && !dmvr {
            self.sub_pu_bio(u)
        } else if dmvr {
            self.process_dmvr(u, bio)
        } else if u.merge_type == MRG_TYPE_SUBPU_ATMVP {
            self.sub_pu_mc(u)
        } else if self.identical_motion(u) {
            self.pred_uni(u, 0, false, false)
        } else {
            self.pred_bi(u)
        }
    }

    /// `InterPrediction::motionCompensationGeo` with `weightedGeoBlk`.
    pub fn motion_compensation_geo(
        &mut self,
        u: &Mcu,
        geo_dir_ref: [u8; 2],
        split: u8,
        mvs: [Mv; 2],
    ) -> Result<PredUnit, Error> {
        let mut preds = Vec::new();
        for k in 0..2 {
            let d = geo_dir_ref[k] >> 4;
            let r = (geo_dir_ref[k] & 15) as i8;
            let mut s = u.clone();
            s.mv[0][0] = if d == 1 { mvs[k] } else { Mv::default() };
            s.mv[1][0] = if d == 1 { Mv::default() } else { mvs[k] };
            s.ref_idx = if d == 1 { [r, -1] } else { [-1, r] };
            preds.push(self.motion_compensation(&s)?);
        }
        let mut dst = self.new_pred(u);
        let g = geo();
        let (angle, _) = g.params[split as usize];
        let w_idx = (31 - (u.w as u32).leading_zeros()) as usize - 3;
        let h_idx = (31 - (u.h as u32).leading_zeros()) as usize - 3;
        let (off_x, off_y) = g.offsets[split as usize][h_idx][w_idx];
        let mask = &g.weights[geo_mask(angle)];
        let size = GEO_WEIGHT_MASK_SIZE;
        let shift = 2.max(IF_INTERNAL_PREC - self.bd as i32) + 3;
        let offset = (1 << (shift - 1)) + (IF_INTERNAL_OFFS << 3);
        let max = (1 << self.bd) - 1;
        for comp in 0..self.num_comp() {
            let (sx, sy) = self.fmt.scale(comp);
            let (_, _, w, h) = self.blk(u, comp);
            let mirror = GEO_ANGLE2MIRROR[angle as usize];
            let step_x_base = 1i32 << sx;
            for y in 0..h {
                for x in 0..w {
                    let (wx, wy) = ((x as i32) * step_x_base, (y as i32) << sy);
                    let widx = match mirror {
                        2 => (size - 1 - off_y - wy) * size + off_x + wx,
                        1 => (off_y + wy) * size + (size - 1 - off_x) - wx,
                        _ => (off_y + wy) * size + off_x + wx,
                    };
                    let wt = i32::from(mask[widx as usize]);
                    let v = ((wt * preds[0][comp].at(x, y)
                        + (8 - wt) * preds[1][comp].at(x, y)
                        + offset)
                        >> shift)
                        .clamp(0, max);
                    dst[comp].set(x, y, v);
                }
            }
        }
        Ok(dst)
    }
}

/// DMVR SAD over every second row (vvdec's `xGetSAD8/16` with subShift 1).
fn dmvr_sad(
    a: &Buf,
    ax: usize,
    ay: usize,
    b: &Buf,
    bx: usize,
    by: usize,
    w: usize,
    h: usize,
) -> u64 {
    let mut sum = 0u64;
    let mut y = 0;
    while y < h {
        for x in 0..w {
            sum += (a.at(ax + x, ay + y) - b.at(bx + x, by + y)).unsigned_abs() as u64;
        }
        y += 2;
    }
    sum << 1
}

/// `div_for_maxq7`.
fn div_for_maxq7(mut n: i64, mut d: i64) -> i32 {
    let sign = n < 0;
    if sign {
        n = -n;
    }
    let mut q = 0i32;
    d <<= 3;
    if n >= d {
        n -= d;
        q += 1;
    }
    q <<= 1;
    d >>= 1;
    if n >= d {
        n -= d;
        q += 1;
    }
    q <<= 1;
    if n >= (d >> 1) {
        q += 1;
    }
    if sign { -q } else { q }
}

/// `xSubPelErrorSrfc`.
fn sub_pel_error_surface(sad: [u64; 5], delta: &mut [i32; 2]) {
    let num = (sad[1] as i64 - sad[3] as i64) * 16;
    let den = sad[1] as i64 + sad[3] as i64 - ((sad[0] as i64) << 1);
    if den != 0 {
        if sad[1] != sad[0] && sad[3] != sad[0] {
            delta[0] = div_for_maxq7(num, den);
        } else {
            delta[0] = if sad[1] == sad[0] { -8 } else { 8 };
        }
    }
    let num = (sad[2] as i64 - sad[4] as i64) * 16;
    let den = sad[2] as i64 + sad[4] as i64 - ((sad[0] as i64) << 1);
    if den != 0 {
        if sad[2] != sad[0] && sad[4] != sad[0] {
            delta[1] = div_for_maxq7(num, den);
        } else {
            delta[1] = if sad[2] == sad[0] { -8 } else { 8 };
        }
    }
}

impl<'p, 's> McCtx<'p, 's> {
    /// `InterPrediction::xProcessDMVR`.
    fn process_dmvr(&mut self, u: &Mcu, bio: bool) -> Result<PredUnit, Error> {
        let merge = [u.mv[0][0], u.mv[1][0]];
        let (ext_w, ext_h) = ((u.w + 4) as usize, (u.h + 4) as usize);
        let mut bl: Vec<Buf> = Vec::new();
        for l in 0..2 {
            let refpic = self.refpic(l, u.ref_idx[l])?;
            let mv = merge[l].sub(Mv::new(2 << 4, 2 << 4));
            let p = BlkParams {
                comp: 0,
                bi: true,
                alt_hpel: u.imv == IMV_HPEL,
                bilinear: true,
                bd: self.bd,
                sx: 0,
                sy: 0,
            };
            bl.push(pred_blk(refpic, u.x, u.y, ext_w, ext_h, mv, &p, false));
        }
        let dy = u.h.min(16);
        let dx = u.w.min(16);
        let bio_thres = (2 * dy * dx) as u64;
        let mut dst = self.new_pred(u);
        let mut deltas = Vec::new();
        let mut ys = 0;
        while ys < u.h {
            let mut xs = 0;
            while xs < u.w {
                let mut sub = u.clone();
                sub.x = u.x + xs;
                sub.y = u.y + ys;
                sub.w = dx;
                sub.h = dy;
                let (cx, cy) = ((2 + xs) as usize, (2 + ys) as usize);
                let (uw, uh) = (dx as usize, dy as usize);
                let mut min_cost = dmvr_sad(&bl[0], cx, cy, &bl[1], cx, cy, uw, uh);
                min_cost >>= 1;
                min_cost -= min_cost >> 2;
                let mut delta = Mv::default();
                if min_cost >= (dx * dy) as u64 {
                    let mut sads = [0u64; 25];
                    sads[12] = min_cost;
                    let mut dmv = (0i32, 0i32);
                    for ver in -2i32..=2 {
                        for hor in -2i32..=2 {
                            let k = ((ver + 2) * 5 + hor + 2) as usize;
                            let cost = if ver == 0 && hor == 0 {
                                sads[12]
                            } else {
                                let a = ((cx as i32 + hor) as usize, (cy as i32 + ver) as usize);
                                let b = ((cx as i32 - hor) as usize, (cy as i32 - ver) as usize);
                                dmvr_sad(&bl[0], a.0, a.1, &bl[1], b.0, b.1, uw, uh) >> 1
                            };
                            sads[k] = cost;
                            if cost < min_cost {
                                min_cost = cost;
                                dmv = (hor, ver);
                            }
                        }
                    }
                    let mut total = [dmv.0 * 16, dmv.1 * 16];
                    if total[0].abs() != 32 && total[1].abs() != 32 {
                        let c = (12 + dmv.1 * 5 + dmv.0) as usize;
                        let mut t = [0i32; 2];
                        sub_pel_error_surface(
                            [sads[c], sads[c - 1], sads[c - 5], sads[c + 1], sads[c + 5]],
                            &mut t,
                        );
                        total[0] += t[0];
                        total[1] += t[1];
                    }
                    delta = Mv::new(total[0], total[1]);
                    sub.mv[0][0] = merge[0].add(delta).clip_storage();
                    sub.mv[1][0] = merge[1].sub(delta).clip_storage();
                }
                deltas.push(delta);
                let bio_sub = if min_cost < bio_thres { false } else { bio };
                let mut p = [
                    self.dmvr_final_mc(&sub, 0, merge[0], bio_sub)?,
                    self.dmvr_final_mc(&sub, 1, merge[1], bio_sub)?,
                ];
                let [ref mut p0, ref mut p1] = p;
                let r = self.weighted_average(&sub, p0, p1, bio_sub);
                self.copy_into(&mut dst, u, &r, &sub);
                xs += dx;
            }
            ys += dy;
        }
        self.dmvr = Some(DmvrRefinement {
            x: u.x,
            y: u.y,
            w: u.w,
            h: u.h,
            mv: merge,
            deltas,
        });
        Ok(dst)
    }

    /// `xFinalPaddedMCForDMVR` for one list: blocks whose integer position
    /// moved read through the padded prefetch region around the merge
    /// vector (`xPrefetchPad`).
    fn dmvr_final_mc(&self, sub: &Mcu, l: usize, start: Mv, bio: bool) -> Result<PredUnit, Error> {
        let refpic = self.refpic(l, sub.ref_idx[l])?;
        let mv = sub.mv[l][0];
        let mut out: PredUnit = Default::default();
        for comp in 0..self.num_comp() {
            let (sx, sy) = self.fmt.scale(comp);
            let (bx, by, w, h) = self.blk(sub, comp);
            let (shx, shy) = (4 + sx as i32, 4 + sy as i32);
            let moved = (mv.x >> shx) != (start.x >> shx) || (mv.y >> shy) != (start.y >> shy);
            let p = BlkParams {
                comp,
                bi: true,
                alt_hpel: sub.imv == IMV_HPEL,
                bilinear: false,
                bd: self.bd,
                sx,
                sy,
            };
            let region = if moved {
                let taps = if comp == 0 { 3 } else { 1 };
                let ox = bx + ((start.x - (taps << shx)) >> shx);
                let oy = by + ((start.y - (taps << shy)) >> shy);
                let ext = 2 * taps + 1;
                Some((ox, oy, w as i32 + ext, h as i32 + ext))
            } else {
                None
            };
            out[comp] = pred_blk_region(refpic, bx, by, w, h, mv, &p, bio, region);
        }
        Ok(out)
    }

    /// `InterPrediction::xPredAffineBlk` for one component.
    fn pred_affine(
        &self,
        u: &Mcu,
        l: usize,
        comp: usize,
        refpic: &RefPic,
        bi: bool,
    ) -> Result<Buf, Error> {
        let (sx, sy) = self.fmt.scale(comp);
        let (w, h) = (u.w, u.h);
        let (cw, ch) = ((w >> sx) as usize, (h >> sy) as usize);
        let [lt, rt, lb] = u.mv[l];
        let log2 = |v: i32| 31 - (v as u32).leading_zeros() as i32;
        let hx = (rt.x - lt.x) * (1 << (7 - log2(w)));
        let hy = (rt.y - lt.y) * (1 << (7 - log2(w)));
        let (vx, vy) = if u.affine_type == 1 {
            (
                (lb.x - lt.x) * (1 << (7 - log2(h))),
                (lb.y - lt.y) * (1 << (7 - log2(h))),
            )
        } else {
            (-hy, hx)
        };
        let spread = subblock_spread_over_limit(hx, hy, vx, vy, u.inter_dir);
        let sps = self.si.sps;
        let prof = sps.prof
            && comp == 0
            && !self.si.ph.dis_prof
            && !((u.affine_type == 1 && lt == rt && lt == lb) || (u.affine_type == 0 && lt == rt))
            && !spread;
        let mut dmv_h = [0i32; 16];
        let mut dmv_v = [0i32; 16];
        if prof {
            let (qhx, qhy, qvx, qvy) = (hx * 4, hy * 4, vx * 4, vy * 4);
            dmv_h[0] = ((hx + vx) * 2) - ((qhx + qvx) * 2);
            dmv_v[0] = ((hy + vy) * 2) - ((qhy + qvy) * 2);
            for x in 1..4 {
                dmv_h[x] = dmv_h[x - 1] + qhx;
                dmv_v[x] = dmv_v[x - 1] + qhy;
            }
            for y in 1..4 {
                for x in 0..4 {
                    dmv_h[y * 4 + x] = dmv_h[(y - 1) * 4 + x] + qvx;
                    dmv_v[y * 4 + x] = dmv_v[(y - 1) * 4 + x] + qvy;
                }
            }
            for i in 0..16 {
                let (a, b) = round_affine(dmv_h[i], dmv_v[i], 8);
                dmv_h[i] = a.clamp(-31, 31);
                dmv_v[i] = b.clamp(-31, 31);
            }
        }
        let is_last_bi = if prof { true } else { bi };
        let pps = self.si.pps;
        let ctu = 1i32 << sps.log2_ctu_size;
        let hor_max = (pps.width as i32 + 8 - u.x - 1) * 16;
        let hor_min = (-ctu - 8 - u.x + 1) * 16;
        let ver_max = (pps.height as i32 + 8 - u.y - 1) * 16;
        let ver_min = (-ctu - 8 - u.y + 1) * 16;
        let (pbx, pby) = (u.x >> sx, u.y >> sy);
        let mut dst = Buf::new(cw, ch);
        let p = BlkParams {
            comp,
            bi: is_last_bi,
            alt_hpel: false,
            bilinear: false,
            bd: self.bd,
            sx,
            sy,
        };
        let shift = 2.max(IF_INTERNAL_PREC - self.bd as i32);
        let max = (1 << self.bd) - 1;
        let fmt = self.fmt;
        let mut yb = 0;
        while yb < ch {
            let mut xb = 0;
            while xb < cw {
                let mv = if comp == 0 || fmt.chroma == 3 {
                    self.pic.mi(u.x + xb as i32, u.y + yb as i32).mv[l]
                } else {
                    // chroma: accumulated luma subblock vectors
                    let (mx, my) = ((xb >> 2) as i32, (yb >> 2) as i32);
                    let mut sum = Mv::default();
                    let (lx0, ly0) = (mx << sx, my << sy);
                    for dyy in 0..(1 << sy) {
                        for dxx in 0..(1 << sx) {
                            let (px, py) = (lx0 + dxx, ly0 + dyy);
                            if fmt.chroma == 1 && ((px ^ py) & 1) != 0 {
                                continue;
                            }
                            let m = self.pic.mi(u.x + px * 4, u.y + py * 4).mv[l];
                            sum = sum.add(m);
                        }
                    }
                    let f = 1 << (1 - (sx | sy) as i32);
                    let (a, b) = round_affine(sum.x * f, sum.y * f, 1);
                    Mv::new(a, b)
                };
                let mv = Mv::new(mv.x.clamp(hor_min, hor_max), mv.y.clamp(ver_min, ver_max));
                let (shx, shy) = (4 + sx as i32, 4 + sy as i32);
                let fx = (mv.x & ((1 << shx) - 1)) as usize;
                let fy = (mv.y & ((1 << shy) - 1)) as usize;
                let x0 = pbx + xb as i32 + (mv.x >> shx);
                let y0 = pby + yb as i32 + (mv.y >> shy);
                let src = fetch(
                    &refpic.planes[comp],
                    x0 - MARGIN,
                    y0 - MARGIN,
                    4 + 2 * MARGIN as usize + 1,
                    4 + 2 * MARGIN as usize + 1,
                );
                let (ox, oy) = (MARGIN as usize, MARGIN as usize);
                if prof {
                    let mut ext = Buf::new(6, 6);
                    interp(&src, ox, oy, fx, fy, 4, 4, &p, &mut ext, 1, 1);
                    let xo = fx >> 3;
                    let yo = fy >> 3;
                    let conv = |v: i32| pel(v * (1 << shift) - IF_INTERNAL_OFFS);
                    for c in 0..6 {
                        ext.set(c, 0, conv(src.at(ox + xo + c - 1, oy + yo - 1)));
                        ext.set(c, 5, conv(src.at(ox + xo + c - 1, oy + yo + 4)));
                    }
                    for r in 0..4 {
                        ext.set(0, r + 1, conv(src.at(ox + xo - 1, oy + yo + r)));
                        ext.set(5, r + 1, conv(src.at(ox + xo + 4, oy + yo + r)));
                    }
                    let lim = 1 << (self.bd as i32 + 1).max(13);
                    let offset = (1 << (shift - 1)) + IF_INTERNAL_OFFS;
                    for r in 0..4 {
                        for c in 0..4 {
                            let (ex, ey) = (c + 1, r + 1);
                            let gx = pel((ext.at(ex + 1, ey) >> 6) - (ext.at(ex - 1, ey) >> 6));
                            let gy = pel((ext.at(ex, ey + 1) >> 6) - (ext.at(ex, ey - 1) >> 6));
                            let idx = r * 4 + c;
                            let di = (dmv_h[idx] * gx + dmv_v[idx] * gy).clamp(-lim, lim - 1);
                            let mut v = pel(ext.at(ex, ey) + di);
                            if !bi {
                                v = ((v + offset) >> shift).clamp(0, max);
                            }
                            dst.set(xb + c, yb + r, v);
                        }
                    }
                } else {
                    interp(&src, ox, oy, fx, fy, 4, 4, &p, &mut dst, xb, yb);
                }
                xb += 4;
            }
            yb += 4;
        }
        Ok(dst)
    }
}
