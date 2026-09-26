//! libheifer: fearless_simd deblocking kernels.
//!
//! One call filters a whole 16-sample luma edge, or the matching 8-sample Cb and
//! Cr edges packed into one 16-lane vector, instead of one scalar call per line.
//! Every lane carries its own alpha/beta/tc0 and boundary strength, so Cb and Cr
//! keep separate thresholds. The scalar line filters in `deblock.rs` remain the
//! oracle (`simd_matches_scalar` there, plus the libheif differential suites).
//!
//! Everything reachable from `dispatch!` is `#[inline(always)]` and closure-free
//! so it compiles inside the selected target-feature context.

use fearless_simd::{dispatch, f64x2, i16x16, mask16x16, prelude::*, u16x8, u32x4, u8x16, Level};

/// SIMD level for this process, detected once by fearless_simd.
#[inline]
pub fn level() -> Level {
    Level::try_detect().unwrap_or_else(Level::baseline)
}

/// Per-lane parameters. `tc0 < 0` together with `strong == 0` disables a lane.
#[derive(Clone, Copy)]
pub struct Lanes {
    pub alpha: [i16; 16],
    pub beta: [i16; 16],
    pub tc0: [i16; 16],
    /// `-1` where the boundary strength is 4, else `0`.
    pub strong: [i16; 16],
}

impl Lanes {
    #[inline]
    fn set(&mut self, i: usize, bs: i32, alpha: i32, beta: i32, tc0: [i32; 3]) {
        // alpha <= 255, beta <= 18, tc0 <= 25: all fit in i16.
        self.alpha[i] = alpha as i16;
        self.beta[i] = beta as i16;
        self.tc0[i] = if (1..4).contains(&bs) {
            tc0[bs as usize - 1] as i16
        } else {
            -1
        };
        self.strong[i] = -((bs >= 4) as i16);
    }

    /// Luma edge: lane `i` belongs to segment `i / 4`.
    pub fn luma(bs: [i32; 4], alpha: i32, beta: i32, tc0: [i32; 3]) -> Self {
        let mut lanes = Lanes {
            alpha: [0; 16],
            beta: [0; 16],
            tc0: [-1; 16],
            strong: [0; 16],
        };
        for i in 0..16 {
            lanes.set(i, bs[i / 4], alpha, beta, tc0);
        }
        lanes
    }

    /// 4:2:0 chroma edge: lanes 0..8 are Cb, 8..16 are Cr; lane `i` belongs to
    /// segment `(i % 8) / 2`. `params[plane] = (alpha, beta, tc0 table)`.
    pub fn chroma(bs: [i32; 4], params: [(i32, i32, [i32; 3]); 2]) -> Self {
        let mut lanes = Lanes {
            alpha: [0; 16],
            beta: [0; 16],
            tc0: [-1; 16],
            strong: [0; 16],
        };
        for i in 0..16 {
            let (alpha, beta, tc0) = params[i / 8];
            lanes.set(i, bs[(i % 8) / 2], alpha, beta, tc0);
        }
        lanes
    }
}

#[inline(always)]
fn widen<S: Simd>(s: S, v: u8x16<S>) -> i16x16<S> {
    let (lo, hi) = s.widen_u8x16(v);
    s.combine_u16x8(lo, hi).bitcast()
}

#[inline(always)]
fn narrow<S: Simd>(s: S, v: i16x16<S>) -> u8x16<S> {
    let clamped = s.min_i16x16(s.max_i16x16(v, i16x16::splat(s, 0)), i16x16::splat(s, 255));
    let (lo, hi): (u16x8<S>, u16x8<S>) = s.split_u16x16(clamped.bitcast());
    s.saturating_narrow_u16x8(lo, hi)
}

#[inline(always)]
fn absdiff<S: Simd>(s: S, a: i16x16<S>, b: i16x16<S>) -> i16x16<S> {
    s.abs_i16x16(a - b)
}

#[inline(always)]
fn clamp<S: Simd>(s: S, v: i16x16<S>, lo: i16x16<S>, hi: i16x16<S>) -> i16x16<S> {
    s.min_i16x16(s.max_i16x16(v, lo), hi)
}

#[inline(always)]
fn pick<S: Simd>(
    s: S,
    filter: mask16x16<S>,
    strong_bs: mask16x16<S>,
    strong: i16x16<S>,
    normal: i16x16<S>,
    orig: i16x16<S>,
) -> i16x16<S> {
    s.select_i16x16(filter, s.select_i16x16(strong_bs, strong, normal), orig)
}

/// `v[i]` holds rows `i` (low half) and `i + 8` (high half), 8 samples each;
/// returns the 8 columns, 16 rows each.
#[inline(always)]
fn transpose<S: Simd>(s: S, v: [u8x16<S>; 8]) -> [u8x16<S>; 8] {
    let (l01, h01) = s.interleave_u8x16(v[0], v[1]);
    let (l23, h23) = s.interleave_u8x16(v[2], v[3]);
    let (l45, h45) = s.interleave_u8x16(v[4], v[5]);
    let (l67, h67) = s.interleave_u8x16(v[6], v[7]);
    let (m0, m1) = s.interleave_u16x8(l01.bitcast::<u16x8<S>>(), l23.bitcast::<u16x8<S>>());
    let (m2, m3) = s.interleave_u16x8(l45.bitcast::<u16x8<S>>(), l67.bitcast::<u16x8<S>>());
    let (n0, n1) = s.interleave_u16x8(h01.bitcast::<u16x8<S>>(), h23.bitcast::<u16x8<S>>());
    let (n2, n3) = s.interleave_u16x8(h45.bitcast::<u16x8<S>>(), h67.bitcast::<u16x8<S>>());
    let (p0, p1) = s.interleave_u32x4(m0.bitcast::<u32x4<S>>(), m2.bitcast::<u32x4<S>>());
    let (p2, p3) = s.interleave_u32x4(m1.bitcast::<u32x4<S>>(), m3.bitcast::<u32x4<S>>());
    let (q0, q1) = s.interleave_u32x4(n0.bitcast::<u32x4<S>>(), n2.bitcast::<u32x4<S>>());
    let (q2, q3) = s.interleave_u32x4(n1.bitcast::<u32x4<S>>(), n3.bitcast::<u32x4<S>>());
    let (c0, c1) = s.interleave_f64x2(p0.bitcast::<f64x2<S>>(), q0.bitcast::<f64x2<S>>());
    let (c2, c3) = s.interleave_f64x2(p1.bitcast::<f64x2<S>>(), q1.bitcast::<f64x2<S>>());
    let (c4, c5) = s.interleave_f64x2(p2.bitcast::<f64x2<S>>(), q2.bitcast::<f64x2<S>>());
    let (c6, c7) = s.interleave_f64x2(p3.bitcast::<f64x2<S>>(), q3.bitcast::<f64x2<S>>());
    [
        c0.bitcast(),
        c1.bitcast(),
        c2.bitcast(),
        c3.bitcast(),
        c4.bitcast(),
        c5.bitcast(),
        c6.bitcast(),
        c7.bitcast(),
    ]
}

/// Inverse of [`transpose`].
#[inline(always)]
fn untranspose<S: Simd>(s: S, c: [u8x16<S>; 8]) -> [u8x16<S>; 8] {
    let (p0, q0) = s.deinterleave_f64x2(c[0].bitcast::<f64x2<S>>(), c[1].bitcast::<f64x2<S>>());
    let (p1, q1) = s.deinterleave_f64x2(c[2].bitcast::<f64x2<S>>(), c[3].bitcast::<f64x2<S>>());
    let (p2, q2) = s.deinterleave_f64x2(c[4].bitcast::<f64x2<S>>(), c[5].bitcast::<f64x2<S>>());
    let (p3, q3) = s.deinterleave_f64x2(c[6].bitcast::<f64x2<S>>(), c[7].bitcast::<f64x2<S>>());
    let (m0, m2) = s.deinterleave_u32x4(p0.bitcast::<u32x4<S>>(), p1.bitcast::<u32x4<S>>());
    let (m1, m3) = s.deinterleave_u32x4(p2.bitcast::<u32x4<S>>(), p3.bitcast::<u32x4<S>>());
    let (n0, n2) = s.deinterleave_u32x4(q0.bitcast::<u32x4<S>>(), q1.bitcast::<u32x4<S>>());
    let (n1, n3) = s.deinterleave_u32x4(q2.bitcast::<u32x4<S>>(), q3.bitcast::<u32x4<S>>());
    let (l01, l23) = s.deinterleave_u16x8(m0.bitcast::<u16x8<S>>(), m1.bitcast::<u16x8<S>>());
    let (l45, l67) = s.deinterleave_u16x8(m2.bitcast::<u16x8<S>>(), m3.bitcast::<u16x8<S>>());
    let (h01, h23) = s.deinterleave_u16x8(n0.bitcast::<u16x8<S>>(), n1.bitcast::<u16x8<S>>());
    let (h45, h67) = s.deinterleave_u16x8(n2.bitcast::<u16x8<S>>(), n3.bitcast::<u16x8<S>>());
    let (v0, v1) = s.deinterleave_u8x16(l01.bitcast::<u8x16<S>>(), h01.bitcast::<u8x16<S>>());
    let (v2, v3) = s.deinterleave_u8x16(l23.bitcast::<u8x16<S>>(), h23.bitcast::<u8x16<S>>());
    let (v4, v5) = s.deinterleave_u8x16(l45.bitcast::<u8x16<S>>(), h45.bitcast::<u8x16<S>>());
    let (v6, v7) = s.deinterleave_u8x16(l67.bitcast::<u8x16<S>>(), h67.bitcast::<u8x16<S>>());
    [v0, v1, v2, v3, v4, v5, v6, v7]
}

/// `v = [p3, p2, p1, p0, q0, q1, q2, q3]`; returns it with p2..q2 filtered.
#[inline(always)]
fn luma_core<S: Simd>(s: S, v: [u8x16<S>; 8], lanes: &Lanes) -> Option<[u8x16<S>; 8]> {
    let (p3, p2, p1, p0) = (
        widen(s, v[0]),
        widen(s, v[1]),
        widen(s, v[2]),
        widen(s, v[3]),
    );
    let (q0, q1, q2, q3) = (
        widen(s, v[4]),
        widen(s, v[5]),
        widen(s, v[6]),
        widen(s, v[7]),
    );
    let alpha = i16x16::from_slice(s, &lanes.alpha);
    let beta = i16x16::from_slice(s, &lanes.beta);
    let tc0 = i16x16::from_slice(s, &lanes.tc0);
    let zero = i16x16::splat(s, 0);
    let strong_bs = s.simd_lt_i16x16(i16x16::from_slice(s, &lanes.strong), zero);
    let filter = (s.simd_ge_i16x16(tc0, zero) | strong_bs)
        & s.simd_lt_i16x16(absdiff(s, p0, q0), alpha)
        & s.simd_lt_i16x16(absdiff(s, p1, p0), beta)
        & s.simd_lt_i16x16(absdiff(s, q1, q0), beta);
    if !s.any_true_mask16x16(filter) {
        return None;
    }
    let ap = s.simd_lt_i16x16(absdiff(s, p2, p0), beta);
    let aq = s.simd_lt_i16x16(absdiff(s, q2, q0), beta);

    // Boundary strength < 4. Masks are all-ones (-1) lanes.
    let tc = tc0
        - s.select_i16x16(ap, i16x16::splat(s, -1), zero)
        - s.select_i16x16(aq, i16x16::splat(s, -1), zero);
    let delta = clamp(s, (((q0 - p0) << 2u32) + (p1 - q1) + 4) >> 3u32, -tc, tc);
    let avg = (p0 + q0 + 1) >> 1u32;
    let n_p0 = p0 + delta;
    let n_q0 = q0 - delta;
    let n_p1 = s.select_i16x16(
        ap,
        p1 + clamp(s, (p2 + avg - (p1 << 1u32)) >> 1u32, -tc0, tc0),
        p1,
    );
    let n_q1 = s.select_i16x16(
        aq,
        q1 + clamp(s, (q2 + avg - (q1 << 1u32)) >> 1u32, -tc0, tc0),
        q1,
    );

    // Boundary strength 4.
    let strong = s.simd_lt_i16x16(absdiff(s, p0, q0), (alpha >> 2u32) + 2);
    let sp = strong & ap;
    let sq = strong & aq;
    let pq = p0 + q0;
    let s_p0 = s.select_i16x16(
        sp,
        (p2 + ((p1 + pq) << 1u32) + q1 + 4) >> 3u32,
        ((p1 << 1u32) + p0 + q1 + 2) >> 2u32,
    );
    let s_p1 = s.select_i16x16(sp, (p2 + p1 + pq + 2) >> 2u32, p1);
    let s_p2 = s.select_i16x16(sp, ((p3 << 1u32) + p2 * 3 + p1 + pq + 4) >> 3u32, p2);
    let s_q0 = s.select_i16x16(
        sq,
        (q2 + ((q1 + pq) << 1u32) + p1 + 4) >> 3u32,
        ((q1 << 1u32) + q0 + p1 + 2) >> 2u32,
    );
    let s_q1 = s.select_i16x16(sq, (q2 + q1 + pq + 2) >> 2u32, q1);
    let s_q2 = s.select_i16x16(sq, ((q3 << 1u32) + q2 * 3 + q1 + pq + 4) >> 3u32, q2);

    Some([
        v[0],
        narrow(s, pick(s, filter, strong_bs, s_p2, p2, p2)),
        narrow(s, pick(s, filter, strong_bs, s_p1, n_p1, p1)),
        narrow(s, pick(s, filter, strong_bs, s_p0, n_p0, p0)),
        narrow(s, pick(s, filter, strong_bs, s_q0, n_q0, q0)),
        narrow(s, pick(s, filter, strong_bs, s_q1, n_q1, q1)),
        narrow(s, pick(s, filter, strong_bs, s_q2, q2, q2)),
        v[7],
    ])
}

/// `p1, p0, q0, q1`; returns filtered `(p0, q0)`.
#[inline(always)]
fn chroma_core<S: Simd>(s: S, v: [u8x16<S>; 4], lanes: &Lanes) -> Option<(u8x16<S>, u8x16<S>)> {
    let (p1, p0, q0, q1) = (
        widen(s, v[0]),
        widen(s, v[1]),
        widen(s, v[2]),
        widen(s, v[3]),
    );
    let alpha = i16x16::from_slice(s, &lanes.alpha);
    let beta = i16x16::from_slice(s, &lanes.beta);
    let tc0 = i16x16::from_slice(s, &lanes.tc0);
    let zero = i16x16::splat(s, 0);
    let strong_bs = s.simd_lt_i16x16(i16x16::from_slice(s, &lanes.strong), zero);
    let filter = (s.simd_ge_i16x16(tc0, zero) | strong_bs)
        & s.simd_lt_i16x16(absdiff(s, p0, q0), alpha)
        & s.simd_lt_i16x16(absdiff(s, p1, p0), beta)
        & s.simd_lt_i16x16(absdiff(s, q1, q0), beta);
    if !s.any_true_mask16x16(filter) {
        return None;
    }
    let tc = tc0 + 1;
    let delta = clamp(s, (((q0 - p0) << 2u32) + (p1 - q1) + 4) >> 3u32, -tc, tc);
    let n_p0 = s.select_i16x16(strong_bs, ((p1 << 1u32) + p0 + q1 + 2) >> 2u32, p0 + delta);
    let n_q0 = s.select_i16x16(strong_bs, ((q1 << 1u32) + q0 + p1 + 2) >> 2u32, q0 - delta);
    Some((
        narrow(s, s.select_i16x16(filter, n_p0, p0)),
        narrow(s, s.select_i16x16(filter, n_q0, q0)),
    ))
}

#[inline(always)]
fn load<S: Simd>(s: S, plane: &[u8], at: usize) -> u8x16<S> {
    u8x16::from_slice(s, &plane[at..at + 16])
}

/// Two 8-sample runs as one vector: `a` in the low half, `b` in the high half.
#[inline(always)]
fn load_pair<S: Simd>(s: S, a: &[u8], b: &[u8]) -> u8x16<S> {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&a[..8]);
    bytes[8..].copy_from_slice(&b[..8]);
    u8x16::from_slice(s, &bytes)
}

#[inline(always)]
fn store_pair<S: Simd>(v: u8x16<S>, a: &mut [u8], b: &mut [u8]) {
    let bytes: [u8; 16] = v.into();
    a[..8].copy_from_slice(&bytes[..8]);
    b[..8].copy_from_slice(&bytes[8..]);
}

#[inline(always)]
fn luma_rows_impl<S: Simd>(s: S, plane: &mut [u8], base: usize, stride: usize, lanes: &Lanes) {
    let top = base - 4 * stride;
    let v = [
        load(s, plane, top),
        load(s, plane, top + stride),
        load(s, plane, top + 2 * stride),
        load(s, plane, top + 3 * stride),
        load(s, plane, top + 4 * stride),
        load(s, plane, top + 5 * stride),
        load(s, plane, top + 6 * stride),
        load(s, plane, top + 7 * stride),
    ];
    if let Some(out) = luma_core(s, v, lanes) {
        for k in 1..7 {
            out[k].store_slice(&mut plane[top + k * stride..top + k * stride + 16]);
        }
    }
}

#[inline(always)]
fn luma_cols_impl<S: Simd>(s: S, plane: &mut [u8], base: usize, stride: usize, lanes: &Lanes) {
    let row = |r: usize| base + r * stride - 4;
    let mut v = [u8x16::splat(s, 0); 8];
    for (i, slot) in v.iter_mut().enumerate() {
        *slot = load_pair(s, &plane[row(i)..], &plane[row(i + 8)..]);
    }
    if let Some(out) = luma_core(s, transpose(s, v), lanes) {
        let rows = untranspose(s, out);
        for (i, r) in rows.into_iter().enumerate() {
            let (a, b) = plane.split_at_mut(row(i + 8));
            store_pair(r, &mut a[row(i)..], b);
        }
    }
}

#[inline(always)]
fn chroma_rows_impl<S: Simd>(
    s: S,
    u: &mut [u8],
    v: &mut [u8],
    base: usize,
    stride: usize,
    lanes: &Lanes,
) {
    let top = base - 2 * stride;
    let at = |k: usize| top + k * stride;
    let rows = [
        load_pair(s, &u[at(0)..], &v[at(0)..]),
        load_pair(s, &u[at(1)..], &v[at(1)..]),
        load_pair(s, &u[at(2)..], &v[at(2)..]),
        load_pair(s, &u[at(3)..], &v[at(3)..]),
    ];
    if let Some((p0, q0)) = chroma_core(s, rows, lanes) {
        store_pair(p0, &mut u[at(1)..], &mut v[at(1)..]);
        store_pair(q0, &mut u[at(2)..], &mut v[at(2)..]);
    }
}

#[inline(always)]
fn chroma_cols_impl<S: Simd>(
    s: S,
    u: &mut [u8],
    v: &mut [u8],
    base: usize,
    stride: usize,
    lanes: &Lanes,
) {
    // An 8-sample window p3..q3 around the edge (always inside the row: chroma
    // edges sit at x >= 4 and x + 4 <= width); Cb rows 0..8 then Cr rows 0..8.
    let row = |r: usize| base + r * stride - 4;
    let mut w = [u8x16::splat(s, 0); 8];
    for (i, slot) in w.iter_mut().enumerate() {
        *slot = load_pair(s, &u[row(i)..], &v[row(i)..]);
    }
    let c = transpose(s, w);
    if let Some((p0, q0)) = chroma_core(s, [c[2], c[3], c[4], c[5]], lanes) {
        let rows = untranspose(s, [c[0], c[1], c[2], p0, q0, c[5], c[6], c[7]]);
        for (i, r) in rows.into_iter().enumerate() {
            store_pair(r, &mut u[row(i)..], &mut v[row(i)..]);
        }
    }
}

/// Luma edge across rows (a horizontal edge): `q0` is the row starting at
/// `base`, 16 samples wide.
pub fn luma_rows(level: Level, plane: &mut [u8], base: usize, stride: usize, lanes: &Lanes) {
    dispatch!(level, s => luma_rows_impl(s, plane, base, stride, lanes));
}

/// Luma edge across columns (a vertical edge): `q0` is column `base % stride`
/// of the 16 rows starting at `base`.
pub fn luma_cols(level: Level, plane: &mut [u8], base: usize, stride: usize, lanes: &Lanes) {
    dispatch!(level, s => luma_cols_impl(s, plane, base, stride, lanes));
}

/// Cb and Cr edges across rows: `q0` is the 8-sample row at `base` in each plane.
pub fn chroma_rows(
    level: Level,
    u: &mut [u8],
    v: &mut [u8],
    base: usize,
    stride: usize,
    lanes: &Lanes,
) {
    dispatch!(level, s => chroma_rows_impl(s, u, v, base, stride, lanes));
}

/// Cb and Cr edges across columns: `q0` is column `base % stride` of the 8 rows
/// starting at `base` in each plane.
pub fn chroma_cols(
    level: Level,
    u: &mut [u8],
    v: &mut [u8],
    base: usize,
    stride: usize,
    lanes: &Lanes,
) {
    dispatch!(level, s => chroma_cols_impl(s, u, v, base, stride, lanes));
}
