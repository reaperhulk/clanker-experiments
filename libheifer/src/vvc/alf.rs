// SPDX-License-Identifier: LGPL-3.0-or-later
//! Adaptive loop filter and cross-component ALF (H.266 clause 8.8.5),
//! following vvdec's `AdaptiveLoopFilter`.
use super::pic::{Picture, Plane};
use super::ps::{AlfParam, PicHeader, Pps, SliceHeader, Sps};

const FIXED_COEFF: [[i16; 13]; 64] = [
    [0, 0, 2, -3, 1, -4, 1, 7, -1, 1, -1, 5, 0],
    [0, 0, 0, 0, 0, -1, 0, 1, 0, 0, -1, 2, 0],
    [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1, 1, 0],
    [2, 2, -7, -3, 0, -5, 13, 22, 12, -3, -3, 17, 0],
    [-1, 0, 6, -8, 1, -5, 1, 23, 0, 2, -5, 10, 0],
    [0, 0, -1, -1, 0, -1, 2, 1, 0, 0, -1, 4, 0],
    [0, 0, 3, -11, 1, 0, -1, 35, 5, 2, -9, 9, 0],
    [0, 0, 8, -8, -2, -7, 4, 4, 2, 1, -1, 25, 0],
    [0, 0, 1, -1, 0, -3, 1, 3, -1, 1, -1, 3, 0],
    [0, 0, 3, -3, 0, -6, 5, -1, 2, 1, -4, 21, 0],
    [-7, 1, 5, 4, -3, 5, 11, 13, 12, -8, 11, 12, 0],
    [-5, -3, 6, -2, -3, 8, 14, 15, 2, -7, 11, 16, 0],
    [2, -1, -6, -5, -2, -2, 20, 14, -4, 0, -3, 25, 0],
    [3, 1, -8, -4, 0, -8, 22, 5, -3, 2, -10, 29, 0],
    [2, 1, -7, -1, 2, -11, 23, -5, 0, 2, -10, 29, 0],
    [-6, -3, 8, 9, -4, 8, 9, 7, 14, -2, 8, 9, 0],
    [2, 1, -4, -7, 0, -8, 17, 22, 1, -1, -4, 23, 0],
    [3, 0, -5, -7, 0, -7, 15, 18, -5, 0, -5, 27, 0],
    [2, 0, 0, -7, 1, -10, 13, 13, -4, 2, -7, 24, 0],
    [3, 3, -13, 4, -2, -5, 9, 21, 25, -2, -3, 12, 0],
    [-5, -2, 7, -3, -7, 9, 8, 9, 16, -2, 15, 12, 0],
    [0, -1, 0, -7, -5, 4, 11, 11, 8, -6, 12, 21, 0],
    [3, -2, -3, -8, -4, -1, 16, 15, -2, -3, 3, 26, 0],
    [2, 1, -5, -4, -1, -8, 16, 4, -2, 1, -7, 33, 0],
    [2, 1, -4, -2, 1, -10, 17, -2, 0, 2, -11, 33, 0],
    [1, -2, 7, -15, -16, 10, 8, 8, 20, 11, 14, 11, 0],
    [2, 2, 3, -13, -13, 4, 8, 12, 2, -3, 16, 24, 0],
    [1, 4, 0, -7, -8, -4, 9, 9, -2, -2, 8, 29, 0],
    [1, 1, 2, -4, -1, -6, 6, 3, -1, -1, -3, 30, 0],
    [-7, 3, 2, 10, -2, 3, 7, 11, 19, -7, 8, 10, 0],
    [0, -2, -5, -3, -2, 4, 20, 15, -1, -3, -1, 22, 0],
    [3, -1, -8, -4, -1, -4, 22, 8, -4, 2, -8, 28, 0],
    [0, 3, -14, 3, 0, 1, 19, 17, 8, -3, -7, 20, 0],
    [0, 2, -1, -8, 3, -6, 5, 21, 1, 1, -9, 13, 0],
    [-4, -2, 8, 20, -2, 2, 3, 5, 21, 4, 6, 1, 0],
    [2, -2, -3, -9, -4, 2, 14, 16, 3, -6, 8, 24, 0],
    [2, 1, 5, -16, -7, 2, 3, 11, 15, -3, 11, 22, 0],
    [1, 2, 3, -11, -2, -5, 4, 8, 9, -3, -2, 26, 0],
    [0, -1, 10, -9, -1, -8, 2, 3, 4, 0, 0, 29, 0],
    [1, 2, 0, -5, 1, -9, 9, 3, 0, 1, -7, 20, 0],
    [-2, 8, -6, -4, 3, -9, -8, 45, 14, 2, -13, 7, 0],
    [1, -1, 16, -19, -8, -4, -3, 2, 19, 0, 4, 30, 0],
    [1, 1, -3, 0, 2, -11, 15, -5, 1, 2, -9, 24, 0],
    [0, 1, -2, 0, 1, -4, 4, 0, 0, 1, -4, 7, 0],
    [0, 1, 2, -5, 1, -6, 4, 10, -2, 1, -4, 10, 0],
    [3, 0, -3, -6, -2, -6, 14, 8, -1, -1, -3, 31, 0],
    [0, 1, 0, -2, 1, -6, 5, 1, 0, 1, -5, 13, 0],
    [3, 1, 9, -19, -21, 9, 7, 6, 13, 5, 15, 21, 0],
    [2, 4, 3, -12, -13, 1, 7, 8, 3, 0, 12, 26, 0],
    [3, 1, -8, -2, 0, -6, 18, 2, -2, 3, -10, 23, 0],
    [1, 1, -4, -1, 1, -5, 8, 1, -1, 2, -5, 10, 0],
    [0, 1, -1, 0, 0, -2, 2, 0, 0, 1, -2, 3, 0],
    [1, 1, -2, -7, 1, -7, 14, 18, 0, 0, -7, 21, 0],
    [0, 1, 0, -2, 0, -7, 8, 1, -2, 0, -3, 24, 0],
    [0, 1, 1, -2, 2, -10, 10, 0, -2, 1, -7, 23, 0],
    [0, 2, 2, -11, 2, -4, -3, 39, 7, 1, -10, 9, 0],
    [1, 0, 13, -16, -5, -6, -1, 8, 6, 0, 6, 29, 0],
    [1, 3, 1, -6, -4, -7, 9, 6, -3, -2, 3, 33, 0],
    [4, 0, -17, -1, -1, 5, 26, 8, -2, 3, -15, 30, 0],
    [0, 1, -2, 0, 2, -8, 12, -6, 1, 1, -6, 16, 0],
    [0, 0, 0, -1, 1, -4, 4, 0, 0, 0, -3, 11, 0],
    [0, 1, 2, -8, 2, -6, 5, 15, 0, 2, -7, 9, 0],
    [1, -1, 12, -15, -7, -2, 3, 6, 6, -1, 7, 30, 0],
];

const CLASS_TO_FILTER: [[u8; 25]; 16] = [
    [8, 2, 2, 2, 3, 4, 53, 9, 9, 52, 4, 4, 5, 9, 2, 8, 10, 9, 1, 3, 39, 39, 10, 9, 52],
    [11, 12, 13, 14, 15, 30, 11, 17, 18, 19, 16, 20, 20, 4, 53, 21, 22, 23, 14, 25, 26, 26, 27, 28, 10],
    [16, 12, 31, 32, 14, 16, 30, 33, 53, 34, 35, 16, 20, 4, 7, 16, 21, 36, 18, 19, 21, 26, 37, 38, 39],
    [35, 11, 13, 14, 43, 35, 16, 4, 34, 62, 35, 35, 30, 56, 7, 35, 21, 38, 24, 40, 16, 21, 48, 57, 39],
    [11, 31, 32, 43, 44, 16, 4, 17, 34, 45, 30, 20, 20, 7, 5, 21, 22, 46, 40, 47, 26, 48, 63, 58, 10],
    [12, 13, 50, 51, 52, 11, 17, 53, 45, 9, 30, 4, 53, 19, 0, 22, 23, 25, 43, 44, 37, 27, 28, 10, 55],
    [30, 33, 62, 51, 44, 20, 41, 56, 34, 45, 20, 41, 41, 56, 5, 30, 56, 38, 40, 47, 11, 37, 42, 57, 8],
    [35, 11, 23, 32, 14, 35, 20, 4, 17, 18, 21, 20, 20, 20, 4, 16, 21, 36, 46, 25, 41, 26, 48, 49, 58],
    [12, 31, 59, 59, 3, 33, 33, 59, 59, 52, 4, 33, 17, 59, 55, 22, 36, 59, 59, 60, 22, 36, 59, 25, 55],
    [31, 25, 15, 60, 60, 22, 17, 19, 55, 55, 20, 20, 53, 19, 55, 22, 46, 25, 43, 60, 37, 28, 10, 55, 52],
    [12, 31, 32, 50, 51, 11, 33, 53, 19, 45, 16, 4, 4, 53, 5, 22, 36, 18, 25, 43, 26, 27, 27, 28, 10],
    [5, 2, 44, 52, 3, 4, 53, 45, 9, 3, 4, 56, 5, 0, 2, 5, 10, 47, 52, 3, 63, 39, 10, 9, 52],
    [12, 34, 44, 44, 3, 56, 56, 62, 45, 9, 56, 56, 7, 5, 0, 22, 38, 40, 47, 52, 48, 57, 39, 10, 9],
    [35, 11, 23, 14, 51, 35, 20, 41, 56, 62, 16, 20, 41, 56, 7, 16, 21, 38, 24, 40, 26, 26, 42, 57, 39],
    [33, 34, 51, 51, 52, 41, 41, 34, 62, 0, 41, 41, 56, 7, 5, 56, 38, 38, 40, 44, 37, 42, 57, 39, 10],
    [16, 31, 32, 15, 60, 30, 4, 17, 19, 25, 22, 20, 4, 53, 19, 21, 22, 46, 25, 55, 26, 48, 63, 58, 55],
];

const CLIP_VALUES: [[i32; 4]; 3] = [[256, 32, 8, 2], [512, 64, 16, 4], [1024, 128, 32, 8]];

const TRANSPOSE: [[usize; 13]; 4] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
    [9, 4, 10, 8, 1, 5, 11, 7, 3, 0, 2, 6, 12],
    [0, 3, 2, 1, 8, 7, 6, 5, 4, 9, 10, 11, 12],
    [9, 8, 10, 4, 3, 7, 11, 5, 1, 0, 2, 6, 12],
];

/// Per-class luma filters: `[class][transpose]` coefficient and clip sets.
struct LumaFilters {
    coeff: Vec<[[i32; 13]; 4]>,
    clip: Vec<[[i32; 13]; 4]>,
}

impl LumaFilters {
    fn build(base: impl Fn(usize) -> ([i32; 13], [i32; 13])) -> Self {
        let mut coeff = Vec::with_capacity(25);
        let mut clip = Vec::with_capacity(25);
        for k in 0..25 {
            let (c, l) = base(k);
            let mut cs = [[0; 13]; 4];
            let mut ls = [[0; 13]; 4];
            for t in 0..4 {
                for i in 0..13 {
                    cs[t][i] = c[TRANSPOSE[t][i]];
                    ls[t][i] = l[TRANSPOSE[t][i]];
                }
            }
            coeff.push(cs);
            clip.push(ls);
        }
        Self { coeff, clip }
    }
}

/// Sample source: either the picture (border-replicated) or a padded
/// temporary region.
trait Src {
    fn g(&self, x: i32, y: i32) -> i32;
}

struct PicSrc<'a>(&'a Plane);

impl Src for PicSrc<'_> {
    #[inline]
    fn g(&self, x: i32, y: i32) -> i32 {
        let p = self.0;
        let x = x.clamp(0, p.width as i32 - 1);
        let y = y.clamp(0, p.height as i32 - 1);
        i32::from(p.at(x, y))
    }
}

/// vvdec's temporary ALF buffer: a region copied from the picture, then
/// border-extended by replication.
struct TmpSrc {
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,
    data: Vec<i32>,
}

impl TmpSrc {
    fn new(p: &Plane, x0: i32, y0: i32, w: i32, h: i32) -> Self {
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                data.push(PicSrc(p).g(x, y));
            }
        }
        Self { x0, y0, w, h, data }
    }

    /// vvdec's `padBorderPel`: dir 1 = top-left, dir 2 = bottom-right.
    fn pad_border(&mut self, mx: i32, my: i32, dir: u8) {
        let w = self.w;
        if dir == 1 {
            for y in 0..my.min(self.h) {
                let v = self.data[(y * w + mx.min(w - 1)) as usize];
                for x in 0..mx.min(w) {
                    self.data[(y * w + x) as usize] = v;
                }
            }
        } else {
            for y in (self.h - my).max(0)..self.h {
                let base = y * w + w - mx;
                if base < 1 {
                    continue;
                }
                let v = self.data[(base - 1) as usize];
                for x in 0..mx {
                    self.data[(base + x) as usize] = v;
                }
            }
        }
    }
}

impl Src for TmpSrc {
    #[inline]
    fn g(&self, x: i32, y: i32) -> i32 {
        let x = (x - self.x0).clamp(0, self.w - 1);
        let y = (y - self.y0).clamp(0, self.h - 1);
        self.data[(y * self.w + x) as usize]
    }
}

#[inline]
fn clip_alf(clip: i32, cur: i32, a: i32, b: i32) -> i32 {
    (a - cur).clamp(-clip, clip) + (b - cur).clamp(-clip, clip)
}

/// vvdec's `deriveClassificationBlk` for one 4x4 block at `(x, y)`;
/// `ry` is `y` relative to the CTU.
fn classify<S: Src>(s: &S, x: i32, y: i32, ry: i32, bd: u32, vb_h: i32, vb_pos: i32) -> (usize, usize) {
    const TH: [usize; 16] = [0, 1, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3, 4];
    let lap = |dx: i32, dy: i32| -> [i32; 4] {
        let px = x - 2 + dx;
        let py = y - 2 + dy;
        let rr = ry - 2 + dy;
        let (mut r0, r1, r2, mut r3) = (py - 1, py, py + 1, py + 2);
        if rr > 0 && rr % vb_h == vb_pos - 2 {
            r3 = r2;
        } else if rr > 0 && rr % vb_h == vb_pos {
            r0 = r1;
        }
        let y0 = s.g(px, r1) << 1;
        let yup1 = s.g(px + 1, r2) << 1;
        [
            (y0 - s.g(px, r0) - s.g(px, r2)).abs() + (yup1 - s.g(px + 1, r1) - s.g(px + 1, r3)).abs(),
            (y0 - s.g(px + 1, r1) - s.g(px - 1, r1)).abs() + (yup1 - s.g(px + 2, r2) - s.g(px, r2)).abs(),
            (y0 - s.g(px - 1, r0) - s.g(px + 1, r2)).abs() + (yup1 - s.g(px, r1) - s.g(px + 2, r3)).abs(),
            (y0 - s.g(px - 1, r2) - s.g(px + 1, r0)).abs() + (yup1 - s.g(px, r3) - s.g(px + 2, r1)).abs(),
        ]
    };
    let m = ry % vb_h;
    let rows: &[i32] = if m == vb_pos - 4 {
        &[0, 2, 4]
    } else if m == vb_pos {
        &[2, 4, 6]
    } else {
        &[0, 2, 4, 6]
    };
    let mut sum = [0i32; 4];
    for &dy in rows {
        for dx in [0, 2, 4, 6] {
            let l = lap(dx, dy);
            for k in 0..4 {
                sum[k] += l[k];
            }
        }
    }
    let [sv, sh, sd0, sd1] = sum;
    let mult = if m == vb_pos - 4 || m == vb_pos { 96 } else { 64 };
    let activity = ((sv + sh) * mult >> (bd + 4)).clamp(0, 15);
    let mut class = TH[activity as usize];
    let (hv1, hv0, dir_hv) = if sv > sh { (sv, sh, 1) } else { (sh, sv, 3) };
    let (d1, d0, dir_d) = if sd0 > sd1 { (sd0, sd1, 0) } else { (sd1, sd0, 2) };
    let (hvd1, hvd0, main, second) =
        if (d1 as u32).wrapping_mul(hv0 as u32) > (hv1 as u32).wrapping_mul(d0 as u32) { (d1, d0, dir_d, dir_hv) } else { (hv1, hv0, dir_hv, dir_d) };
    let mut strength = 0;
    if hvd1 > 2 * hvd0 {
        strength = 1;
    }
    if hvd1 * 2 > 9 * hvd0 {
        strength = 2;
    }
    if strength != 0 {
        class += (((main & 1) << 1) + strength) * 5;
    }
    const TRANSPOSE_TABLE: [usize; 8] = [0, 1, 0, 2, 2, 3, 1, 3];
    (class, TRANSPOSE_TABLE[main * 2 + (second >> 1)])
}

struct Vb {
    h: i32,
    pos: i32,
}

/// vvdec's `filterBlk`: filters `(x, y, w, h)` (picture coordinates) of one
/// component into `dst`. `ctu_y` is the CTU's first row in this component.
#[allow(clippy::too_many_arguments)]
fn filter_blk<S: Src>(s: &S, dst: &mut Plane, x0: i32, y0: i32, w: i32, h: i32, ctu_y: i32, chroma: bool, luma: Option<&LumaFilters>, cc: &[i32; 13], cl: &[i32; 13], vb: &Vb, bd: u32, max: i32) {
    let mut by = 0;
    while by < h {
        let mut bx = 0;
        while bx < w {
            let (coeff, clip) = if let Some(lf) = luma {
                let (c, t) = classify(s, x0 + bx, y0 + by, y0 + by - ctu_y, bd, vb.h, vb.pos);
                (&lf.coeff[c][t], &lf.clip[c][t])
            } else {
                (cc, cl)
            };
            for ii in 0..4.min(h - by) {
                let y = y0 + by + ii;
                let y_vb = (y - ctu_y) & (vb.h - 1);
                let (mut r1, mut r2, mut r3, mut r4, mut r5, mut r6) = (y + 1, y - 1, y + 2, y - 2, y + 3, y - 3);
                let near = if chroma { 2 } else { 4 };
                if y_vb < vb.pos && y_vb >= vb.pos - near {
                    if y_vb == vb.pos - 1 {
                        r1 = y;
                        r2 = y;
                    }
                    if y_vb >= vb.pos - 2 {
                        r3 = r1;
                        r4 = r2;
                    }
                    if y_vb >= vb.pos - 3 {
                        r5 = r3;
                        r6 = r4;
                    }
                } else if y_vb >= vb.pos && y_vb <= vb.pos + near - 1 {
                    if y_vb == vb.pos {
                        r2 = y;
                        r1 = y;
                    }
                    if y_vb <= vb.pos + 1 {
                        r4 = r2;
                        r3 = r1;
                    }
                    if y_vb <= vb.pos + 2 {
                        r6 = r4;
                        r5 = r3;
                    }
                }
                let near_vb = y_vb == vb.pos - 1 || y_vb == vb.pos;
                for jj in 0..4.min(w - bx) {
                    let x = x0 + bx + jj;
                    let cur = s.g(x, y);
                    let mut sum = 0;
                    if !chroma {
                        sum += coeff[0] * clip_alf(clip[0], cur, s.g(x, r5), s.g(x, r6));
                        sum += coeff[1] * clip_alf(clip[1], cur, s.g(x + 1, r3), s.g(x - 1, r4));
                        sum += coeff[2] * clip_alf(clip[2], cur, s.g(x, r3), s.g(x, r4));
                        sum += coeff[3] * clip_alf(clip[3], cur, s.g(x - 1, r3), s.g(x + 1, r4));
                        sum += coeff[4] * clip_alf(clip[4], cur, s.g(x + 2, r1), s.g(x - 2, r2));
                        sum += coeff[5] * clip_alf(clip[5], cur, s.g(x + 1, r1), s.g(x - 1, r2));
                        sum += coeff[6] * clip_alf(clip[6], cur, s.g(x, r1), s.g(x, r2));
                        sum += coeff[7] * clip_alf(clip[7], cur, s.g(x - 1, r1), s.g(x + 1, r2));
                        sum += coeff[8] * clip_alf(clip[8], cur, s.g(x - 2, r1), s.g(x + 2, r2));
                        sum += coeff[9] * clip_alf(clip[9], cur, s.g(x + 3, y), s.g(x - 3, y));
                        sum += coeff[10] * clip_alf(clip[10], cur, s.g(x + 2, y), s.g(x - 2, y));
                        sum += coeff[11] * clip_alf(clip[11], cur, s.g(x + 1, y), s.g(x - 1, y));
                    } else {
                        sum += coeff[0] * clip_alf(clip[0], cur, s.g(x, r3), s.g(x, r4));
                        sum += coeff[1] * clip_alf(clip[1], cur, s.g(x + 1, r1), s.g(x - 1, r2));
                        sum += coeff[2] * clip_alf(clip[2], cur, s.g(x, r1), s.g(x, r2));
                        sum += coeff[3] * clip_alf(clip[3], cur, s.g(x - 1, r1), s.g(x + 1, r2));
                        sum += coeff[4] * clip_alf(clip[4], cur, s.g(x + 2, y), s.g(x - 2, y));
                        sum += coeff[5] * clip_alf(clip[5], cur, s.g(x + 1, y), s.g(x - 1, y));
                    }
                    sum = if near_vb { (sum + (1 << 9)) >> 10 } else { (sum + 64) >> 7 };
                    dst.set(x, y, (sum + cur).clamp(0, max) as i16);
                }
            }
            bx += 4;
        }
        by += 4;
    }
}

/// vvdec's `filterBlkCcAlf`: adds the cross-component correction to
/// `dst` for the chroma block `(x0, y0, w, h)`; `luma` is the unfiltered
/// luma source.
#[allow(clippy::too_many_arguments)]
fn filter_cc<S: Src>(luma: &S, dst: &mut Plane, x0: i32, y0: i32, w: i32, h: i32, ctu_yc: i32, sx: u32, sy: u32, coeff: &[i16; 8], vb: &Vb, bd: u32, max: i32) {
    for yc in y0..y0 + h {
        let pos = ((yc - ctu_yc) << sy) & (vb.h - 1);
        if sy == 0 && (pos == vb.pos || pos == vb.pos + 1) {
            continue;
        }
        let (mut o1, mut o2, mut o3) = (1, -1, 2);
        if pos == vb.pos - 2 || pos == vb.pos + 1 {
            o3 = o1;
        } else if pos == vb.pos - 1 || pos == vb.pos {
            o1 = 0;
            o2 = 0;
            o3 = 0;
        }
        let ly = yc << sy;
        for xc in x0..x0 + w {
            let lx = xc << sx;
            let cur = luma.g(lx, ly);
            let c = |i: usize| i32::from(coeff[i]);
            let mut sum = 0;
            sum += c(0) * (luma.g(lx, ly + o2) - cur);
            sum += c(1) * (luma.g(lx - 1, ly) - cur);
            sum += c(2) * (luma.g(lx + 1, ly) - cur);
            sum += c(3) * (luma.g(lx - 1, ly + o1) - cur);
            sum += c(4) * (luma.g(lx, ly + o1) - cur);
            sum += c(5) * (luma.g(lx + 1, ly + o1) - cur);
            sum += c(6) * (luma.g(lx, ly + o3) - cur);
            sum = (sum + 64) >> 7;
            let off = (1 << bd) >> 1;
            sum = (sum + off).clamp(0, max) - off;
            let v = sum + i32::from(dst.at(xc, yc));
            dst.set(xc, yc, v.clamp(0, max) as i16);
        }
    }
}

struct CtuFilters {
    luma: Option<LumaFilters>,
    chroma: [Option<([i32; 13], [i32; 13])>; 2],
    cc: [Option<[i16; 8]>; 2],
}

fn luma_filters(idx: u16, sh: &SliceHeader, aps: &[Option<AlfParam>; 8], bd: u32) -> Option<LumaFilters> {
    let vls = CLIP_VALUES[(bd - 8) as usize];
    if idx < 16 {
        let set = idx as usize;
        Some(LumaFilters::build(|k| {
            let f = CLASS_TO_FILTER[set][k] as usize;
            let mut c = [0i32; 13];
            for i in 0..12 {
                c[i] = i32::from(FIXED_COEFF[f][i]);
            }
            c[12] = 128;
            (c, [vls[0]; 13])
        }))
    } else {
        let id = *sh.alf_aps_ids_luma.get((idx - 16) as usize)? as usize;
        let a = aps.get(id)?.as_ref()?;
        Some(LumaFilters::build(|k| {
            let f = a.coeff_delta_idx[k] as usize;
            let mut c = [0i32; 13];
            let mut l = [vls[0]; 13];
            for i in 0..12 {
                c[i] = i32::from(a.luma_coeff[f * 13 + i]);
                if a.nonlinear_luma {
                    l[i] = vls[a.luma_clip[f * 13 + i] as usize];
                }
            }
            (c, l)
        }))
    }
}

/// Applies ALF and CC-ALF to the whole (SAO-filtered) picture.
pub fn alf(pic: &mut Picture, sps: &Sps, pps: &Pps, ph: &PicHeader, slices: &[SliceHeader], aps: &[Option<AlfParam>; 8]) {
    let nc = pic.fmt.num_comp();
    let any = pic.ctus.iter().any(|c| c.alf.enable.iter().any(|&e| e) || c.alf.cc.iter().any(|&v| v != 0));
    if !any || pic.bit_depth > 10 {
        return;
    }
    let src = pic.planes.clone();
    let bd = pic.bit_depth;
    let max = (1i32 << bd) - 1;
    let vls = CLIP_VALUES[(bd - 8) as usize];
    let ctu = 1i32 << pic.ctu_log2;
    let wc = pic.width_ctus as i32;
    let (csx, csy) = pic.fmt.scale(1);
    let vb_l = Vb { h: ctu, pos: ctu - 4 };
    let vb_c = Vb { h: ctu >> csy, pos: (ctu >> csy) - 2 };
    let num_tiles = pps.num_tiles();
    let ctu_count = pic.ctus.len() as u32;
    for addr in 0..pic.ctus.len() {
        let cd = pic.ctus[addr].clone();
        let Some(si) = cd.slice else { continue };
        let sh = &slices[si as usize];
        let a = cd.alf;
        let cc_on = |c: usize| sh.ccalf_enabled[c] && a.cc[c] != 0;
        if !a.enable.iter().any(|&e| e) && !cc_on(0) && !cc_on(1) {
            continue;
        }
        let filters = CtuFilters {
            luma: if a.enable[0] { luma_filters(a.filter_idx, sh, aps, bd) } else { None },
            chroma: [1, 2].map(|c| {
                if nc > 1 && a.enable[c] {
                    let p = aps[sh.alf_aps_id_chroma as usize].as_ref()?;
                    let alt = a.alt[c - 1] as usize;
                    let mut co = [0i32; 13];
                    let mut cl = [vls[0]; 13];
                    for i in 0..6 {
                        co[i] = i32::from(p.chroma_coeff[alt * 7 + i]);
                        if p.nonlinear_chroma {
                            cl[i] = vls[p.chroma_clip[alt * 7 + i] as usize];
                        }
                    }
                    Some((co, cl))
                } else {
                    None
                }
            }),
            cc: [0, 1].map(|c| {
                if nc > 1 && cc_on(c) {
                    let p = aps[sh.ccalf_aps_id[c] as usize].as_ref()?;
                    Some(p.cc_coeff[c][a.cc[c] as usize - 1])
                } else {
                    None
                }
            }),
        };
        let (cx, cy) = (addr as i32 % wc, addr as i32 / wc);
        let (x0, y0) = (cx * ctu, cy * ctu);
        let (w, h) = (ctu.min(pic.width - x0), ctu.min(pic.height - y0));
        // vvdec's isClipOrCrossedByVirtualBoundaries
        let (mut ct, mut cb, mut cl, mut cr) = (false, false, false, false);
        let mut vb_hor = Vec::new();
        let mut vb_ver = Vec::new();
        if ph.vb_present {
            for &p in &ph.vb_pos_y {
                let p = p as i32;
                if p == y0 {
                    ct = true;
                } else if p == y0 + h {
                    cb = true;
                } else if y0 < p && p < y0 + h {
                    vb_hor.push(p);
                }
            }
            for &p in &ph.vb_pos_x {
                let p = p as i32;
                if p == x0 {
                    cl = true;
                } else if p == x0 + w {
                    cr = true;
                } else if x0 < p && p < x0 + w {
                    vb_ver.push(p);
                }
            }
        }
        let subpic_of = |a: usize| -> usize {
            let (x, y) = ((a as i32 % wc) as u32, (a as i32 / wc) as u32);
            (0..sps.num_subpics as usize)
                .find(|&i| x >= sps.subpic_x[i] && x < sps.subpic_x[i] + sps.subpic_w[i] && y >= sps.subpic_y[i] && y < sps.subpic_y[i] + sps.subpic_h[i])
                .unwrap_or(0)
        };
        let across_subpic = !sps.subpic_info_present || sps.loop_filter_across_subpic[subpic_of(addr)];
        let across_tiles = num_tiles <= 1 || pps.loop_filter_across_tiles;
        let slice_ctus = pic.ctus.iter().filter(|c| c.slice == Some(si)).count() as u32;
        let across_slices = slice_ctus == ctu_count || pps.loop_filter_across_slices;
        let restrict_any = !across_slices || !across_tiles || !across_subpic;
        let avail = |o: usize| -> bool {
            let oc = &pic.ctus[o];
            (across_slices || oc.slice == cd.slice) && (across_tiles || oc.tile == cd.tile) && (across_subpic || subpic_of(o) == subpic_of(addr))
        };
        let at = |dx: i32, dy: i32| ((cy + dy) * wc + cx + dx) as usize;
        if y0 >= ctu && !ct && restrict_any && !avail(at(0, -1)) {
            ct = true;
        }
        if y0 + ctu < pic.height && !cb && restrict_any && !avail(at(0, 1)) {
            cb = true;
        }
        if x0 >= ctu && !cl && restrict_any && !avail(at(-1, 0)) {
            cl = true;
        }
        if x0 + ctu < pic.width && !cr && restrict_any && !avail(at(1, 0)) {
            cr = true;
        }
        let mut raster_pad = 0u8;
        if !ct && !cl && !across_slices && x0 >= ctu && y0 >= ctu && pic.ctus[at(-1, -1)].slice != cd.slice {
            raster_pad = 1;
        }
        if !cb && !cr && !across_slices && x0 + ctu < pic.width && y0 + ctu < pic.height && pic.ctus[at(1, 1)].slice != cd.slice {
            raster_pad += 2;
        }
        let crossed = !vb_hor.is_empty() || !vb_ver.is_empty() || ct || cb || cl || cr || raster_pad != 0;
        let (planes_y, rest) = pic.planes.split_at_mut(1);
        if !crossed {
            if let Some(lf) = &filters.luma {
                filter_blk(&PicSrc(&src[0]), &mut planes_y[0], x0, y0, w, h, y0, false, Some(lf), &[0; 13], &[0; 13], &vb_l, bd, max);
            }
            for c in 1..nc {
                let (bx, by, bw, bh) = (x0 >> csx, y0 >> csy, w >> csx, h >> csy);
                if let Some((co, cl)) = &filters.chroma[c - 1] {
                    filter_blk(&PicSrc(&src[c]), &mut rest[c - 1], bx, by, bw, bh, by, true, None, co, cl, &vb_c, bd, max);
                }
                if let Some(coeff) = &filters.cc[c - 1] {
                    filter_cc(&PicSrc(&src[0]), &mut rest[c - 1], bx, by, bw, bh, by, csx, csy, coeff, &vb_l, bd, max);
                }
            }
            continue;
        }
        const PAD: i32 = 4;
        for c in 0..nc {
            let cc_slice = c > 0 && sh.ccalf_enabled[c - 1];
            if !a.enable[c] && (c == 0 || !cc_on(c - 1)) {
                continue;
            }
            let (sx, sy) = pic.fmt.scale(c);
            let mut ys = y0;
            for i in 0..=vb_hor.len() {
                let ye = if i == vb_hor.len() { y0 + h } else { vb_hor[i] };
                let hh = ye - ys;
                let clip_t = (i == 0 && ct) || i > 0 || ys == 0;
                let clip_b = (i == vb_hor.len() && cb) || i < vb_hor.len() || ye == pic.height;
                let mut xs = x0;
                for j in 0..=vb_ver.len() {
                    let xe = if j == vb_ver.len() { x0 + w } else { vb_ver[j] };
                    let ww = xe - xs;
                    let clip_l = (j == 0 && cl) || j > 0 || xs == 0;
                    let clip_r = (j == vb_ver.len() && cr) || j < vb_ver.len() || xe == pic.width;
                    let pl = if clip_l { 0 } else { PAD };
                    let pr = if clip_r { 0 } else { PAD };
                    let pt = if clip_t { 0 } else { PAD };
                    let pb = if clip_b { 0 } else { PAD };
                    // padded region in luma units, then per component
                    let (rx, ry, rw, rh) = (xs - pl, ys - pt, ww + pl + pr, hh + pt + pb);
                    let region = |comp: usize| {
                        let (qx, qy) = pic.fmt.scale(comp);
                        TmpSrc::new(&src[comp], rx >> qx, ry >> qy, rw >> qx, rh >> qy)
                    };
                    let first = xs == x0 && ys == y0 && raster_pad & 1 != 0;
                    let last = xe == x0 + w && ye == y0 + h && raster_pad & 2 != 0;
                    if c == 0 || !cc_slice {
                        let mut t = region(c);
                        if first {
                            t.pad_border(PAD, PAD, 1);
                        }
                        if last {
                            t.pad_border(PAD, PAD, 2);
                        }
                        if c == 0 {
                            if let Some(lf) = &filters.luma {
                                filter_blk(&t, &mut planes_y[0], xs, ys, ww, hh, y0, false, Some(lf), &[0; 13], &[0; 13], &vb_l, bd, max);
                            }
                        } else if let Some((co, clp)) = &filters.chroma[c - 1] {
                            filter_blk(&t, &mut rest[c - 1], xs >> sx, ys >> sy, ww >> sx, hh >> sy, y0 >> sy, true, None, co, clp, &vb_c, bd, max);
                        }
                    } else {
                        let mut tl = region(0);
                        let mut tc = region(c);
                        if first {
                            tl.pad_border(PAD, PAD, 1);
                            tc.pad_border(PAD >> sx, PAD >> sy, 1);
                        }
                        if last {
                            tl.pad_border(PAD, PAD, 2);
                            tc.pad_border(PAD >> sx, PAD >> sy, 2);
                        }
                        let (bx, by, bw, bh) = (xs >> sx, ys >> sy, ww >> sx, hh >> sy);
                        if let Some((co, clp)) = &filters.chroma[c - 1] {
                            filter_blk(&tc, &mut rest[c - 1], bx, by, bw, bh, y0 >> sy, true, None, co, clp, &vb_c, bd, max);
                        }
                        if let Some(coeff) = &filters.cc[c - 1] {
                            filter_cc(&tl, &mut rest[c - 1], bx, by, bw, bh, y0 >> sy, sx, sy, coeff, &vb_l, bd, max);
                        }
                    }
                    xs = xe;
                }
                ys = ye;
            }
        }
    }
}
