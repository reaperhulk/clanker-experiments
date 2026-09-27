//! HTJ2K (ITU-T T.814) encoder reproducing the codestreams libheif's OpenJPH
//! encoder plugin writes with OpenJPH 0.32.0: planar components, no colour
//! transform, one quality layer, HT cleanup passes only, and the plugin's
//! decomposition, progression, tile, tile-part, TLM and code-block options.
//!
//! OpenJPH processes lines as they arrive; the arithmetic here is the same,
//! applied to whole tile-components: its reversible and irreversible lifting
//! (including the single-row and single-column cases), its quantisation and
//! step-size derivation, its block coder and its packet-header tag trees,
//! which read past the end of a row into the next one when a subband has an
//! odd number of code-blocks.

use crate::htj2k_encoder_tables::{TABLE0, TABLE1};

/// One image plane. `samples` are row-major, `width` x `height`, holding
/// unsigned values of `precision` bits.
pub struct Component<'a> {
    pub samples: &'a [u16],
    pub width: u32,
    pub height: u32,
    pub dx: u32,
    pub dy: u32,
    pub precision: u32,
}

/// Progression orders, numbered as in the COD marker.
pub const LRCP: u8 = 0;
pub const RLCP: u8 = 1;
pub const RPCL: u8 = 2;
pub const PCRL: u8 = 3;
pub const CPRL: u8 = 4;

pub struct Settings<'a> {
    pub reversible: bool,
    pub num_decompositions: u32,
    pub progression: u8,
    /// log2 of the nominal code-block width and height.
    pub log_block: (u32, u32),
    /// Tile size; (0, 0) means a single tile covering the image.
    pub tile_size: (u32, u32),
    pub tilepart_resolutions: bool,
    pub tilepart_components: bool,
    pub tlm: bool,
    /// Extra COM marker text; empty writes none.
    pub comment: &'a [u8],
}

/// Configurations for which OpenJPH raises an error (an exception that
/// escapes libheif's plugin) or relies on undefined behaviour.
#[derive(Debug, PartialEq, Eq)]
pub enum EncodeError {
    Unsupported,
}

const TILEPART_RESOLUTIONS: u32 = 1;
const TILEPART_COMPONENTS: u32 = 2;

#[derive(Clone, Copy, Default, Debug)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

// OpenJPH's sqrt_energy_gains and bibo_gains tables (ojph_params.cpp).
const GAIN_97_L: [f32; 34] = [
    1.0000e+00, 1.4021e+00, 2.0304e+00, 2.9012e+00, 4.1153e+00, 5.8245e+00, 8.2388e+00, 1.1652e+01,
    1.6479e+01, 2.3304e+01, 3.2957e+01, 4.6609e+01, 6.5915e+01, 9.3217e+01, 1.3183e+02, 1.8643e+02,
    2.6366e+02, 3.7287e+02, 5.2732e+02, 7.4574e+02, 1.0546e+03, 1.4915e+03, 2.1093e+03, 2.9830e+03,
    4.2185e+03, 5.9659e+03, 8.4371e+03, 1.1932e+04, 1.6874e+04, 2.3864e+04, 3.3748e+04, 4.7727e+04,
    6.7496e+04, 9.5454e+04,
];
const GAIN_97_H: [f32; 34] = [
    1.4425e+00, 1.9669e+00, 2.8839e+00, 4.1475e+00, 5.8946e+00, 8.3472e+00, 1.1809e+01, 1.6701e+01,
    2.3620e+01, 3.3403e+01, 4.7240e+01, 6.6807e+01, 9.4479e+01, 1.3361e+02, 1.8896e+02, 2.6723e+02,
    3.7792e+02, 5.3446e+02, 7.5583e+02, 1.0689e+03, 1.5117e+03, 2.1378e+03, 3.0233e+03, 4.2756e+03,
    6.0467e+03, 8.5513e+03, 1.2093e+04, 1.7103e+04, 2.4187e+04, 3.4205e+04, 4.8373e+04, 6.8410e+04,
    9.6747e+04, 1.3682e+05,
];
const BIBO_53_L: [f32; 34] = [
    1.0000e+00, 1.5000e+00, 1.6250e+00, 1.6875e+00, 1.6963e+00, 1.7067e+00, 1.7116e+00, 1.7129e+00,
    1.7141e+00, 1.7145e+00, 1.7151e+00, 1.7152e+00, 1.7155e+00, 1.7155e+00, 1.7156e+00, 1.7156e+00,
    1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00,
    1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00, 1.7156e+00,
    1.7156e+00, 1.7156e+00,
];
const BIBO_53_H: [f32; 34] = [
    2.0000e+00, 2.5000e+00, 2.7500e+00, 2.8047e+00, 2.8198e+00, 2.8410e+00, 2.8558e+00, 2.8601e+00,
    2.8628e+00, 2.8656e+00, 2.8662e+00, 2.8667e+00, 2.8669e+00, 2.8670e+00, 2.8671e+00, 2.8671e+00,
    2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00,
    2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00, 2.8671e+00,
    2.8671e+00, 2.8671e+00,
];

// param_atk::init_irv97, stored in OpenJPH's (reversed) step order.
#[allow(clippy::excessive_precision)]
const IRV_STEPS: [f32; 4] = [
    0.443506852043971,
    0.882911075530934,
    -0.052980118572961,
    -1.586134342059924,
];
#[allow(clippy::excessive_precision)]
const IRV_K: f32 = 1.230174104914001;

/// QCD/QCC parameters for one component (param_qcd).
#[derive(Clone, PartialEq)]
struct Quant {
    sqcd: u8,
    /// SPqcd: u8 values for reversible, u16 for irreversible.
    sp: Vec<u16>,
    bit_depth: u32,
}

impl Quant {
    fn new(reversible: bool, num_decomps: u32, bit_depth: u32) -> Result<Quant, EncodeError> {
        let n = (1 + 3 * num_decomps) as usize;
        let mut sp = vec![0u16; n];
        let sqcd;
        if reversible {
            // set_rev_quant
            let b = bit_depth;
            let x_of = |g: f64| (((g).ln() / std::f64::consts::LN_2).ceil()) as u32;
            let mut s = 0;
            let l = f64::from(BIBO_53_L[num_decomps as usize]);
            let x = x_of(l * l);
            let mut vals = vec![0u32; n];
            vals[s] = b + x;
            s += 1;
            let mut max_bx = b + x;
            for d in (1..=num_decomps).rev() {
                let l = f64::from(BIBO_53_L[d as usize]);
                let h = f64::from(BIBO_53_H[d as usize - 1]);
                let x = x_of(h * l);
                vals[s] = b + x;
                vals[s + 1] = b + x;
                max_bx = max_bx.max(b + x);
                let x = x_of(h * h);
                vals[s + 2] = b + x;
                max_bx = max_bx.max(b + x);
                s += 3;
            }
            if max_bx > 38 {
                return Err(EncodeError::Unsupported);
            }
            let guard = 1.max(max_bx as i32 - 31) as u32;
            sqcd = (guard << 5) as u8;
            for (o, v) in sp.iter_mut().zip(&vals) {
                *o = u16::from(((v - guard) << 3) as u8);
            }
        } else {
            // set_irrev_quant with no visual weights and the default step
            let delta_ref = 1.0f32 / (1u32 << bit_depth.min(16)) as f32;
            sqcd = (1 << 5) | 2;
            let gain_l = GAIN_97_L[num_decomps as usize];
            sp[0] = encode_step(delta_ref / (gain_l * gain_l * 1.0 * 1.0));
            let mut b = 1;
            for d in (1..=num_decomps).rev() {
                let gain_l = GAIN_97_L[d as usize];
                let gain_h = GAIN_97_H[d as usize - 1];
                sp[b] = encode_step(delta_ref / (gain_h * gain_l * 1.0 * 1.0));
                sp[b + 1] = encode_step(delta_ref / (gain_l * gain_h * 1.0 * 1.0));
                sp[b + 2] = encode_step(delta_ref / (gain_h * gain_h * 1.0 * 1.0));
                b += 3;
            }
        }
        Ok(Quant {
            sqcd,
            sp,
            bit_depth,
        })
    }

    fn reversible(&self) -> bool {
        self.sqcd & 0x1f == 0
    }

    fn guard(&self) -> u32 {
        u32::from(self.sqcd >> 5)
    }

    fn index(&self, res: u32, band: u32) -> usize {
        let idx = if res > 0 {
            ((res - 1) * 3 + band) as usize
        } else {
            0
        };
        idx.min(self.sp.len() - 1)
    }

    fn kmax_at(&self, i: usize) -> u32 {
        let bits = if self.reversible() {
            let t = u32::from(self.sp[i] >> 3);
            t.saturating_sub(1)
        } else {
            u32::from(self.sp[i] >> 11).wrapping_sub(1)
        };
        bits.wrapping_add(self.guard())
    }

    fn kmax(&self, res: u32, band: u32) -> u32 {
        self.kmax_at(self.index(res, band))
    }

    /// get_largest_Kmax: the maximum is taken before the guard bits are
    /// added, in unsigned arithmetic, so a wrapped exponent of 0 dominates.
    fn largest_kmax(&self) -> u32 {
        let mut num_bits = 0u32;
        for &v in &self.sp {
            let t = if self.reversible() {
                u32::from(v >> 3).saturating_sub(1)
            } else {
                u32::from(v >> 11).wrapping_sub(1)
            };
            num_bits = num_bits.max(t);
        }
        num_bits.wrapping_add(self.guard())
    }

    fn precision(&self) -> u32 {
        self.largest_kmax().wrapping_add(2)
    }

    /// get_irrev_delta
    fn delta(&self, res: u32, band: u32) -> f32 {
        let arr = [1.0f32, 2.0, 2.0, 4.0];
        let v = self.sp[self.index(res, band)];
        let eps = u32::from(v >> 11);
        let mut m = (u32::from(v & 0x7ff) | 0x800) as f32 * arr[band as usize];
        m /= (1u32 << 11) as f32;
        m /= (1u32 << (eps & 31)) as f32;
        m
    }

    /// Contribution to get_MAGB.
    fn magb(&self) -> u32 {
        let n = self.sp.len() as u32;
        let num_decomps = (n - 1) / 3;
        let mut b = 0u32;
        for i in 0..n {
            let t = if self.reversible() {
                u32::from(self.sp[i as usize] >> 3) + self.guard() - 1
            } else {
                let nb = num_decomps - if i > 0 { (i - 1) / 3 } else { 0 };
                (u32::from(self.sp[i as usize] >> 11) + self.guard()).wrapping_sub(nb)
            };
            b = b.max(t);
        }
        b
    }

    fn write(&self, out: &mut Vec<u8>, qcc: Option<(u16, usize)>) {
        let wide = !self.reversible();
        let n = self.sp.len();
        let payload = if wide { 2 * n } else { n };
        match qcc {
            None => {
                out.extend_from_slice(&[0xff, 0x5c]);
                out.extend_from_slice(&((3 + payload) as u16).to_be_bytes());
            }
            Some((comp, num_comps)) => {
                out.extend_from_slice(&[0xff, 0x5d]);
                let len = 4 + usize::from(num_comps >= 257) + payload;
                out.extend_from_slice(&(len as u16).to_be_bytes());
                if num_comps < 257 {
                    out.push(comp as u8);
                } else {
                    out.extend_from_slice(&comp.to_be_bytes());
                }
            }
        }
        out.push(self.sqcd);
        for &v in &self.sp {
            if wide {
                out.extend_from_slice(&v.to_be_bytes());
            } else {
                out.push(v as u8);
            }
        }
    }
}

/// param_qcd::encode_SPqcd for a float step.
fn encode_step(mut delta: f32) -> u16 {
    let mut exp = 0i32;
    while delta < 1.0 {
        exp += 1;
        delta *= 2.0;
    }
    let mut mantissa = (delta * (1 << 11) as f32).round() as i32 - (1 << 11);
    if mantissa >= 1 << 11 {
        mantissa = 0x7ff;
    }
    ((exp << 11) | mantissa) as u16
}

fn div_ceil(a: u32, b: u32) -> u32 {
    a.div_ceil(b)
}

/// x86 cvttss2si, as OpenJPH's (si32) cast compiles.
fn trunc_i32(v: f32) -> i32 {
    if v.is_nan() || !(-2147483648.0..2147483648.0).contains(&v) {
        i32::MIN
    } else {
        v as i32
    }
}

// ---------------------------------------------------------------------------
// Wavelet analysis

#[derive(Clone)]
enum Plane {
    Int(Vec<i64>),
    Float(Vec<f32>),
}

/// Reversible 5/3 lifting on split low/high arrays (gen_rev_horz_ana steps).
fn rev_lift(lo: &mut [i64], hi: &mut [i64], mut even: bool) {
    // Steps in OpenJPH order: predict (A=-1, B=1, E=1), then update (A=1, B=2, E=2).
    let (mut lp, mut hp): (&mut [i64], &mut [i64]) = (lo, hi);
    for step in 0..2 {
        let l_width = lp.len();
        let h_width = hp.len();
        let at = |i: isize| -> i64 {
            if i < 0 {
                lp_get(lp, 0)
            } else if i as usize >= l_width {
                lp_get(lp, l_width - 1)
            } else {
                lp_get(lp, i as usize)
            }
        };
        let off: isize = if even { 1 } else { 0 };
        let mut updates = Vec::with_capacity(h_width);
        for i in 0..h_width {
            let s = at(i as isize + off - 1) + at(i as isize + off);
            updates.push(s);
        }
        for (d, s) in hp.iter_mut().zip(updates) {
            if step == 0 {
                *d -= s >> 1;
            } else {
                *d += (2 + s) >> 2;
            }
        }
        std::mem::swap(&mut lp, &mut hp);
        even = !even;
    }
}

fn lp_get<T: Copy>(v: &[T], i: usize) -> T {
    v[i]
}

/// Irreversible 9/7 lifting on split arrays, without the K scaling.
fn irv_lift(lo: &mut [f32], hi: &mut [f32], mut even: bool) {
    let (mut lp, mut hp): (&mut [f32], &mut [f32]) = (lo, hi);
    for j in (0..4).rev() {
        let a = IRV_STEPS[j];
        let l_width = lp.len();
        let off: isize = if even { 1 } else { 0 };
        let at = |i: isize| -> f32 {
            if i < 0 {
                lp[0]
            } else if i as usize >= l_width {
                lp[l_width - 1]
            } else {
                lp[i as usize]
            }
        };
        let sums: Vec<f32> = (0..hp.len())
            .map(|i| at(i as isize + off - 1) + at(i as isize + off))
            .collect();
        for (d, s) in hp.iter_mut().zip(sums) {
            *d += a * s;
        }
        std::mem::swap(&mut lp, &mut hp);
        even = !even;
    }
}

fn split<T: Copy>(src: &[T], even: bool) -> (Vec<T>, Vec<T>) {
    let mut lo = Vec::with_capacity(src.len() / 2 + 1);
    let mut hi = Vec::with_capacity(src.len() / 2 + 1);
    for (i, &v) in src.iter().enumerate() {
        if (i % 2 == 0) == even {
            lo.push(v);
        } else {
            hi.push(v);
        }
    }
    (lo, hi)
}

/// gen_rev_horz_ana / gen_irv_horz_ana on one row.
fn horz_ana(row: &Plane, even: bool) -> (Plane, Plane) {
    match row {
        Plane::Int(r) => {
            if r.len() > 1 {
                let (mut lo, mut hi) = split(r, even);
                rev_lift(&mut lo, &mut hi, even);
                (Plane::Int(lo), Plane::Int(hi))
            } else if even {
                (Plane::Int(vec![r[0]]), Plane::Int(vec![]))
            } else {
                (Plane::Int(vec![]), Plane::Int(vec![r[0] << 1]))
            }
        }
        Plane::Float(r) => {
            if r.len() > 1 {
                let (mut lo, mut hi) = split(r, even);
                irv_lift(&mut lo, &mut hi, even);
                let k_inv = 1.0f32 / IRV_K;
                for v in &mut lo {
                    *v *= k_inv;
                }
                for v in &mut hi {
                    *v *= IRV_K;
                }
                (Plane::Float(lo), Plane::Float(hi))
            } else if even {
                (Plane::Float(vec![r[0]]), Plane::Float(vec![]))
            } else {
                (Plane::Float(vec![]), Plane::Float(vec![r[0] * 2.0]))
            }
        }
    }
}

impl Plane {
    fn row(&self, w: usize, y: usize) -> Plane {
        match self {
            Plane::Int(v) => Plane::Int(v[y * w..(y + 1) * w].to_vec()),
            Plane::Float(v) => Plane::Float(v[y * w..(y + 1) * w].to_vec()),
        }
    }

    fn empty_like(&self) -> Plane {
        match self {
            Plane::Int(_) => Plane::Int(Vec::new()),
            Plane::Float(_) => Plane::Float(Vec::new()),
        }
    }

    fn append(&mut self, other: Plane) {
        match (self, other) {
            (Plane::Int(a), Plane::Int(b)) => a.extend(b),
            (Plane::Float(a), Plane::Float(b)) => a.extend(b),
            _ => unreachable!(),
        }
    }
}

/// Vertical analysis over whole columns: lifting, then the K scaling OpenJPH
/// applies to each row before its horizontal transform (h > 1), or the
/// single-row doubling of an odd row.
fn vert_ana(plane: &mut Plane, w: usize, h: usize, even: bool) {
    if h > 1 {
        for x in 0..w {
            match plane {
                Plane::Int(v) => {
                    let col: Vec<i64> = (0..h).map(|y| v[y * w + x]).collect();
                    let (mut lo, mut hi) = split(&col, even);
                    rev_lift(&mut lo, &mut hi, even);
                    merge_col(v, w, x, h, even, &lo, &hi);
                }
                Plane::Float(v) => {
                    let col: Vec<f32> = (0..h).map(|y| v[y * w + x]).collect();
                    let (mut lo, mut hi) = split(&col, even);
                    irv_lift(&mut lo, &mut hi, even);
                    let k_inv = 1.0f32 / IRV_K;
                    for s in &mut lo {
                        *s *= k_inv;
                    }
                    for s in &mut hi {
                        *s *= IRV_K;
                    }
                    merge_col(v, w, x, h, even, &lo, &hi);
                }
            }
        }
    } else if h == 1 && !even {
        match plane {
            Plane::Int(v) => v.iter_mut().for_each(|s| *s <<= 1),
            Plane::Float(v) => v.iter_mut().for_each(|s| *s *= 2.0),
        }
    }
}

fn merge_col<T: Copy>(v: &mut [T], w: usize, x: usize, h: usize, even: bool, lo: &[T], hi: &[T]) {
    let (mut li, mut hi_i) = (0, 0);
    for y in 0..h {
        if (y % 2 == 0) == even {
            v[y * w + x] = lo[li];
            li += 1;
        } else {
            v[y * w + x] = hi[hi_i];
            hi_i += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Code-blocks

#[derive(Clone, Default)]
struct CodedBlock {
    data: Vec<u8>,
    missing_msbs: u32,
    num_passes: u32,
    pass_length: [u32; 2],
    coded: bool,
}

struct Band {
    empty: bool,
    num_blocks: (u32, u32),
    blocks: Vec<CodedBlock>,
}

impl Band {
    fn empty() -> Band {
        Band {
            empty: true,
            num_blocks: (0, 0),
            blocks: Vec::new(),
        }
    }
}

struct Precinct {
    img_point: (u32, u32),
    cb_idxs: [Rect; 4],
    packet: Vec<u8>,
}

struct Resolution {
    precincts: Vec<Precinct>,
    num_bytes: u32,
    cursor: usize,
}

/// Quantises and codes one subband (subband + codeblock).
#[allow(clippy::too_many_arguments)]
fn code_band(
    data: &Plane,
    rect: Rect,
    res_num: u32,
    band_num: u32,
    quant: &Quant,
    log_block: (u32, u32),
    transform: bool,
) -> Result<Band, EncodeError> {
    let kmax = quant.kmax(res_num, band_num);
    let precision = quant.precision();
    let wide = precision > 32;
    let reversible = quant.reversible();
    if !reversible && wide {
        // OpenJPH has no 64-bit irreversible path (a null function pointer).
        return Err(EncodeError::Unsupported);
    }
    if rect.w == 0 || rect.h == 0 {
        return Ok(Band::empty());
    }
    let (delta, delta_inv) = if reversible {
        (0.0, 0.0)
    } else {
        let d = quant.delta(res_num, band_num) / (1u32 << (31 - kmax)) as f32;
        (d, 1.0f32 / d)
    };
    let _ = delta;
    // log_PP is 15 (no precinct partition); with a transform it is 14 here.
    let pp = if transform { 14 } else { 15 };
    let xcb = log_block.0.min(pp);
    let ycb = log_block.1.min(pp);
    let (tbx0, tby0, tbx1, tby1) = (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h);
    let nbw = ((tbx1 + (1 << xcb) - 1) >> xcb) - (tbx0 >> xcb);
    let nbh = ((tby1 + (1 << ycb) - 1) >> ycb) - (tby0 >> ycb);
    let xlb = (tbx0 >> xcb) << xcb;
    let ylb = (tby0 >> ycb) << ycb;
    let bits = if wide { 64 } else { 32 };
    let mut blocks = Vec::with_capacity((nbw * nbh) as usize);
    let w = rect.w as usize;
    for by in 0..nbh {
        let cby0 = tby0.max(ylb + by * (1 << ycb));
        let cby1 = tby1.min(ylb + (by + 1) * (1 << ycb));
        for bx in 0..nbw {
            let cbx0 = tbx0.max(xlb + bx * (1 << xcb));
            let cbx1 = tbx1.min(xlb + (bx + 1) * (1 << xcb));
            let (cw, ch) = ((cbx1 - cbx0) as usize, (cby1 - cby0) as usize);
            // sign-magnitude words in OpenJPH's 32- or 64-bit layout
            let mut words = vec![0u64; cw * ch];
            let mut max_val = 0u64;
            for y in 0..ch {
                let row = (cby0 - tby0) as usize + y;
                for x in 0..cw {
                    let col = (cbx0 - tbx0) as usize + x;
                    let (sign, mag) = match data {
                        Plane::Int(v) => {
                            let s = v[row * w + col];
                            let m = s.unsigned_abs();
                            // val <<= shift in the word width; a magnitude
                            // reaching the sign bit is kept as OpenJPH does
                            let shifted = if wide {
                                m << (63 - kmax)
                            } else {
                                u64::from((m as u32) << (31 - kmax))
                            };
                            (u64::from(s < 0), shifted)
                        }
                        Plane::Float(v) => {
                            let t = trunc_i32(v[row * w + col] * delta_inv);
                            (u64::from(t < 0), u64::from(t.unsigned_abs()))
                        }
                    };
                    words[y * cw + x] = (sign << (bits - 1)) | mag;
                    max_val |= mag;
                }
            }
            let threshold = if wide {
                1u64 << (63 - kmax)
            } else {
                1u64 << (31 - kmax)
            };
            let mut cb = CodedBlock::default();
            if max_val >= threshold {
                cb.missing_msbs = kmax.wrapping_sub(1);
                cb.num_passes = 1;
                cb.data = encode_block(&words, cb.missing_msbs, cw, ch, bits)?;
                cb.pass_length[0] = cb.data.len() as u32;
                cb.coded = true;
            }
            blocks.push(cb);
        }
    }
    Ok(Band {
        empty: false,
        num_blocks: (nbw, nbh),
        blocks,
    })
}

// ---------------------------------------------------------------------------
// HT block encoder (ojph_block_encoder.cpp)

struct VlcTables {
    t0: [u16; 2048],
    t1: [u16; 2048],
}

fn build_vlc(src: &[[u8; 7]]) -> [u16; 2048] {
    let mut tgt = [0u16; 2048];
    for (i, t) in tgt.iter_mut().enumerate() {
        let c_q = (i >> 8) as u8;
        let rho = ((i >> 4) & 0xf) as u8;
        let emb = (i & 0xf) as u8;
        if (emb & rho) != emb || (rho == 0 && c_q == 0) {
            continue;
        }
        let mut best: Option<&[u8; 7]> = None;
        if emb != 0 {
            let mut best_e_k = -1i32;
            for e in src {
                if e[0] == c_q && e[1] == rho && e[2] == 1 && (emb & e[3]) == e[4] {
                    let ones = e[3].count_ones() as i32;
                    if ones >= best_e_k {
                        best = Some(e);
                        best_e_k = ones;
                    }
                }
            }
        } else {
            best = src.iter().find(|e| e[0] == c_q && e[1] == rho && e[2] == 0);
        }
        let e = best.expect("VLC table entry");
        *t = (u16::from(e[5]) << 8) + (u16::from(e[6]) << 4) + u16::from(e[3]);
    }
    tgt
}

fn vlc_tables() -> &'static VlcTables {
    static TABLES: std::sync::OnceLock<VlcTables> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| VlcTables {
        t0: build_vlc(&TABLE0),
        t1: build_vlc(&TABLE1),
    })
}

#[derive(Clone, Copy)]
struct Uvlc {
    pre: u32,
    pre_len: u32,
    suf: u32,
    suf_len: u32,
}

fn uvlc(i: usize) -> Uvlc {
    match i {
        0 => Uvlc {
            pre: 0,
            pre_len: 0,
            suf: 0,
            suf_len: 0,
        },
        1 => Uvlc {
            pre: 1,
            pre_len: 1,
            suf: 0,
            suf_len: 0,
        },
        2 => Uvlc {
            pre: 2,
            pre_len: 2,
            suf: 0,
            suf_len: 0,
        },
        3 => Uvlc {
            pre: 4,
            pre_len: 3,
            suf: 0,
            suf_len: 1,
        },
        4 => Uvlc {
            pre: 4,
            pre_len: 3,
            suf: 1,
            suf_len: 1,
        },
        5..=32 => Uvlc {
            pre: 0,
            pre_len: 3,
            suf: (i - 5) as u32,
            suf_len: 5,
        },
        _ => Uvlc {
            pre: 0,
            pre_len: 3,
            suf: (28 + (i - 33) % 4) as u32,
            suf_len: 5,
        },
    }
}

const MEL_SIZE: usize = 192;
const VLC_SIZE: usize = 3072 - MEL_SIZE;

struct Mel {
    buf: Vec<u8>,
    remaining_bits: i32,
    tmp: i32,
    run: i32,
    k: i32,
    threshold: i32,
}

const MEL_EXP: [i32; 13] = [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 4, 5];

impl Mel {
    fn new() -> Mel {
        Mel {
            buf: Vec::new(),
            remaining_bits: 8,
            tmp: 0,
            run: 0,
            k: 0,
            threshold: 1,
        }
    }

    fn emit_bit(&mut self, v: i32) -> Result<(), EncodeError> {
        self.tmp = (self.tmp << 1) + v;
        self.remaining_bits -= 1;
        if self.remaining_bits == 0 {
            if self.buf.len() >= MEL_SIZE {
                return Err(EncodeError::Unsupported);
            }
            self.buf.push(self.tmp as u8);
            self.remaining_bits = if self.tmp == 0xff { 7 } else { 8 };
            self.tmp = 0;
        }
        Ok(())
    }

    fn encode(&mut self, bit: bool) -> Result<(), EncodeError> {
        if !bit {
            self.run += 1;
            if self.run >= self.threshold {
                self.emit_bit(1)?;
                self.run = 0;
                self.k = (self.k + 1).min(12);
                self.threshold = 1 << MEL_EXP[self.k as usize];
            }
        } else {
            self.emit_bit(0)?;
            let mut t = MEL_EXP[self.k as usize];
            while t > 0 {
                t -= 1;
                self.emit_bit((self.run >> t) & 1)?;
            }
            self.run = 0;
            self.k = (self.k - 1).max(0);
            self.threshold = 1 << MEL_EXP[self.k as usize];
        }
        Ok(())
    }
}

struct Vlc {
    /// Bytes in writing order (the codestream holds them reversed).
    buf: Vec<u8>,
    used_bits: i32,
    tmp: i32,
    last_greater_than_8f: bool,
}

impl Vlc {
    fn new() -> Vlc {
        Vlc {
            buf: vec![0xff],
            used_bits: 4,
            tmp: 0xf,
            last_greater_than_8f: true,
        }
    }

    fn pos(&self) -> usize {
        self.buf.len()
    }

    fn encode(&mut self, mut cwd: i32, mut cwd_len: i32) -> Result<(), EncodeError> {
        while cwd_len > 0 {
            if self.pos() >= VLC_SIZE {
                return Err(EncodeError::Unsupported);
            }
            let mut avail = 8 - i32::from(self.last_greater_than_8f) - self.used_bits;
            let t = avail.min(cwd_len);
            self.tmp |= (cwd & ((1 << t) - 1)) << self.used_bits;
            self.used_bits += t;
            avail -= t;
            cwd_len -= t;
            cwd >>= t;
            if avail == 0 {
                if self.last_greater_than_8f && self.tmp != 0x7f {
                    self.last_greater_than_8f = false;
                    continue;
                }
                self.buf.push(self.tmp as u8);
                self.last_greater_than_8f = self.tmp > 0x8f;
                self.tmp = 0;
                self.used_bits = 0;
            }
        }
        Ok(())
    }
}

struct Ms {
    buf: Vec<u8>,
    max_bits: i32,
    used_bits: i32,
    tmp: u32,
}

impl Ms {
    fn new() -> Ms {
        Ms {
            buf: Vec::new(),
            max_bits: 8,
            used_bits: 0,
            tmp: 0,
        }
    }

    fn encode(&mut self, mut cwd: u64, mut cwd_len: i32) {
        while cwd_len > 0 {
            let t = (self.max_bits - self.used_bits).min(cwd_len);
            self.tmp |= ((cwd & ((1u64 << t) - 1)) << self.used_bits) as u32;
            self.used_bits += t;
            cwd >>= t;
            cwd_len -= t;
            if self.used_bits >= self.max_bits {
                self.buf.push(self.tmp as u8);
                self.max_bits = if self.tmp == 0xff { 7 } else { 8 };
                self.tmp = 0;
                self.used_bits = 0;
            }
        }
    }

    fn terminate(&mut self) {
        if self.used_bits != 0 {
            let t = self.max_bits - self.used_bits;
            self.tmp |= (0xff & ((1u32 << t) - 1)) << self.used_bits;
            self.used_bits += t;
            if self.tmp != 0xff {
                self.buf.push(self.tmp as u8);
            }
        } else if self.max_bits == 7 {
            self.buf.pop();
        }
    }
}

fn terminate_mel_vlc(mel: &mut Mel, vlc: &mut Vlc) -> Result<(), EncodeError> {
    if mel.run > 0 {
        mel.emit_bit(1)?;
    }
    mel.tmp <<= mel.remaining_bits;
    let mel_mask = (0xff << mel.remaining_bits) & 0xff;
    let vlc_mask = 0xff >> (8 - vlc.used_bits);
    if (mel_mask | vlc_mask) == 0 {
        return Ok(());
    }
    if mel.buf.len() >= MEL_SIZE {
        return Err(EncodeError::Unsupported);
    }
    let fuse = mel.tmp | vlc.tmp;
    if (((fuse ^ mel.tmp) & mel_mask) | ((fuse ^ vlc.tmp) & vlc_mask)) == 0
        && fuse != 0xff
        && vlc.pos() > 1
    {
        mel.buf.push(fuse as u8);
    } else {
        if vlc.pos() >= VLC_SIZE {
            return Err(EncodeError::Unsupported);
        }
        mel.buf.push(mel.tmp as u8);
        vlc.buf.push(vlc.tmp as u8);
    }
    Ok(())
}

/// ojph_encode_codeblock32/64 with one cleanup pass over `bits`-wide
/// sign-magnitude words.
fn encode_block(
    words: &[u64],
    missing_msbs: u32,
    width: usize,
    height: usize,
    bits: u32,
) -> Result<Vec<u8>, EncodeError> {
    let tables = vlc_tables();
    let mut mel = Mel::new();
    let mut vlc = Vlc::new();
    let mut ms = Ms::new();
    // x86 masks the shift count to the word width
    let p = (bits - 2).wrapping_sub(missing_msbs) & (bits - 1);
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    // (e_q, s) for one sample: t + t drops the sign bit in the word width
    let prep = |x: usize, y: usize| -> Option<(i32, u64)> {
        let t = if x < width && y < height {
            words[y * width + x]
        } else {
            0
        };
        let sign = t >> (bits - 1);
        let mut val = (t.wrapping_add(t) & mask) >> p;
        val &= !1u64;
        if val != 0 {
            val -= 1;
            let e = 64 - val.leading_zeros() as i32;
            val -= 1;
            Some((e, val + sign))
        } else {
            None
        }
    };
    let mut e_val = vec![0u8; width / 2 + 3];
    let mut cx_val = vec![0u8; width / 2 + 3];

    // Loads one quad at column x (rows y, y+1); returns (rho, e_q[4], s[4], e_qmax).
    let quad = |x: usize, y: usize| -> (i32, [i32; 4], [u64; 4], i32) {
        let mut rho = 0;
        let mut e_q = [0i32; 4];
        let mut s = [0u64; 4];
        let mut e_qmax = 0;
        let positions = [(x, y), (x, y + 1), (x + 1, y), (x + 1, y + 1)];
        for (n, &(px, py)) in positions.iter().enumerate() {
            if n >= 2 && px >= width {
                break;
            }
            if let Some((e, v)) = prep(px, py) {
                rho |= 1 << n;
                e_q[n] = e;
                s[n] = v;
                e_qmax = e_qmax.max(e);
            }
        }
        (rho, e_q, s, e_qmax)
    };

    let emit_ms = |ms: &mut Ms, rho: i32, s: &[u64; 4], uq: i32, tuple: u16| {
        for (n, &sn) in s.iter().enumerate() {
            let m = if rho & (1 << n) != 0 {
                uq - i32::from((tuple >> n) & 1)
            } else {
                0
            };
            let mask = if m >= 64 { u64::MAX } else { (1u64 << m) - 1 };
            ms.encode(sn & mask, m);
        }
    };
    let eps_of = |e_q: &[i32; 4], e_qmax: i32| -> usize {
        let mut eps = 0;
        for (n, &e) in e_q.iter().enumerate() {
            eps |= usize::from(e == e_qmax) << n;
        }
        eps
    };

    // initial row of quads
    let mut lep = 0usize;
    let mut lcxp = 0usize;
    e_val[0] = 0;
    cx_val[0] = 0;
    let mut c_q0 = 0usize;
    let mut x = 0;
    while x < width {
        let (rho0, eq0, s0, eqmax0) = quad(x, 0);
        let uq0 = eqmax0.max(1);
        let u_q0 = uq0 - 1;
        let mut u_q1 = 0;
        let eps0 = if u_q0 > 0 { eps_of(&eq0, eqmax0) } else { 0 };
        e_val[lep] = e_val[lep].max(eq0[1] as u8);
        lep += 1;
        e_val[lep] = eq0[3] as u8;
        cx_val[lcxp] |= ((rho0 & 2) >> 1) as u8;
        lcxp += 1;
        cx_val[lcxp] = ((rho0 & 8) >> 3) as u8;
        let tuple0 = tables.t0[(c_q0 << 8) + ((rho0 as usize) << 4) + eps0];
        vlc.encode(i32::from(tuple0 >> 8), i32::from((tuple0 >> 4) & 7))?;
        if c_q0 == 0 {
            mel.encode(rho0 != 0)?;
        }
        emit_ms(&mut ms, rho0, &s0, uq0, tuple0);
        let mut rho1 = 0;
        if x + 2 < width {
            let (r1, eq1, s1, eqmax1) = quad(x + 2, 0);
            rho1 = r1;
            let c_q1 = ((rho0 >> 1) | (rho0 & 1)) as usize;
            let uq1 = eqmax1.max(1);
            u_q1 = uq1 - 1;
            let eps1 = if u_q1 > 0 { eps_of(&eq1, eqmax1) } else { 0 };
            e_val[lep] = e_val[lep].max(eq1[1] as u8);
            lep += 1;
            e_val[lep] = eq1[3] as u8;
            cx_val[lcxp] |= ((rho1 & 2) >> 1) as u8;
            lcxp += 1;
            cx_val[lcxp] = ((rho1 & 8) >> 3) as u8;
            let tuple1 = tables.t0[(c_q1 << 8) + ((rho1 as usize) << 4) + eps1];
            vlc.encode(i32::from(tuple1 >> 8), i32::from((tuple1 >> 4) & 7))?;
            if c_q1 == 0 {
                mel.encode(rho1 != 0)?;
            }
            emit_ms(&mut ms, rho1, &s1, uq1, tuple1);
        }
        if u_q0 > 0 && u_q1 > 0 {
            mel.encode(u_q0.min(u_q1) > 2)?;
        }
        if u_q0 > 2 && u_q1 > 2 {
            let a = uvlc((u_q0 - 2) as usize);
            let b = uvlc((u_q1 - 2) as usize);
            vlc.encode(a.pre as i32, a.pre_len as i32)?;
            vlc.encode(b.pre as i32, b.pre_len as i32)?;
            vlc.encode(a.suf as i32, a.suf_len as i32)?;
            vlc.encode(b.suf as i32, b.suf_len as i32)?;
        } else if u_q0 > 2 && u_q1 > 0 {
            let a = uvlc(u_q0 as usize);
            vlc.encode(a.pre as i32, a.pre_len as i32)?;
            vlc.encode(u_q1 - 1, 1)?;
            vlc.encode(a.suf as i32, a.suf_len as i32)?;
        } else {
            let a = uvlc(u_q0 as usize);
            let b = uvlc(u_q1 as usize);
            vlc.encode(a.pre as i32, a.pre_len as i32)?;
            vlc.encode(b.pre as i32, b.pre_len as i32)?;
            vlc.encode(a.suf as i32, a.suf_len as i32)?;
            vlc.encode(b.suf as i32, b.suf_len as i32)?;
        }
        c_q0 = ((rho1 >> 1) | (rho1 & 1)) as usize;
        x += 4;
    }
    e_val[lep + 1] = 0;

    let mut y = 2;
    while y < height {
        lep = 0;
        let mut max_e = i32::from(e_val[0].max(e_val[1])) - 1;
        e_val[0] = 0;
        lcxp = 0;
        let mut c_q0 = usize::from(cx_val[0]) + (usize::from(cx_val[1]) << 2);
        cx_val[0] = 0;
        let mut x = 0;
        while x < width {
            let (rho0, eq0, s0, eqmax0) = quad(x, y);
            let kappa = if rho0 & (rho0 - 1) != 0 {
                1.max(max_e)
            } else {
                1
            };
            let uq0 = eqmax0.max(kappa);
            let u_q0 = uq0 - kappa;
            let mut u_q1 = 0;
            let eps0 = if u_q0 > 0 { eps_of(&eq0, eqmax0) } else { 0 };
            e_val[lep] = e_val[lep].max(eq0[1] as u8);
            lep += 1;
            max_e = i32::from(e_val[lep].max(e_val[lep + 1])) - 1;
            e_val[lep] = eq0[3] as u8;
            cx_val[lcxp] |= ((rho0 & 2) >> 1) as u8;
            lcxp += 1;
            let mut c_q1 = usize::from(cx_val[lcxp]) + (usize::from(cx_val[lcxp + 1]) << 2);
            cx_val[lcxp] = ((rho0 & 8) >> 3) as u8;
            let tuple0 = tables.t1[(c_q0 << 8) + ((rho0 as usize) << 4) + eps0];
            vlc.encode(i32::from(tuple0 >> 8), i32::from((tuple0 >> 4) & 7))?;
            if c_q0 == 0 {
                mel.encode(rho0 != 0)?;
            }
            emit_ms(&mut ms, rho0, &s0, uq0, tuple0);
            let mut rho1 = 0;
            if x + 2 < width {
                let (r1, eq1, s1, eqmax1) = quad(x + 2, y);
                rho1 = r1;
                let kappa = if rho1 & (rho1 - 1) != 0 {
                    1.max(max_e)
                } else {
                    1
                };
                c_q1 |= (((rho0 & 4) >> 1) | ((rho0 & 8) >> 2)) as usize;
                let uq1 = eqmax1.max(kappa);
                u_q1 = uq1 - kappa;
                let eps1 = if u_q1 > 0 { eps_of(&eq1, eqmax1) } else { 0 };
                e_val[lep] = e_val[lep].max(eq1[1] as u8);
                lep += 1;
                max_e = i32::from(e_val[lep].max(e_val[lep + 1])) - 1;
                e_val[lep] = eq1[3] as u8;
                cx_val[lcxp] |= ((rho1 & 2) >> 1) as u8;
                lcxp += 1;
                c_q0 = usize::from(cx_val[lcxp]) + (usize::from(cx_val[lcxp + 1]) << 2);
                cx_val[lcxp] = ((rho1 & 8) >> 3) as u8;
                let tuple1 = tables.t1[(c_q1 << 8) + ((rho1 as usize) << 4) + eps1];
                vlc.encode(i32::from(tuple1 >> 8), i32::from((tuple1 >> 4) & 7))?;
                if c_q1 == 0 {
                    mel.encode(rho1 != 0)?;
                }
                emit_ms(&mut ms, rho1, &s1, uq1, tuple1);
            }
            let a = uvlc(u_q0 as usize);
            let b = uvlc(u_q1 as usize);
            vlc.encode(a.pre as i32, a.pre_len as i32)?;
            vlc.encode(b.pre as i32, b.pre_len as i32)?;
            vlc.encode(a.suf as i32, a.suf_len as i32)?;
            vlc.encode(b.suf as i32, b.suf_len as i32)?;
            c_q0 |= (((rho1 & 4) >> 1) | ((rho1 & 8) >> 2)) as usize;
            x += 4;
        }
        y += 2;
    }

    terminate_mel_vlc(&mut mel, &mut vlc)?;
    ms.terminate();
    let mut out = Vec::with_capacity(ms.buf.len() + mel.buf.len() + vlc.buf.len());
    out.extend_from_slice(&ms.buf);
    out.extend_from_slice(&mel.buf);
    out.extend(vlc.buf.iter().rev());
    let len = out.len();
    let num_bytes = (mel.buf.len() + vlc.buf.len()) as u32;
    out[len - 1] = (num_bytes >> 4) as u8;
    out[len - 2] = (out[len - 2] & 0xf0) | (num_bytes & 0xf) as u8;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Packet headers (precinct::prepare_precinct)

struct BitWriter {
    out: Vec<u8>,
    avail_bits: i32,
    tmp: u32,
}

impl BitWriter {
    fn new() -> BitWriter {
        BitWriter {
            out: Vec::new(),
            avail_bits: 8,
            tmp: 0,
        }
    }

    fn put_bit(&mut self, bit: u32) {
        self.avail_bits -= 1;
        self.tmp |= (bit & 1) << self.avail_bits;
        if self.avail_bits <= 0 {
            self.avail_bits = 8 - i32::from(self.tmp == 0xff);
            self.out.push(self.tmp as u8);
            self.tmp = 0;
        }
    }

    fn put_bits(&mut self, data: u32, num_bits: i32) {
        for i in (0..num_bits).rev() {
            self.put_bit(data >> i);
        }
    }

    fn terminate(&mut self) {
        if self.avail_bits < 8 {
            self.out.push(self.tmp as u8);
        }
    }
}

/// A tag tree stored as OpenJPH does: level l is a row-major array with row
/// width ceil(w / 2^l), in a square buffer of side 2^(levels - 1 - l).
struct TagTree {
    levels: Vec<Vec<u8>>,
    width: u32,
}

impl TagTree {
    fn new(num_levels: u32, w: u32, init: u8) -> TagTree {
        let mut levels = Vec::with_capacity(num_levels as usize + 1);
        for i in 0..num_levels {
            let side = 1usize << (num_levels - 1 - i);
            levels.push(vec![init; side * side]);
        }
        levels.push(vec![0u8; 1]);
        TagTree { levels, width: w }
    }

    fn idx(&self, x: u32, y: u32, lev: u32) -> usize {
        (x + y * ((self.width + (1 << lev) - 1) >> lev)) as usize
    }

    fn get(&self, x: u32, y: u32, lev: u32) -> u8 {
        let i = self.idx(x, y, lev);
        self.levels[lev as usize].get(i).copied().unwrap_or(0xff)
    }

    fn set(&mut self, x: u32, y: u32, lev: u32, v: u8) {
        let i = self.idx(x, y, lev);
        self.levels[lev as usize][i] = v;
    }
}

fn log2ceil(x: u32) -> u32 {
    let t = 31 - x.leading_zeros();
    t + u32::from(x & (x - 1) != 0)
}

fn prepare_precinct(bands: &[Band; 4], cb_idxs: &[Rect; 4]) -> Vec<u8> {
    let mut bb = BitWriter::new();
    let mut coded = false;
    let mut num_skipped = 0;
    let mut cb_data: Vec<&[u8]> = Vec::new();
    for s in 0..4 {
        let band = &bands[s];
        let idx = cb_idxs[s];
        if band.empty || idx.w == 0 || idx.h == 0 {
            continue;
        }
        let num_levels = 1 + log2ceil(idx.w).max(log2ceil(idx.h));
        let mut inc = TagTree::new(num_levels, idx.w, 255);
        let mut inc_flags = TagTree::new(num_levels, idx.w, 0);
        let mut mmsb = TagTree::new(num_levels, idx.w, 255);
        let mut mmsb_flags = TagTree::new(num_levels, idx.w, 0);
        let bw = band.num_blocks.0;
        let block = |x: u32, y: u32| &band.blocks[((idx.y + y) * bw + idx.x + x) as usize];
        for y in 0..idx.h {
            for x in 0..idx.w {
                let b = block(x, y);
                inc.set(x, y, 0, u8::from(!b.coded));
                mmsb.set(x, y, 0, b.missing_msbs as u8);
            }
        }
        for lev in 1..num_levels {
            let h = (idx.h + (1 << lev) - 1) >> lev;
            let w = (idx.w + (1 << lev) - 1) >> lev;
            for y in 0..h {
                for x in 0..w {
                    for tree in [&mut inc, &mut mmsb] {
                        let t1 = tree.get(x << 1, y << 1, lev - 1).min(tree.get(
                            (x << 1) + 1,
                            y << 1,
                            lev - 1,
                        ));
                        let t2 = tree.get(x << 1, (y << 1) + 1, lev - 1).min(tree.get(
                            (x << 1) + 1,
                            (y << 1) + 1,
                            lev - 1,
                        ));
                        tree.set(x, y, lev, t1.min(t2));
                    }
                    inc_flags.set(x, y, lev, 0);
                    mmsb_flags.set(x, y, lev, 0);
                }
            }
        }
        inc.set(0, 0, num_levels, 0);
        inc_flags.set(0, 0, num_levels, 0);
        mmsb.set(0, 0, num_levels, 0);
        mmsb_flags.set(0, 0, num_levels, 0);
        if inc.get(0, 0, num_levels - 1) != 0 {
            if coded {
                bb.put_bits(0, 1);
            } else {
                num_skipped += 1;
            }
            continue;
        }
        if !coded {
            coded = true;
            bb.put_bit(1);
            bb.put_bits(0, num_skipped);
        }
        for y in 0..idx.h {
            for x in 0..idx.w {
                let b = block(x, y);
                for cur in (1..=num_levels).rev() {
                    let l = cur - 1;
                    if inc_flags.get(x >> l, y >> l, l) == 0 {
                        let skipped = inc.get(x >> l, y >> l, l).wrapping_sub(inc.get(
                            x >> cur,
                            y >> cur,
                            cur,
                        ));
                        bb.put_bits(1u32.wrapping_sub(u32::from(skipped)), 1);
                        inc_flags.set(x >> l, y >> l, l, 1);
                    }
                    if inc.get(x >> l, y >> l, l) > 0 {
                        break;
                    }
                }
                if b.num_passes == 0 {
                    continue;
                }
                for cur in (1..=num_levels).rev() {
                    let l = cur - 1;
                    if mmsb_flags.get(x >> l, y >> l, l) == 0 {
                        let zeros = i32::from(mmsb.get(x >> l, y >> l, l))
                            - i32::from(mmsb.get(x >> cur, y >> cur, cur));
                        for _ in 0..zeros.max(0) {
                            bb.put_bit(0);
                        }
                        bb.put_bits(1, 1);
                        mmsb_flags.set(x >> l, y >> l, l, 1);
                    }
                }
                match b.num_passes {
                    3 => bb.put_bits(12, 4),
                    2 => bb.put_bits(2, 2),
                    _ => bb.put_bits(0, 1),
                }
                let bits1 = 32 - b.pass_length[0].leading_zeros() as i32;
                let extra = i32::from(b.num_passes > 2);
                let bits2 = if b.num_passes > 1 {
                    32 - b.pass_length[1].leading_zeros() as i32
                } else {
                    0
                };
                let bits = (bits1.max(bits2 - extra) - 3).max(0);
                bb.put_bits(0xffff_fffe, bits + 1);
                bb.put_bits(b.pass_length[0], bits + 3);
                if b.num_passes > 1 {
                    bb.put_bits(b.pass_length[1], bits + 3 + extra);
                }
            }
        }
    }
    if !coded {
        return vec![0];
    }
    bb.terminate();
    let mut packet = bb.out;
    for s in 0..4 {
        let band = &bands[s];
        let idx = cb_idxs[s];
        if band.empty {
            continue;
        }
        for y in 0..idx.h {
            for x in 0..idx.w {
                cb_data.push(
                    &band.blocks[((idx.y + y) * band.num_blocks.0 + idx.x + x) as usize].data,
                );
            }
        }
    }
    for d in cb_data {
        packet.extend_from_slice(d);
    }
    packet
}

// ---------------------------------------------------------------------------
// Tile-components

struct TileComp {
    /// Resolutions indexed by resolution number (0 = lowest).
    resolutions: Vec<Resolution>,
    num_bytes: u32,
}

impl TileComp {
    fn bytes(&self, r: u32) -> u32 {
        self.resolutions.get(r as usize).map_or(0, |x| x.num_bytes)
    }

    fn top_left(&self, r: u32) -> Option<(u32, u32)> {
        let res = self.resolutions.get(r as usize)?;
        res.precincts.get(res.cursor).map(|p| p.img_point)
    }

    fn write_one(&mut self, r: u32, out: &mut Vec<u8>) {
        if let Some(res) = self.resolutions.get_mut(r as usize) {
            out.extend_from_slice(&res.precincts[res.cursor].packet);
            res.cursor += 1;
        }
    }

    fn write_all(&self, r: u32, out: &mut Vec<u8>) {
        if let Some(res) = self.resolutions.get(r as usize) {
            for p in &res.precincts {
                out.extend_from_slice(&p.packet);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn code_tile_comp(
    comp: &Component,
    tile: Rect,
    quant: &Quant,
    settings: &Settings,
) -> Result<TileComp, EncodeError> {
    let (dx, dy) = (comp.dx, comp.dy);
    let x0 = div_ceil(tile.x, dx);
    let y0 = div_ceil(tile.y, dy);
    let x1 = div_ceil(tile.x + tile.w, dx);
    let y1 = div_ceil(tile.y + tile.h, dy);
    let rect = Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    };
    let w = rect.w as usize;
    let h = rect.h as usize;
    let depth = comp.precision;
    let half = 1i64 << (depth - 1);
    let mut plane = if settings.reversible {
        let mut v = Vec::with_capacity(w * h);
        for y in 0..h {
            let row = (y0 as usize + y) * comp.width as usize;
            for x in 0..w {
                v.push(i64::from(comp.samples[row + x0 as usize + x]) - half);
            }
        }
        Plane::Int(v)
    } else {
        let mul = (1.0f64 / (1u64 << depth) as f64) as f32;
        let mut v = Vec::with_capacity(w * h);
        for y in 0..h {
            let row = (y0 as usize + y) * comp.width as usize;
            for x in 0..w {
                let s = i32::from(comp.samples[row + x0 as usize + x]) - half as i32;
                v.push(s as f32 * mul);
            }
        }
        Plane::Float(v)
    };

    let nd = settings.num_decompositions;
    let mut resolutions: Vec<Option<Resolution>> = (0..=nd).map(|_| None).collect();
    let mut cur_rect = rect;
    let mut downsamp = (dx, dy);
    for res_num in (0..=nd).rev() {
        let (trx0, try0) = (cur_rect.x, cur_rect.y);
        let (trx1, try1) = (cur_rect.x + cur_rect.w, cur_rect.y + cur_rect.h);
        let mut bands = [Band::empty(), Band::empty(), Band::empty(), Band::empty()];
        let mut band_rects = [Rect::default(); 4];
        let next_rect;
        let next_plane;
        if res_num > 0 {
            for (i, br) in band_rects.iter_mut().enumerate() {
                let i = i as u32;
                let bx0 = (trx0 + 1 - (i & 1)) >> 1;
                let bx1 = (trx1 + 1 - (i & 1)) >> 1;
                let by0 = (try0 + 1 - (i >> 1)) >> 1;
                let by1 = (try1 + 1 - (i >> 1)) >> 1;
                *br = Rect {
                    x: bx0,
                    y: by0,
                    w: bx1 - bx0,
                    h: by1 - by0,
                };
            }
            next_rect = band_rects[0];
            let (cw, ch) = (cur_rect.w as usize, cur_rect.h as usize);
            if cw == 0 || ch == 0 {
                next_plane = plane.empty_like();
                for b in 1..4 {
                    bands[b] = code_band(
                        &plane.empty_like(),
                        band_rects[b],
                        res_num,
                        b as u32,
                        quant,
                        settings.log_block,
                        true,
                    )?;
                }
            } else {
                let vert_even = try0 & 1 == 0;
                let horz_even = trx0 & 1 == 0;
                vert_ana(&mut plane, cw, ch, vert_even);
                let mut ll = plane.empty_like();
                let mut hl = plane.empty_like();
                let mut lh = plane.empty_like();
                let mut hh = plane.empty_like();
                for y in 0..ch {
                    let (lo, hi) = horz_ana(&plane.row(cw, y), horz_even);
                    let low_row = ((try0 as usize + y) & 1) == 0;
                    if low_row {
                        ll.append(lo);
                        hl.append(hi);
                    } else {
                        lh.append(lo);
                        hh.append(hi);
                    }
                }
                bands[1] = code_band(
                    &hl,
                    band_rects[1],
                    res_num,
                    1,
                    quant,
                    settings.log_block,
                    true,
                )?;
                bands[2] = code_band(
                    &lh,
                    band_rects[2],
                    res_num,
                    2,
                    quant,
                    settings.log_block,
                    true,
                )?;
                bands[3] = code_band(
                    &hh,
                    band_rects[3],
                    res_num,
                    3,
                    quant,
                    settings.log_block,
                    true,
                )?;
                next_plane = ll;
            }
        } else {
            bands[0] = code_band(&plane, cur_rect, 0, 0, quant, settings.log_block, false)?;
            next_rect = Rect::default();
            next_plane = plane.empty_like();
        }

        // precincts (log_PP = 15)
        let log_pp = 15u32;
        let mut precincts = Vec::new();
        if trx0 != trx1 && try0 != try1 {
            let npw = ((trx1 + (1 << log_pp) - 1) >> log_pp) - (trx0 >> log_pp);
            let nph = ((try1 + (1 << log_pp) - 1) >> log_pp) - (try0 >> log_pp);
            let xlb = (trx0 >> log_pp) << log_pp;
            let ylb = (try0 >> log_pp) << log_pp;
            for y in 0..nph {
                let ppy0 = ylb + (y << log_pp);
                for x in 0..npw {
                    let ppx0 = xlb + (x << log_pp);
                    let t = (
                        (downsamp.0 * ppx0).max(tile.x),
                        (downsamp.1 * ppy0).max(tile.y),
                    );
                    precincts.push(Precinct {
                        img_point: t,
                        cb_idxs: [Rect::default(); 4],
                        packet: Vec::new(),
                    });
                }
            }
            // subband::get_cb_indices
            let shift = u32::from(res_num > 0);
            let xcb = settings.log_block.0.min(log_pp - shift);
            let ycb = settings.log_block.1.min(log_pp - shift);
            for (b, band) in bands.iter().enumerate() {
                if band.empty {
                    continue;
                }
                let b = b as u32;
                let mut coly = 0;
                for y in 0..nph {
                    let mut pcy0 = try0.max(ylb + (y << log_pp));
                    let mut pcy1 = try1.min(ylb + ((y + 1) << log_pp));
                    pcy0 = (pcy0 - (b >> 1) + (1 << shift) - 1) >> shift;
                    pcy1 = (pcy1 - (b >> 1) + (1 << shift) - 1) >> shift;
                    let yb = ((pcy1 + (1 << ycb) - 1) >> ycb) - (pcy0 >> ycb);
                    let mut colx = 0;
                    for x in 0..npw {
                        let mut pcx0 = trx0.max(xlb + (x << log_pp));
                        let mut pcx1 = trx1.min(xlb + ((x + 1) << log_pp));
                        pcx0 = (pcx0 - (b & 1) + (1 << shift) - 1) >> shift;
                        pcx1 = (pcx1 - (b & 1) + (1 << shift) - 1) >> shift;
                        let xb = ((pcx1 + (1 << xcb) - 1) >> xcb) - (pcx0 >> xcb);
                        precincts[(y * npw + x) as usize].cb_idxs[b as usize] = Rect {
                            x: colx,
                            y: coly,
                            w: xb,
                            h: yb,
                        };
                        colx += xb;
                    }
                    coly += yb;
                }
            }
        }
        let mut num_bytes = 0u32;
        for p in &mut precincts {
            p.packet = prepare_precinct(&bands, &p.cb_idxs);
            num_bytes += p.packet.len() as u32;
        }
        resolutions[res_num as usize] = Some(Resolution {
            precincts,
            num_bytes,
            cursor: 0,
        });
        cur_rect = next_rect;
        plane = next_plane;
        downsamp = (downsamp.0 * 2, downsamp.1 * 2);
    }
    let resolutions: Vec<Resolution> = resolutions
        .into_iter()
        .map(|r| r.expect("resolution"))
        .collect();
    let num_bytes = resolutions.iter().map(|r| r.num_bytes).sum();
    Ok(TileComp {
        resolutions,
        num_bytes,
    })
}

// ---------------------------------------------------------------------------

fn sot(out: &mut Vec<u8>, tile: u16, payload: u32, tp: u8, tn: u8) {
    out.extend_from_slice(&[0xff, 0x90, 0x00, 0x0a]);
    out.extend_from_slice(&tile.to_be_bytes());
    out.extend_from_slice(&(payload + 14).to_be_bytes());
    out.push(tp);
    out.push(tn);
    out.extend_from_slice(&[0xff, 0x93]);
}

/// The messages OpenJPH writes to stdout (its default info and warning
/// stream) when `write_headers` adjusts the tile-part division.
pub fn diagnostics(settings: &Settings) -> Vec<&'static str> {
    let po = settings.progression;
    let res = settings.tilepart_resolutions;
    let comp = settings.tilepart_components;
    let mut out = Vec::new();
    if (po == LRCP || po == RLCP) && comp && !res {
        out.push(
            "ojph info 0x00030021 at ojph_codestream_local.cpp:587: For LRCP and RLCP progression orders, \
             tilepart divisions at the component level, means that we have a tilepart for every resolution and \
             component.\n\n",
        );
    }
    if po == RPCL && comp {
        out.push(
            "ojph warning 0x00030021 at ojph_codestream_local.cpp:595: For RPCL progression, having tilepart \
             divisions at the component level means a tilepart for every precinct, which does not make sense, \
             since we can have no more than 255 tile parts. This has been corrected by removing tilepart divisions \
             at the component level.\n",
        );
    }
    if po == PCRL && (res || comp) {
        out.push(
            "ojph warning 0x00030022 at ojph_codestream_local.cpp:605: For PCRL progression, having tilepart \
             divisions at the component level or the resolution level means a tile part for every precinct, which \
             does not make sense, since we can have no more than 255 tile parts.  This has been corrected by \
             removing tilepart divisions; use another progression if you want tileparts.\n",
        );
    }
    if po == CPRL && res {
        out.push(
            "ojph warning 0x00030023 at ojph_codestream_local.cpp:615: For CPRL progression, having tilepart \
             divisions at the resolution level means a tile part for every precinct, which does not make sense, \
             since we can have no more than 255 tile parts. This has been corrected by removing tilepart divisions \
             at the resolution level.\n",
        );
    }
    out
}

pub fn encode(
    components: &[Component],
    width: u32,
    height: u32,
    settings: &Settings,
) -> Result<Vec<u8>, EncodeError> {
    let nd = settings.num_decompositions;
    if nd > 32 || components.is_empty() || width == 0 || height == 0 {
        return Err(EncodeError::Unsupported);
    }
    let num_comps = components.len();
    let (mut tw, mut th) = settings.tile_size;
    if tw == 0 && th == 0 {
        tw = width;
        th = height;
    }
    if tw == 0 || th == 0 {
        return Err(EncodeError::Unsupported);
    }
    let ntw = div_ceil(width, tw);
    let nth = div_ceil(height, th);
    if u64::from(ntw) * u64::from(nth) > 65535 {
        return Err(EncodeError::Unsupported);
    }
    let po = settings.progression;
    let mut div = (u32::from(settings.tilepart_resolutions) * TILEPART_RESOLUTIONS)
        | (u32::from(settings.tilepart_components) * TILEPART_COMPONENTS);
    if (po == LRCP || po == RLCP) && div == TILEPART_COMPONENTS {
        div |= TILEPART_RESOLUTIONS;
    }
    if po == RPCL && div & TILEPART_COMPONENTS != 0 {
        div &= !TILEPART_COMPONENTS;
    }
    if po == PCRL && div != 0 {
        div = 0;
    }
    if po == CPRL && div & TILEPART_RESOLUTIONS != 0 {
        div &= !TILEPART_RESOLUTIONS;
    }
    let num_tileparts = match div {
        0 => 1,
        TILEPART_COMPONENTS => num_comps as u32,
        TILEPART_RESOLUTIONS => 1 + nd,
        _ => num_comps as u32 * (nd + 1),
    };
    if num_tileparts > 255 {
        return Err(EncodeError::Unsupported);
    }
    if (po == RPCL || po == PCRL)
        && components
            .iter()
            .any(|c| c.dx & (c.dx - 1) != 0 || c.dy & (c.dy - 1) != 0)
    {
        return Err(EncodeError::Unsupported);
    }

    // QCD from component 0; QCC where another component's depth differs.
    let main = Quant::new(settings.reversible, nd, components[0].precision)?;
    let mut quants = Vec::with_capacity(num_comps);
    let mut qccs = Vec::new();
    for (c, comp) in components.iter().enumerate() {
        if comp.precision == main.bit_depth {
            quants.push(main.clone());
        } else {
            let q = Quant::new(settings.reversible, nd, comp.precision)?;
            qccs.push((c, q.clone()));
            quants.push(q);
        }
    }

    let mut out = vec![0xff, 0x4f];
    // SIZ
    out.extend_from_slice(&[0xff, 0x51]);
    out.extend_from_slice(&((38 + 3 * num_comps) as u16).to_be_bytes());
    out.extend_from_slice(&0x4000u16.to_be_bytes());
    for v in [width, height, 0, 0, tw, th, 0, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&(num_comps as u16).to_be_bytes());
    for c in components {
        out.extend_from_slice(&[(c.precision - 1) as u8, c.dx as u8, c.dy as u8]);
    }
    // CAP
    let mut b = main.magb();
    for (_, q) in &qccs {
        b = b.max(q.magb());
    }
    let bp = if b <= 8 {
        0
    } else if b < 28 {
        b - 8
    } else {
        13 + (b >> 2)
    };
    let ccap = (if settings.reversible { 0u16 } else { 0x20 }) | bp as u16;
    out.extend_from_slice(&[0xff, 0x50, 0x00, 0x08, 0x00, 0x02, 0x00, 0x00]);
    out.extend_from_slice(&ccap.to_be_bytes());
    // COD
    out.extend_from_slice(&[0xff, 0x52, 0x00, 0x0c, 0x00, po, 0x00, 0x01, 0x00]);
    out.extend_from_slice(&[
        nd as u8,
        (settings.log_block.0 - 2) as u8,
        (settings.log_block.1 - 2) as u8,
        0x40,
        u8::from(settings.reversible),
    ]);
    main.write(&mut out, None);
    for (c, q) in &qccs {
        q.write(&mut out, Some((*c as u16, num_comps)));
    }
    let version = b"OpenJPH Ver 0.32.0.";
    out.extend_from_slice(&[0xff, 0x64]);
    out.extend_from_slice(&((version.len() + 4) as u16).to_be_bytes());
    out.extend_from_slice(&[0x00, 0x01]);
    out.extend_from_slice(version);
    if !settings.comment.is_empty() {
        out.extend_from_slice(&[0xff, 0x64]);
        out.extend_from_slice(&((settings.comment.len() + 4) as u16).to_be_bytes());
        out.extend_from_slice(&[0x00, 0x01]);
        out.extend_from_slice(settings.comment);
    }

    // Tiles
    let mut tiles = Vec::with_capacity((ntw * nth) as usize);
    for ty in 0..nth {
        let y0 = ty * th;
        let y1 = (y0 + th).min(height);
        for tx in 0..ntw {
            let x0 = tx * tw;
            let x1 = (x0 + tw).min(width);
            let rect = Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            };
            let mut comps = Vec::with_capacity(num_comps);
            for (c, comp) in components.iter().enumerate() {
                comps.push(code_tile_comp(comp, rect, &quants[c], settings)?);
            }
            tiles.push(comps);
        }
    }

    if settings.tlm {
        let mut pairs: Vec<(u16, u32)> = Vec::new();
        for (t, comps) in tiles.iter().enumerate() {
            let t = t as u16;
            match div {
                0 => pairs.push((t, comps.iter().map(|c| c.num_bytes).sum())),
                TILEPART_RESOLUTIONS => {
                    for r in 0..=nd {
                        pairs.push((t, comps.iter().map(|c| c.bytes(r)).sum()));
                    }
                }
                TILEPART_COMPONENTS if po == CPRL => {
                    for c in comps {
                        pairs.push((t, c.num_bytes));
                    }
                }
                _ => {
                    for r in 0..=nd {
                        for c in comps {
                            pairs.push((t, c.bytes(r)));
                        }
                    }
                }
            }
        }
        let max_per_seg = (65535 - 4) / 6;
        if pairs.len() > max_per_seg * 256 {
            return Err(EncodeError::Unsupported);
        }
        for (z, chunk) in pairs.chunks(max_per_seg).enumerate() {
            out.extend_from_slice(&[0xff, 0x55]);
            out.extend_from_slice(&((4 + 6 * chunk.len()) as u16).to_be_bytes());
            out.push(z as u8);
            out.push(0x60);
            for &(t, p) in chunk {
                out.extend_from_slice(&t.to_be_bytes());
                out.extend_from_slice(&(p + 14).to_be_bytes());
            }
        }
    }

    for (t, comps) in tiles.iter_mut().enumerate() {
        let t = t as u16;
        if div == 0 {
            sot(&mut out, t, comps.iter().map(|c| c.num_bytes).sum(), 0, 1);
        }
        match po {
            LRCP | RLCP => {
                if div == 0 {
                    for r in 0..=nd {
                        for c in comps.iter() {
                            c.write_all(r, &mut out);
                        }
                    }
                } else if div == TILEPART_RESOLUTIONS {
                    for r in 0..=nd {
                        sot(
                            &mut out,
                            t,
                            comps.iter().map(|c| c.bytes(r)).sum(),
                            r as u8,
                            (nd + 1) as u8,
                        );
                        for c in comps.iter() {
                            c.write_all(r, &mut out);
                        }
                    }
                } else {
                    let n = num_comps as u32 * (nd + 1);
                    for r in 0..=nd {
                        for (ci, c) in comps.iter().enumerate() {
                            sot(
                                &mut out,
                                t,
                                c.bytes(r),
                                (ci as u32 + r * num_comps as u32) as u8,
                                n as u8,
                            );
                            c.write_all(r, &mut out);
                        }
                    }
                }
            }
            RPCL => {
                for r in 0..=nd {
                    if div == TILEPART_RESOLUTIONS {
                        sot(
                            &mut out,
                            t,
                            comps.iter().map(|c| c.bytes(r)).sum(),
                            r as u8,
                            (nd + 1) as u8,
                        );
                    }
                    loop {
                        let mut best: Option<(usize, (u32, u32))> = None;
                        for (ci, c) in comps.iter().enumerate() {
                            if let Some(cur) = c.top_left(r) {
                                match best {
                                    None => best = Some((ci, cur)),
                                    Some((_, s))
                                        if cur.1 < s.1 || (cur.1 == s.1 && cur.0 < s.0) =>
                                    {
                                        best = Some((ci, cur))
                                    }
                                    _ => {}
                                }
                            }
                        }
                        match best {
                            Some((ci, _)) => comps[ci].write_one(r, &mut out),
                            None => break,
                        }
                    }
                }
            }
            PCRL => loop {
                let mut best: Option<(usize, u32, (u32, u32))> = None;
                for (ci, c) in comps.iter().enumerate() {
                    for r in 0..=nd {
                        if let Some(cur) = c.top_left(r) {
                            let take = match best {
                                None => true,
                                Some((bc, br, s)) => {
                                    cur.1 < s.1
                                        || (cur.1 == s.1 && cur.0 < s.0)
                                        || (cur == s && ci < bc)
                                        || (cur == s && ci == bc && r < br)
                                }
                            };
                            if take {
                                best = Some((ci, r, cur));
                            }
                        }
                    }
                }
                match best {
                    Some((ci, r, _)) => comps[ci].write_one(r, &mut out),
                    None => break,
                }
            },
            _ => {
                for (ci, comp) in comps.iter_mut().enumerate() {
                    if div == TILEPART_COMPONENTS {
                        sot(&mut out, t, comp.num_bytes, ci as u8, num_comps as u8);
                    }
                    loop {
                        let mut best: Option<(u32, (u32, u32))> = None;
                        for r in 0..=nd {
                            if let Some(cur) = comp.top_left(r) {
                                match best {
                                    None => best = Some((r, cur)),
                                    Some((_, s))
                                        if cur.1 < s.1 || (cur.1 == s.1 && cur.0 < s.0) =>
                                    {
                                        best = Some((r, cur))
                                    }
                                    _ => {}
                                }
                            }
                        }
                        match best {
                            Some((r, _)) => comp.write_one(r, &mut out),
                            None => break,
                        }
                    }
                }
            }
        }
    }
    out.extend_from_slice(&[0xff, 0xd9]);
    Ok(out)
}
