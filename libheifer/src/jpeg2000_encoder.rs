// SPDX-License-Identifier: LGPL-3.0-or-later
//! JPEG2000 codestream encoding with the output of libheif's OpenJPEG
//! encoder plugin (OpenJPEG 2.5 defaults: 6 resolutions, 64x64 code-blocks,
//! LRCP, one layer, no colour transform, one tile and precincts of 2^15).
//!
//! The pipeline follows OpenJPEG's encoder: DC level shift, 5/3 or 9/7
//! forward wavelet (`dwt.c`), EBCOT tier-1 with its MQ coder termination,
//! pass rates and distortion estimates (`t1.c`, `mqc.c`), rate allocation
//! (`tcd.c`), packet headers with tag trees (`t2.c`, `tgt.c`) and its marker
//! segments and tile buffer (`j2k.c`).

/// OpenJPEG's version, as written in its default COM marker.
const OPENJPEG_VERSION: &str = "2.5.4";
const NUM_RESOLUTIONS: u32 = 6;
const CBLK_EXPONENT: u32 = 6;
const GUARD_BITS: u32 = 2;
/// T1_NMSEDEC_FRACBITS: fractional bits of tier-1 magnitudes.
const FRACBITS: u32 = 6;

/// One image component: `width * height` samples of `precision` bits,
/// subsampled by `dx`, `dy` relative to the reference grid.
pub struct Component<'a> {
    pub samples: &'a [u16],
    pub width: u32,
    pub height: u32,
    pub dx: u32,
    pub dy: u32,
    pub precision: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// `opj_start_compress` rejects the image (resolutions vs tile size).
    StartCompress,
    /// `opj_encode` fails: the packets overflow OpenJPEG's tile buffer.
    Encode,
}

/// Quality settings as libheif's plugin passes them to OpenJPEG.
#[derive(Clone, Copy)]
pub struct Settings {
    /// `parameters.irreversible`: the 9/7 wavelet with rate allocation.
    pub irreversible: bool,
    /// Plugin quality (0-100), giving the rate `1 + (100 - quality) / 2`.
    pub quality: i32,
}

fn ceil_div_pow2(a: i64, b: u32) -> i64 {
    (a + (1i64 << b) - 1) >> b
}

fn floorlog2(mut a: u32) -> u32 {
    let mut l = 0;
    while a > 1 {
        a >>= 1;
        l += 1;
    }
    l
}

// ---------------------------------------------------------------------------
// Geometry and quantization

struct CodeBlock {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
    /// Can be negative for tiny magnitudes (OpenJPEG then codes no pass).
    numbps: i32,
    passes: Vec<Pass>,
    data: Vec<u8>,
    /// Passes in the layer (and in the previous `makelayer` trial).
    layer_passes: usize,
}

#[derive(Clone, Copy, Default)]
struct Pass {
    rate: u32,
    len: u32,
    distortion: f64,
}

struct Band {
    bandno: u32,
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
    /// Mb: exponent + guard bits - 1.
    numbps: i32,
    stepsize: f32,
    cw: u32,
    ch: u32,
    cblks: Vec<CodeBlock>,
}

impl Band {
    fn is_empty(&self) -> bool {
        self.x1 - self.x0 == 0 || self.y1 - self.y0 == 0
    }
}

struct Resolution {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
    bands: Vec<Band>,
    /// Number of precincts (0 or 1 with 2^15 precincts).
    precincts: u32,
}

struct TileComponent {
    width: usize,
    height: usize,
    ints: Vec<i32>,
    floats: Vec<f32>,
    resolutions: Vec<Resolution>,
}

/// 9/7 wavelet norms (`opj_dwt_norms_real`), by orientation and level.
const NORMS_REAL: [[f64; 10]; 4] = [
    [
        1.000, 1.965, 4.177, 8.403, 16.90, 33.84, 67.69, 135.3, 270.6, 540.9,
    ],
    [
        2.022, 3.989, 8.355, 17.04, 34.27, 68.63, 137.3, 274.6, 549.0, 549.0,
    ],
    [
        2.022, 3.989, 8.355, 17.04, 34.27, 68.63, 137.3, 274.6, 549.0, 549.0,
    ],
    [
        2.080, 3.865, 8.307, 17.18, 34.71, 69.59, 139.3, 278.6, 557.2, 557.2,
    ],
];

/// 5/3 wavelet norms (`opj_dwt_norms`), by orientation and level.
const NORMS: [[f64; 10]; 4] = [
    [
        1.000, 1.500, 2.750, 5.375, 10.68, 21.34, 42.67, 85.33, 170.7, 341.3,
    ],
    [
        1.038, 1.592, 2.919, 5.703, 11.33, 22.64, 45.25, 90.48, 180.9, 180.9,
    ],
    [
        1.038, 1.592, 2.919, 5.703, 11.33, 22.64, 45.25, 90.48, 180.9, 180.9,
    ],
    [
        0.7186, 0.9218, 1.586, 3.043, 6.019, 12.01, 24.00, 47.97, 95.93, 95.93,
    ],
];

fn log2_gain(orient: u32) -> u32 {
    match orient {
        0 => 0,
        3 => 2,
        _ => 1,
    }
}

/// `opj_dwt_calc_explicit_stepsizes` + `opj_dwt_encode_stepsize`: the
/// (exponent, mantissa) of a band.
fn band_step(precision: u32, level: u32, orient: u32, irreversible: bool) -> (u32, u32) {
    let (stepsize, gain) = if irreversible {
        (1.0 / NORMS_REAL[orient as usize][level as usize], 0)
    } else {
        (1.0, log2_gain(orient))
    };
    let s = (stepsize * 8192.0).floor() as i32;
    let log = floorlog2(s as u32) as i32;
    let p = log - 13;
    let n = 11 - log;
    let mant = (if n < 0 { s >> -n } else { s << n }) & 0x7ff;
    ((precision as i32 + gain as i32 - p) as u32, mant as u32)
}

fn build_tile_component(
    component: &Component<'_>,
    image_width: u32,
    image_height: u32,
    settings: Settings,
) -> TileComponent {
    let x1 = (image_width as i64 + component.dx as i64 - 1) / component.dx as i64;
    let y1 = (image_height as i64 + component.dy as i64 - 1) / component.dy as i64;
    let mut resolutions = Vec::new();
    for resno in 0..NUM_RESOLUTIONS {
        let level = NUM_RESOLUTIONS - 1 - resno;
        let (rx1, ry1) = (ceil_div_pow2(x1, level), ceil_div_pow2(y1, level));
        let precincts = if rx1 > 0 && ry1 > 0 { 1 } else { 0 };
        let mut bands = Vec::new();
        let band_numbers: &[u32] = if resno == 0 { &[0] } else { &[1, 2, 3] };
        for &bandno in band_numbers {
            let (bx0, by0, bx1, by1) = if resno == 0 {
                (0, 0, ceil_div_pow2(x1, level), ceil_div_pow2(y1, level))
            } else {
                let x0b = (bandno & 1) as i64;
                let y0b = (bandno >> 1) as i64;
                (
                    ceil_div_pow2(-(x0b << level), level + 1),
                    ceil_div_pow2(-(y0b << level), level + 1),
                    ceil_div_pow2(x1 - (x0b << level), level + 1),
                    ceil_div_pow2(y1 - (y0b << level), level + 1),
                )
            };
            let (expn, mant) = band_step(component.precision, level, bandno, settings.irreversible);
            let rb = component.precision as i32 + log2_gain(bandno) as i32;
            let stepsize = ((1.0 + mant as f64 / 2048.0) * 2f64.powi(rb - expn as i32)) as f32;
            let numbps = expn as i32 + GUARD_BITS as i32 - 1;
            let mut band = Band {
                bandno,
                x0: bx0,
                y0: by0,
                x1: bx1,
                y1: by1,
                numbps,
                stepsize,
                cw: 0,
                ch: 0,
                cblks: Vec::new(),
            };
            if !band.is_empty() && precincts > 0 {
                let size = 1i64 << CBLK_EXPONENT;
                let tlx = bx0.div_euclid(size) * size;
                let tly = by0.div_euclid(size) * size;
                let brx = (bx1 + size - 1).div_euclid(size) * size;
                let bry = (by1 + size - 1).div_euclid(size) * size;
                band.cw = ((brx - tlx) >> CBLK_EXPONENT) as u32;
                band.ch = ((bry - tly) >> CBLK_EXPONENT) as u32;
                for j in 0..band.ch as i64 {
                    for i in 0..band.cw as i64 {
                        let cx0 = tlx + i * size;
                        let cy0 = tly + j * size;
                        band.cblks.push(CodeBlock {
                            x0: cx0.max(bx0),
                            y0: cy0.max(by0),
                            x1: (cx0 + size).min(bx1),
                            y1: (cy0 + size).min(by1),
                            numbps: 0,
                            passes: Vec::new(),
                            data: Vec::new(),
                            layer_passes: 0,
                        });
                    }
                }
            }
            bands.push(band);
        }
        resolutions.push(Resolution {
            x0: 0,
            y0: 0,
            x1: rx1,
            y1: ry1,
            bands,
            precincts,
        });
    }
    let width = x1 as usize;
    let height = y1 as usize;
    let shift = 1i32 << (component.precision - 1);
    let shifted =
        (0..width * height).map(|i| component.samples.get(i).copied().unwrap_or(0) as i32 - shift);
    let (ints, floats) = if settings.irreversible {
        (Vec::new(), shifted.map(|v| v as f32).collect())
    } else {
        (shifted.collect(), Vec::new())
    };
    TileComponent {
        width,
        height,
        ints,
        floats,
        resolutions,
    }
}

// ---------------------------------------------------------------------------
// Forward wavelets (opj_dwt_encode, opj_dwt_encode_real)

fn dwt53_1d(line: &mut [i32], tmp: &mut [i32], even: bool) {
    let width = line.len();
    let sn = (width + even as usize) >> 1;
    let dn = width - sn;
    if even {
        if width > 1 {
            let mut i = 0;
            while i + 1 < sn {
                tmp[sn + i] =
                    line[2 * i + 1].wrapping_sub((line[2 * i].wrapping_add(line[2 * i + 2])) >> 1);
                i += 1;
            }
            if width.is_multiple_of(2) {
                tmp[sn + i] = line[2 * i + 1].wrapping_sub(line[2 * i]);
            }
            line[0] = line[0].wrapping_add((tmp[sn].wrapping_add(tmp[sn]).wrapping_add(2)) >> 2);
            let mut i = 1;
            while i < dn {
                line[i] = line[2 * i]
                    .wrapping_add((tmp[sn + i - 1].wrapping_add(tmp[sn + i]).wrapping_add(2)) >> 2);
                i += 1;
            }
            if !width.is_multiple_of(2) {
                line[i] = line[2 * i].wrapping_add(
                    (tmp[sn + i - 1]
                        .wrapping_add(tmp[sn + i - 1])
                        .wrapping_add(2))
                        >> 2,
                );
            }
            line[sn..].copy_from_slice(&tmp[sn..sn + dn]);
        }
    } else if width == 1 {
        line[0] = line[0].wrapping_mul(2);
    } else {
        tmp[sn] = line[0].wrapping_sub(line[1]);
        let mut i = 1;
        while i < sn {
            tmp[sn + i] = line[2 * i]
                .wrapping_sub((line[2 * i + 1].wrapping_add(line[2 * (i - 1) + 1])) >> 1);
            i += 1;
        }
        if !width.is_multiple_of(2) {
            tmp[sn + i] = line[2 * i].wrapping_sub(line[2 * (i - 1) + 1]);
        }
        let mut i = 0;
        while i + 1 < dn {
            line[i] = line[2 * i + 1]
                .wrapping_add((tmp[sn + i].wrapping_add(tmp[sn + i + 1]).wrapping_add(2)) >> 2);
            i += 1;
        }
        if width.is_multiple_of(2) {
            line[i] = line[2 * i + 1]
                .wrapping_add((tmp[sn + i].wrapping_add(tmp[sn + i]).wrapping_add(2)) >> 2);
        }
        line[sn..].copy_from_slice(&tmp[sn..sn + dn]);
    }
}

// OpenJPEG's literals, kept verbatim.
#[allow(clippy::excessive_precision)]
const ALPHA: f32 = -1.586134342;
#[allow(clippy::excessive_precision)]
const BETA: f32 = -0.052980118;
#[allow(clippy::excessive_precision)]
const GAMMA: f32 = 0.882911075;
#[allow(clippy::excessive_precision)]
const DELTA: f32 = 0.443506852;
#[allow(clippy::excessive_precision)]
const K: f32 = 1.230174105;

/// `opj_dwt_encode_step2`: `w[fw - 1] += (w[fl] + w[fw]) * c` along the line.
fn step2(w: &mut [f32], fl: usize, mut fw: usize, end: usize, m: usize, c: f32) {
    let imax = end.min(m);
    if imax > 0 {
        w[fw - 1] += (w[fl] + w[fw]) * c;
        fw += 2;
        for _ in 1..imax {
            w[fw - 1] += (w[fw - 2] + w[fw]) * c;
            fw += 2;
        }
    }
    if m < end {
        w[fw - 1] += (2.0 * w[fw - 2]) * c;
    }
}

/// `opj_dwt_encode_1_real` on an interleaved line, then deinterleaved.
fn dwt97_1d(line: &mut [f32], tmp: &mut [f32], even: bool) {
    let width = line.len();
    if width == 1 {
        return;
    }
    let sn = (width + even as usize) >> 1;
    let dn = width - sn;
    tmp[..width].copy_from_slice(line);
    let (a, b) = if even { (0, 1) } else { (1, 0) };
    let w = &mut tmp[..width];
    step2(w, a, b + 1, dn, dn.min(sn.wrapping_sub(b)), ALPHA);
    step2(w, b, a + 1, sn, sn.min(dn.wrapping_sub(a)), BETA);
    step2(w, a, b + 1, dn, dn.min(sn.wrapping_sub(b)), GAMMA);
    step2(w, b, a + 1, sn, sn.min(dn.wrapping_sub(a)), DELTA);
    #[allow(clippy::excessive_precision)]
    let inv_k = (1.0f64 / 1.230174105) as f32;
    for i in 0..dn {
        w[b + 2 * i] *= K;
    }
    for i in 0..sn {
        w[a + 2 * i] *= inv_k;
    }
    for i in 0..sn {
        line[i] = w[a + 2 * i];
    }
    for i in 0..dn {
        line[sn + i] = w[b + 2 * i];
    }
}

fn dwt_forward(tc: &mut TileComponent, irreversible: bool) {
    let w = tc.width;
    let max = tc.width.max(tc.height);
    let mut line_i = vec![0i32; max];
    let mut tmp_i = vec![0i32; max];
    let mut line_f = vec![0f32; max];
    let mut tmp_f = vec![0f32; max];
    for resno in (1..tc.resolutions.len()).rev() {
        let cur = &tc.resolutions[resno];
        let rw = (cur.x1 - cur.x0) as usize;
        let rh = (cur.y1 - cur.y0) as usize;
        let even_col = cur.y0 & 1 == 0;
        let even_row = cur.x0 & 1 == 0;
        if irreversible {
            for x in 0..rw {
                for (y, v) in line_f[..rh].iter_mut().enumerate() {
                    *v = tc.floats[y * w + x];
                }
                dwt97_1d(&mut line_f[..rh], &mut tmp_f, even_col);
                for (y, &v) in line_f[..rh].iter().enumerate() {
                    tc.floats[y * w + x] = v;
                }
            }
            for y in 0..rh {
                dwt97_1d(&mut tc.floats[y * w..y * w + rw], &mut tmp_f, even_row);
            }
        } else {
            for x in 0..rw {
                for (y, v) in line_i[..rh].iter_mut().enumerate() {
                    *v = tc.ints[y * w + x];
                }
                dwt53_1d(&mut line_i[..rh], &mut tmp_i, even_col);
                for (y, &v) in line_i[..rh].iter().enumerate() {
                    tc.ints[y * w + x] = v;
                }
            }
            for y in 0..rh {
                dwt53_1d(&mut tc.ints[y * w..y * w + rw], &mut tmp_i, even_row);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MQ encoder (mqc.c)

/// (Qe, NMPS, NLPS, SWITCH) of ITU-T T.800 Table C.2.
const QE: [(u32, u8, u8, u8); 47] = [
    (0x5601, 1, 1, 1),
    (0x3401, 2, 6, 0),
    (0x1801, 3, 9, 0),
    (0x0AC1, 4, 12, 0),
    (0x0521, 5, 29, 0),
    (0x0221, 38, 33, 0),
    (0x5601, 7, 6, 1),
    (0x5401, 8, 14, 0),
    (0x4801, 9, 14, 0),
    (0x3801, 10, 14, 0),
    (0x3001, 11, 17, 0),
    (0x2401, 12, 18, 0),
    (0x1C01, 13, 20, 0),
    (0x1601, 29, 21, 0),
    (0x5601, 15, 14, 1),
    (0x5401, 16, 14, 0),
    (0x5101, 17, 15, 0),
    (0x4801, 18, 16, 0),
    (0x3801, 19, 17, 0),
    (0x3401, 20, 18, 0),
    (0x3001, 21, 19, 0),
    (0x2801, 22, 19, 0),
    (0x2401, 23, 20, 0),
    (0x2201, 24, 21, 0),
    (0x1C01, 25, 22, 0),
    (0x1801, 26, 23, 0),
    (0x1601, 27, 24, 0),
    (0x1401, 28, 25, 0),
    (0x1201, 29, 26, 0),
    (0x1101, 30, 27, 0),
    (0x0AC1, 31, 28, 0),
    (0x09C1, 32, 29, 0),
    (0x08A1, 33, 30, 0),
    (0x0521, 34, 31, 0),
    (0x0441, 35, 32, 0),
    (0x02A1, 36, 33, 0),
    (0x0221, 37, 34, 0),
    (0x0141, 38, 35, 0),
    (0x0111, 39, 36, 0),
    (0x0085, 40, 37, 0),
    (0x0049, 41, 38, 0),
    (0x0025, 42, 39, 0),
    (0x0015, 43, 40, 0),
    (0x0009, 44, 41, 0),
    (0x0005, 45, 42, 0),
    (0x0001, 45, 43, 0),
    (0x5601, 46, 46, 0),
];

const CTX_AGG: usize = 17;
const CTX_UNI: usize = 18;
const NUM_CTXS: usize = 19;

struct Mqc {
    /// `buf[0]` is the byte before the code-block data (OpenJPEG's `start - 1`).
    buf: Vec<u8>,
    bp: usize,
    a: u32,
    c: u32,
    ct: u32,
    ctx: [(u8, u8); NUM_CTXS],
}

impl Mqc {
    fn new() -> Self {
        let mut ctx = [(0u8, 0u8); NUM_CTXS];
        ctx[CTX_UNI] = (46, 0);
        ctx[CTX_AGG] = (3, 0);
        ctx[0] = (4, 0);
        Mqc {
            buf: vec![0],
            bp: 0,
            a: 0x8000,
            c: 0,
            ct: 12,
            ctx,
        }
    }

    fn put(&mut self, index: usize, value: u8) {
        if self.buf.len() <= index {
            self.buf.resize(index + 1, 0);
        }
        self.buf[index] = value;
    }

    fn byteout(&mut self) {
        if self.buf[self.bp] == 0xff {
            self.bp += 1;
            self.put(self.bp, (self.c >> 20) as u8);
            self.c &= 0xfffff;
            self.ct = 7;
        } else if self.c & 0x8000000 == 0 {
            self.bp += 1;
            self.put(self.bp, (self.c >> 19) as u8);
            self.c &= 0x7ffff;
            self.ct = 8;
        } else {
            self.buf[self.bp] = self.buf[self.bp].wrapping_add(1);
            if self.buf[self.bp] == 0xff {
                self.c &= 0x7ffffff;
                self.bp += 1;
                self.put(self.bp, (self.c >> 20) as u8);
                self.c &= 0xfffff;
                self.ct = 7;
            } else {
                self.bp += 1;
                self.put(self.bp, (self.c >> 19) as u8);
                self.c &= 0x7ffff;
                self.ct = 8;
            }
        }
    }

    fn renorm(&mut self) {
        loop {
            self.a <<= 1;
            self.c <<= 1;
            self.ct -= 1;
            if self.ct == 0 {
                self.byteout();
            }
            if self.a & 0x8000 != 0 {
                break;
            }
        }
    }

    fn encode(&mut self, cx: usize, d: u32) {
        let (state, mps) = self.ctx[cx];
        let (qe, nmps, nlps, switch) = QE[state as usize];
        if mps as u32 == d {
            self.a -= qe;
            if self.a & 0x8000 == 0 {
                if self.a < qe {
                    self.a = qe;
                } else {
                    self.c += qe;
                }
                self.ctx[cx] = (nmps, mps);
                self.renorm();
            } else {
                self.c += qe;
            }
        } else {
            self.a -= qe;
            if self.a < qe {
                self.c += qe;
            } else {
                self.a = qe;
            }
            self.ctx[cx] = (nlps, if switch == 1 { 1 - mps } else { mps });
            self.renorm();
        }
    }

    fn flush(&mut self) {
        let tempc = self.c + self.a;
        self.c |= 0xffff;
        if self.c >= tempc {
            self.c -= 0x8000;
        }
        self.c <<= self.ct;
        self.byteout();
        self.c <<= self.ct;
        self.byteout();
        if self.buf[self.bp] != 0xff {
            self.bp += 1;
        }
    }

    /// `opj_mqc_numbytes`: bytes from the code-block start to `bp`.
    fn numbytes(&self) -> u32 {
        // bp can still be at start - 1: the difference wraps like OpenJPEG's.
        (self.bp as u32).wrapping_sub(1)
    }
}

// ---------------------------------------------------------------------------
// EBCOT tier-1 encoding (t1.c), code-block style 0

/// OpenJPEG's `lut_nmsedec_sig`, `_sig0`, `_ref` and `_ref0` (t1_luts.h):
/// normalized distortion decreases of significance and refinement, indexed
/// by the 7 bits at and below the coded bit-plane.
const LUT_NMSEDEC_SIG: [i16; 128] = [
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0180, 0x0300, 0x0480, 0x0600, 0x0780, 0x0900, 0x0a80, 0x0c00, 0x0d80, 0x0f00, 0x1080,
    0x1200, 0x1380, 0x1500, 0x1680, 0x1800, 0x1980, 0x1b00, 0x1c80, 0x1e00, 0x1f80, 0x2100, 0x2280,
    0x2400, 0x2580, 0x2700, 0x2880, 0x2a00, 0x2b80, 0x2d00, 0x2e80, 0x3000, 0x3180, 0x3300, 0x3480,
    0x3600, 0x3780, 0x3900, 0x3a80, 0x3c00, 0x3d80, 0x3f00, 0x4080, 0x4200, 0x4380, 0x4500, 0x4680,
    0x4800, 0x4980, 0x4b00, 0x4c80, 0x4e00, 0x4f80, 0x5100, 0x5280, 0x5400, 0x5580, 0x5700, 0x5880,
    0x5a00, 0x5b80, 0x5d00, 0x5e80, 0x6000, 0x6180, 0x6300, 0x6480, 0x6600, 0x6780, 0x6900, 0x6a80,
    0x6c00, 0x6d80, 0x6f00, 0x7080, 0x7200, 0x7380, 0x7500, 0x7680,
];
const LUT_NMSEDEC_SIG0: [i16; 128] = [
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0080, 0x0080, 0x0080, 0x0080, 0x0100, 0x0100,
    0x0100, 0x0180, 0x0180, 0x0200, 0x0200, 0x0280, 0x0280, 0x0300, 0x0300, 0x0380, 0x0400, 0x0400,
    0x0480, 0x0500, 0x0580, 0x0580, 0x0600, 0x0680, 0x0700, 0x0780, 0x0800, 0x0880, 0x0900, 0x0980,
    0x0a00, 0x0a80, 0x0b80, 0x0c00, 0x0c80, 0x0d00, 0x0e00, 0x0e80, 0x0f00, 0x1000, 0x1080, 0x1180,
    0x1200, 0x1300, 0x1380, 0x1480, 0x1500, 0x1600, 0x1700, 0x1780, 0x1880, 0x1980, 0x1a80, 0x1b00,
    0x1c00, 0x1d00, 0x1e00, 0x1f00, 0x2000, 0x2100, 0x2200, 0x2300, 0x2400, 0x2500, 0x2680, 0x2780,
    0x2880, 0x2980, 0x2b00, 0x2c00, 0x2d00, 0x2e80, 0x2f80, 0x3100, 0x3200, 0x3380, 0x3480, 0x3600,
    0x3700, 0x3880, 0x3a00, 0x3b00, 0x3c80, 0x3e00, 0x3f80, 0x4080, 0x4200, 0x4380, 0x4500, 0x4680,
    0x4800, 0x4980, 0x4b00, 0x4c80, 0x4e00, 0x4f80, 0x5180, 0x5300, 0x5480, 0x5600, 0x5800, 0x5980,
    0x5b00, 0x5d00, 0x5e80, 0x6080, 0x6200, 0x6400, 0x6580, 0x6780, 0x6900, 0x6b00, 0x6d00, 0x6e80,
    0x7080, 0x7280, 0x7480, 0x7600, 0x7800, 0x7a00, 0x7c00, 0x7e00,
];
const LUT_NMSEDEC_REF: [i16; 128] = [
    0x1800, 0x1780, 0x1700, 0x1680, 0x1600, 0x1580, 0x1500, 0x1480, 0x1400, 0x1380, 0x1300, 0x1280,
    0x1200, 0x1180, 0x1100, 0x1080, 0x1000, 0x0f80, 0x0f00, 0x0e80, 0x0e00, 0x0d80, 0x0d00, 0x0c80,
    0x0c00, 0x0b80, 0x0b00, 0x0a80, 0x0a00, 0x0980, 0x0900, 0x0880, 0x0800, 0x0780, 0x0700, 0x0680,
    0x0600, 0x0580, 0x0500, 0x0480, 0x0400, 0x0380, 0x0300, 0x0280, 0x0200, 0x0180, 0x0100, 0x0080,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0080, 0x0100, 0x0180,
    0x0200, 0x0280, 0x0300, 0x0380, 0x0400, 0x0480, 0x0500, 0x0580, 0x0600, 0x0680, 0x0700, 0x0780,
    0x0800, 0x0880, 0x0900, 0x0980, 0x0a00, 0x0a80, 0x0b00, 0x0b80, 0x0c00, 0x0c80, 0x0d00, 0x0d80,
    0x0e00, 0x0e80, 0x0f00, 0x0f80, 0x1000, 0x1080, 0x1100, 0x1180, 0x1200, 0x1280, 0x1300, 0x1380,
    0x1400, 0x1480, 0x1500, 0x1580, 0x1600, 0x1680, 0x1700, 0x1780,
];
const LUT_NMSEDEC_REF0: [i16; 128] = [
    0x2000, 0x1f00, 0x1e00, 0x1d00, 0x1c00, 0x1b00, 0x1a80, 0x1980, 0x1880, 0x1780, 0x1700, 0x1600,
    0x1500, 0x1480, 0x1380, 0x1300, 0x1200, 0x1180, 0x1080, 0x1000, 0x0f00, 0x0e80, 0x0e00, 0x0d00,
    0x0c80, 0x0c00, 0x0b80, 0x0a80, 0x0a00, 0x0980, 0x0900, 0x0880, 0x0800, 0x0780, 0x0700, 0x0680,
    0x0600, 0x0580, 0x0580, 0x0500, 0x0480, 0x0400, 0x0400, 0x0380, 0x0300, 0x0300, 0x0280, 0x0280,
    0x0200, 0x0200, 0x0180, 0x0180, 0x0100, 0x0100, 0x0100, 0x0080, 0x0080, 0x0080, 0x0080, 0x0000,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0080, 0x0080,
    0x0080, 0x0080, 0x0100, 0x0100, 0x0100, 0x0180, 0x0180, 0x0200, 0x0200, 0x0280, 0x0280, 0x0300,
    0x0300, 0x0380, 0x0400, 0x0400, 0x0480, 0x0500, 0x0580, 0x0580, 0x0600, 0x0680, 0x0700, 0x0780,
    0x0800, 0x0880, 0x0900, 0x0980, 0x0a00, 0x0a80, 0x0b80, 0x0c00, 0x0c80, 0x0d00, 0x0e00, 0x0e80,
    0x0f00, 0x1000, 0x1080, 0x1180, 0x1200, 0x1300, 0x1380, 0x1480, 0x1500, 0x1600, 0x1700, 0x1780,
    0x1880, 0x1980, 0x1a80, 0x1b00, 0x1c00, 0x1d00, 0x1e00, 0x1f00,
];

fn nmsedec_sig(x: u32, bitpos: u32) -> i32 {
    if bitpos > 0 {
        LUT_NMSEDEC_SIG[((x >> bitpos) & 127) as usize] as i32
    } else {
        LUT_NMSEDEC_SIG0[(x & 127) as usize] as i32
    }
}

fn nmsedec_ref(x: u32, bitpos: u32) -> i32 {
    if bitpos > 0 {
        LUT_NMSEDEC_REF[((x >> bitpos) & 127) as usize] as i32
    } else {
        LUT_NMSEDEC_REF0[(x & 127) as usize] as i32
    }
}

struct T1<'a> {
    w: usize,
    h: usize,
    /// Padded (w + 2) x (h + 2) state.
    sig: Vec<u8>,
    neg: Vec<u8>,
    visited: Vec<u8>,
    refined: Vec<u8>,
    /// Magnitudes with FRACBITS fractional bits, and signs.
    mag: &'a [u32],
    sign: &'a [u8],
    orient: u32,
    mqc: Mqc,
    nmsedec: i32,
}

impl T1<'_> {
    fn at(&self, x: usize, y: usize) -> usize {
        (y + 1) * (self.w + 2) + x + 1
    }

    fn bit(&self, index: usize, bpno: u32) -> u32 {
        (self.mag[index] >> (bpno + FRACBITS)) & 1
    }

    fn zc_context(&self, p: usize) -> usize {
        let s = &self.sig;
        let stride = self.w + 2;
        let mut h = s[p - 1] as u32 + s[p + 1] as u32;
        let mut v = s[p - stride] as u32 + s[p + stride] as u32;
        let d = s[p - stride - 1] as u32
            + s[p - stride + 1] as u32
            + s[p + stride - 1] as u32
            + s[p + stride + 1] as u32;
        if self.orient == 3 {
            let hv = h + v;
            return match d {
                0 => [0, 1, 2][hv.min(2) as usize],
                1 => [3, 4, 5][hv.min(2) as usize],
                2 => {
                    if hv == 0 {
                        6
                    } else {
                        7
                    }
                }
                _ => 8,
            };
        }
        // OpenJPEG's lut_ctxno_zc stores the swapped table under band 1 (HL).
        if self.orient == 1 {
            core::mem::swap(&mut h, &mut v);
        }
        match h {
            0 => match v {
                0 => [0, 1, 2][d.min(2) as usize],
                1 => 3,
                _ => 4,
            },
            1 => {
                if v != 0 {
                    7
                } else if d == 0 {
                    5
                } else {
                    6
                }
            }
            _ => 8,
        }
    }

    /// Sign coding context and XOR bit (T.800 Table D.3).
    fn sc_context(&self, p: usize) -> (usize, u32) {
        let stride = self.w + 2;
        let contribution = |q: usize| -> (i32, i32) {
            if self.sig[q] == 0 {
                (0, 0)
            } else if self.neg[q] == 0 {
                (1, 0)
            } else {
                (0, 1)
            }
        };
        let (ep, en) = contribution(p + 1);
        let (wp, wn) = contribution(p - 1);
        let (np, nn) = contribution(p - stride);
        let (sp, sn) = contribution(p + stride);
        let hc0 = (ep + wp).min(1) - (en + wn).min(1);
        let vc0 = (np + sp).min(1) - (nn + sn).min(1);
        let spb = if hc0 == 0 && vc0 == 0 {
            0
        } else {
            !(hc0 > 0 || (hc0 == 0 && vc0 > 0)) as u32
        };
        let (mut hc, mut vc) = (hc0, vc0);
        if hc < 0 {
            hc = -hc;
            vc = -vc;
        }
        let n = match (hc, vc) {
            (0, 0) => 0,
            (0, _) => 1,
            (_, -1) => 2,
            (_, 0) => 3,
            _ => 4,
        };
        (9 + n, spb)
    }

    fn any_neighbour(&self, p: usize) -> bool {
        let s = &self.sig;
        let stride = self.w + 2;
        s[p - 1]
            | s[p + 1]
            | s[p - stride]
            | s[p + stride]
            | s[p - stride - 1]
            | s[p - stride + 1]
            | s[p + stride - 1]
            | s[p + stride + 1]
            != 0
    }

    fn code_sign(&mut self, p: usize, index: usize, bpno: u32) {
        let (cx, spb) = self.sc_context(p);
        let v = self.sign[index] as u32;
        self.nmsedec += nmsedec_sig(self.mag[index], bpno);
        self.mqc.encode(cx, v ^ spb);
        self.sig[p] = 1;
        self.neg[p] = v as u8;
    }

    fn sigpass(&mut self, bpno: u32) {
        for k in (0..self.h).step_by(4) {
            let lim = (self.h - k).min(4);
            for i in 0..self.w {
                for ci in 0..lim {
                    let y = k + ci;
                    let p = self.at(i, y);
                    if self.sig[p] == 0 && self.visited[p] == 0 && self.any_neighbour(p) {
                        let index = y * self.w + i;
                        let v = self.bit(index, bpno);
                        let cx = self.zc_context(p);
                        self.mqc.encode(cx, v);
                        if v == 1 {
                            self.code_sign(p, index, bpno);
                        }
                        self.visited[p] = 1;
                    }
                }
            }
        }
    }

    fn refpass(&mut self, bpno: u32) {
        for k in (0..self.h).step_by(4) {
            let lim = (self.h - k).min(4);
            for i in 0..self.w {
                for ci in 0..lim {
                    let y = k + ci;
                    let p = self.at(i, y);
                    if self.sig[p] == 1 && self.visited[p] == 0 {
                        let index = y * self.w + i;
                        self.nmsedec += nmsedec_ref(self.mag[index], bpno);
                        let v = self.bit(index, bpno);
                        let cx = if self.refined[p] == 1 {
                            16
                        } else if self.any_neighbour(p) {
                            15
                        } else {
                            14
                        };
                        self.mqc.encode(cx, v);
                        self.refined[p] = 1;
                    }
                }
            }
        }
    }

    /// Run-length mode applies when the column and its 6x3 neighbourhood
    /// hold no significant or visited sample (OpenJPEG's `*f == 0`).
    fn column_is_quiet(&self, i: usize, k: usize) -> bool {
        let stride = self.w + 2;
        for ci in 0..4 {
            if self.visited[self.at(i, k + ci)] != 0 {
                return false;
            }
        }
        let top = self.at(i, k) - stride;
        for row in 0..6 {
            let q = top + row * stride;
            if self.sig[q - 1] | self.sig[q] | self.sig[q + 1] != 0 {
                return false;
            }
        }
        true
    }

    fn clnpass(&mut self, bpno: u32) {
        let full = self.h & !3;
        for k in (0..self.h).step_by(4) {
            let lim = (self.h - k).min(4);
            for i in 0..self.w {
                let mut runlen = 0;
                let agg = k < full && self.column_is_quiet(i, k);
                if agg {
                    while runlen < 4 && self.bit((k + runlen) * self.w + i, bpno) == 0 {
                        runlen += 1;
                    }
                    self.mqc.encode(CTX_AGG, (runlen != 4) as u32);
                    if runlen == 4 {
                        continue;
                    }
                    self.mqc.encode(CTX_UNI, (runlen >> 1) as u32);
                    self.mqc.encode(CTX_UNI, (runlen & 1) as u32);
                }
                for ci in runlen..lim {
                    let y = k + ci;
                    let p = self.at(i, y);
                    let index = y * self.w + i;
                    if agg && ci == runlen {
                        self.code_sign(p, index, bpno);
                    } else if self.sig[p] == 0 && self.visited[p] == 0 {
                        let v = self.bit(index, bpno);
                        let cx = self.zc_context(p);
                        self.mqc.encode(cx, v);
                        if v == 1 {
                            self.code_sign(p, index, bpno);
                        }
                    }
                    self.visited[p] = 0;
                }
            }
        }
    }
}

/// Per code-block constants of `opj_t1_getwmsedec` (no MCT: w1 = 1).
struct Distortion {
    irreversible: bool,
    level: u32,
    orient: u32,
    stepsize: f64,
}

impl Distortion {
    fn wmsedec(&self, nmsedec: i32, bpno: u32) -> f64 {
        let mut stepsize = self.stepsize;
        let level = self.level.min(9) as usize;
        let w2 = if self.irreversible {
            stepsize /= (1u32 << log2_gain(self.orient)) as f64;
            NORMS_REAL[self.orient as usize][level]
        } else {
            NORMS[self.orient as usize][level]
        };
        let mut wmsedec = 1.0 * w2 * stepsize * (1u32 << bpno) as f64;
        wmsedec *= wmsedec * nmsedec as f64 / 8192.0;
        wmsedec
    }
}

/// Encodes one code-block (`opj_t1_encode_cblk`) from its tier-1 values
/// (coefficients scaled by 2^FRACBITS), filling its passes and data.
fn encode_code_block(
    cblk: &mut CodeBlock,
    values: &[i32],
    w: usize,
    h: usize,
    orient: u32,
    distortion: &Distortion,
) {
    let mut max = 0i32;
    let mut mag = Vec::with_capacity(values.len());
    let mut sign = Vec::with_capacity(values.len());
    for &v in values {
        let v = if v == i32::MIN { i32::MIN + 1 } else { v };
        max = max.max(v.abs());
        mag.push(v.unsigned_abs());
        sign.push((v < 0) as u8);
    }
    cblk.numbps = if max == 0 {
        0
    } else {
        floorlog2(max as u32) as i32 + 1 - FRACBITS as i32
    };
    if cblk.numbps <= 0 {
        return;
    }
    let padded = (w + 2) * (h + 2);
    let mut t1 = T1 {
        w,
        h,
        sig: vec![0; padded],
        neg: vec![0; padded],
        visited: vec![0; padded],
        refined: vec![0; padded],
        mag: &mag,
        sign: &sign,
        orient,
        mqc: Mqc::new(),
        nmsedec: 0,
    };
    let mut cumulative = 0.0f64;
    let mut bpno = cblk.numbps - 1;
    let mut passtype = 2;
    while bpno >= 0 {
        t1.nmsedec = 0;
        match passtype {
            0 => t1.sigpass(bpno as u32),
            1 => t1.refpass(bpno as u32),
            _ => t1.clnpass(bpno as u32),
        }
        cumulative += distortion.wmsedec(t1.nmsedec, bpno as u32);
        let rate = if passtype == 2 && bpno == 0 {
            t1.mqc.flush();
            t1.mqc.numbytes()
        } else {
            t1.mqc.numbytes().wrapping_add(3)
        };
        cblk.passes.push(Pass {
            rate,
            len: 0,
            distortion: cumulative,
        });
        passtype += 1;
        if passtype == 3 {
            passtype = 0;
            bpno -= 1;
        }
    }
    let mut last = t1.mqc.numbytes();
    for pass in cblk.passes.iter_mut().rev() {
        if pass.rate > last {
            pass.rate = last;
        } else {
            last = pass.rate;
        }
    }
    let data = &t1.mqc.buf[1..];
    let mut previous = 0u32;
    for pass in &mut cblk.passes {
        if pass.rate > 0 && data.get(pass.rate as usize - 1) == Some(&0xff) {
            pass.rate -= 1;
        }
        pass.len = pass.rate.wrapping_sub(previous);
        previous = pass.rate;
    }
    cblk.data = data.to_vec();
}

fn encode_tile_component(tc: &mut TileComponent, irreversible: bool) {
    let w = tc.width;
    let numres = tc.resolutions.len() as u32;
    for resno in 0..tc.resolutions.len() {
        let (pw, ph) = if resno > 0 {
            let prev = &tc.resolutions[resno - 1];
            (prev.x1 - prev.x0, prev.y1 - prev.y0)
        } else {
            (0, 0)
        };
        let ints = &tc.ints;
        let floats = &tc.floats;
        let res = &mut tc.resolutions[resno];
        for band in &mut res.bands {
            if band.is_empty() {
                continue;
            }
            let offx = if band.bandno & 1 != 0 { pw } else { 0 };
            let offy = if band.bandno & 2 != 0 { ph } else { 0 };
            let distortion = Distortion {
                irreversible,
                level: numres - 1 - resno as u32,
                orient: band.bandno,
                stepsize: band.stepsize as f64,
            };
            let stepsize = band.stepsize;
            for cblk in &mut band.cblks {
                let cw = (cblk.x1 - cblk.x0) as usize;
                let chh = (cblk.y1 - cblk.y0) as usize;
                let x = (cblk.x0 - band.x0 + offx) as usize;
                let y = (cblk.y0 - band.y0 + offy) as usize;
                let mut values = Vec::with_capacity(cw * chh);
                for row in 0..chh {
                    let at = (y + row) * w + x;
                    if irreversible {
                        // opj_lrintf((v / stepsize) * (1 << T1_NMSEDEC_FRACBITS)).
                        values.extend(floats[at..at + cw].iter().map(|&v| {
                            ((v / stepsize) * (1u32 << FRACBITS) as f32).round_ties_even() as i32
                        }));
                    } else {
                        values.extend(
                            ints[at..at + cw]
                                .iter()
                                .map(|&v| (v as u32).wrapping_shl(FRACBITS) as i32),
                        );
                    }
                }
                encode_code_block(cblk, &values, cw, chh, band.bandno, &distortion);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rate allocation (opj_tcd_rateallocate, opj_tcd_makelayer)

fn all_cblks(tiles: &[TileComponent]) -> impl Iterator<Item = &CodeBlock> {
    tiles
        .iter()
        .flat_map(|tc| tc.resolutions.iter())
        .flat_map(|r| r.bands.iter().filter(|b| !b.is_empty()))
        .flat_map(|b| b.cblks.iter())
}

/// Minimum and maximum rate-distortion slopes over all passes.
fn slopes(tiles: &[TileComponent]) -> (f64, f64) {
    let mut min = f64::MAX;
    let mut max = 0.0f64;
    for cblk in all_cblks(tiles) {
        for (passno, pass) in cblk.passes.iter().enumerate() {
            let (dr, dd) = if passno == 0 {
                (pass.rate as i32, pass.distortion)
            } else {
                let prev = &cblk.passes[passno - 1];
                (
                    pass.rate.wrapping_sub(prev.rate) as i32,
                    pass.distortion - prev.distortion,
                )
            };
            if dr == 0 {
                continue;
            }
            let slope = dd / dr as f64;
            if slope < min {
                min = slope;
            }
            if slope > max {
                max = slope;
            }
        }
    }
    (min, max)
}

/// `opj_tcd_makelayer` for the first (only) layer: returns whether the
/// allocation equals the previous one. A negative threshold takes all passes.
fn make_layer(tiles: &mut [TileComponent], thresh: f64) -> bool {
    let mut same = true;
    for cblk in tiles
        .iter_mut()
        .flat_map(|tc| tc.resolutions.iter_mut())
        .flat_map(|r| r.bands.iter_mut().filter(|b| !b.is_empty()))
        .flat_map(|b| b.cblks.iter_mut())
    {
        let mut n = 0usize;
        if thresh < 0.0 {
            n = cblk.passes.len();
        } else {
            for passno in 0..cblk.passes.len() {
                let pass = cblk.passes[passno];
                let (dr, dd) = if n == 0 {
                    (pass.rate, pass.distortion)
                } else {
                    let prev = cblk.passes[n - 1];
                    (
                        pass.rate.wrapping_sub(prev.rate),
                        pass.distortion - prev.distortion,
                    )
                };
                if dr == 0 {
                    if dd != 0.0 {
                        n = passno + 1;
                    }
                    continue;
                }
                // OpenJPEG compares without abs(): a margin, not equality.
                #[allow(clippy::float_equality_without_abs)]
                if thresh - (dd / dr as f64) < f64::EPSILON {
                    n = passno + 1;
                }
            }
        }
        if cblk.layer_passes != n {
            same = false;
            cblk.layer_passes = n;
        }
    }
    same
}

// ---------------------------------------------------------------------------
// Tier-2 (t2.c, tgt.c, bio.c)

struct BitWriter {
    out: Vec<u8>,
    buf: u32,
    ct: u32,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter {
            out: Vec::new(),
            buf: 0,
            ct: 8,
        }
    }

    fn byteout(&mut self) {
        self.buf = (self.buf << 8) & 0xffff;
        self.ct = if self.buf == 0xff00 { 7 } else { 8 };
        self.out.push((self.buf >> 8) as u8);
    }

    fn putbit(&mut self, b: u32) {
        if self.ct == 0 {
            self.byteout();
        }
        self.ct -= 1;
        self.buf |= b << self.ct;
    }

    fn write(&mut self, v: u32, n: u32) {
        for i in (0..n).rev() {
            self.putbit((v >> i) & 1);
        }
    }

    fn flush(mut self) -> Vec<u8> {
        self.byteout();
        if self.ct == 7 {
            self.byteout();
        }
        self.out
    }
}

#[derive(Clone, Copy)]
struct TagNode {
    parent: Option<usize>,
    value: i32,
    low: i32,
    known: bool,
}

struct TagTree {
    nodes: Vec<TagNode>,
}

impl TagTree {
    fn new(w: u32, h: u32) -> Self {
        let mut levels = vec![(w, h)];
        while levels.last().is_some_and(|&(a, b)| a * b > 1) {
            let &(a, b) = levels.last().unwrap();
            levels.push((a.div_ceil(2), b.div_ceil(2)));
        }
        let mut offsets = Vec::new();
        let mut total = 0;
        for &(a, b) in &levels {
            offsets.push(total);
            total += (a * b) as usize;
        }
        let mut nodes = vec![
            TagNode {
                parent: None,
                value: 999,
                low: 0,
                known: false
            };
            total
        ];
        for l in 0..levels.len() - 1 {
            let (a, b) = levels[l];
            let (pa, _) = levels[l + 1];
            for j in 0..b {
                for i in 0..a {
                    let index = offsets[l] + (j * a + i) as usize;
                    nodes[index].parent = Some(offsets[l + 1] + ((j / 2) * pa + i / 2) as usize);
                }
            }
        }
        TagTree { nodes }
    }

    fn set_value(&mut self, leaf: usize, value: i32) {
        let mut node = Some(leaf);
        while let Some(n) = node {
            if self.nodes[n].value <= value {
                break;
            }
            self.nodes[n].value = value;
            node = self.nodes[n].parent;
        }
    }

    fn encode(&mut self, bio: &mut BitWriter, leaf: usize, threshold: i32) {
        let mut stack = Vec::new();
        let mut node = leaf;
        while let Some(parent) = self.nodes[node].parent {
            stack.push(node);
            node = parent;
        }
        let mut low = 0;
        loop {
            if low > self.nodes[node].low {
                self.nodes[node].low = low;
            } else {
                low = self.nodes[node].low;
            }
            while low < threshold {
                if low >= self.nodes[node].value {
                    if !self.nodes[node].known {
                        bio.putbit(1);
                        self.nodes[node].known = true;
                    }
                    break;
                }
                bio.putbit(0);
                low += 1;
            }
            self.nodes[node].low = low;
            match stack.pop() {
                Some(next) => node = next,
                None => break,
            }
        }
    }
}

fn put_num_passes(bio: &mut BitWriter, n: u32) {
    match n {
        1 => bio.putbit(0),
        2 => bio.write(2, 2),
        3..=5 => bio.write(0xc | (n - 3), 4),
        6..=36 => bio.write(0x1e0 | (n - 6), 9),
        _ => bio.write(0xff80 | (n - 37), 16),
    }
}

/// The packet of a resolution in the single layer; with `out` absent only
/// its size is computed (THRESH_CALC).
fn encode_packet(res: &Resolution, mut out: Option<&mut Vec<u8>>) -> usize {
    let mut bio = BitWriter::new();
    bio.putbit(1);
    for band in res.bands.iter().filter(|b| !b.is_empty()) {
        let mut incl = TagTree::new(band.cw, band.ch);
        let mut imsb = TagTree::new(band.cw, band.ch);
        for (i, cblk) in band.cblks.iter().enumerate() {
            imsb.set_value(i, band.numbps - cblk.numbps);
        }
        for (i, cblk) in band.cblks.iter().enumerate() {
            if cblk.layer_passes != 0 {
                incl.set_value(i, 0);
            }
        }
        for (i, cblk) in band.cblks.iter().enumerate() {
            incl.encode(&mut bio, i, 1);
            let numpasses = cblk.layer_passes as u32;
            if numpasses == 0 {
                continue;
            }
            imsb.encode(&mut bio, i, 999);
            put_num_passes(&mut bio, numpasses);
            // Only the last included pass ends a segment (style 0).
            let len = cblk.passes[..numpasses as usize]
                .iter()
                .fold(0u32, |a, p| a.wrapping_add(p.len));
            let numlenbits = 3i32;
            let increment =
                (floorlog2(len) as i32 + 1 - (numlenbits + floorlog2(numpasses) as i32)).max(0);
            for _ in 0..increment {
                bio.putbit(1);
            }
            bio.putbit(0);
            bio.write(len, (numlenbits + increment) as u32 + floorlog2(numpasses));
        }
    }
    let header = bio.flush();
    let mut size = header.len();
    if let Some(out) = out.as_deref_mut() {
        out.extend_from_slice(&header);
    }
    for band in res.bands.iter().filter(|b| !b.is_empty()) {
        for cblk in band.cblks.iter().filter(|c| c.layer_passes != 0) {
            let len = cblk.passes[cblk.layer_passes - 1].rate as usize;
            size += len;
            if let Some(out) = out.as_deref_mut() {
                out.extend_from_slice(&cblk.data[..len]);
            }
        }
    }
    size
}

/// All packets in LRCP order; `None` when they exceed `max_len`.
fn encode_packets(
    tiles: &[TileComponent],
    max_len: u64,
    mut out: Option<&mut Vec<u8>>,
) -> Option<usize> {
    let mut total = 0usize;
    for resno in 0..NUM_RESOLUTIONS as usize {
        for tc in tiles {
            let res = &tc.resolutions[resno];
            if res.precincts == 0 {
                continue;
            }
            total += encode_packet(res, out.as_deref_mut());
            if total as u64 > max_len {
                return None;
            }
        }
    }
    Some(total)
}

// ---------------------------------------------------------------------------
// Codestream

fn marker(out: &mut Vec<u8>, code: u8, body: &[u8]) {
    out.extend_from_slice(&[0xff, code]);
    out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(body);
}

fn main_header(
    components: &[Component<'_>],
    width: u32,
    height: u32,
    irreversible: bool,
) -> Vec<u8> {
    let mut out = vec![0xff, 0x4f];
    let mut siz = Vec::new();
    siz.extend_from_slice(&0u16.to_be_bytes());
    for v in [width, height, 0, 0, width, height, 0, 0] {
        siz.extend_from_slice(&v.to_be_bytes());
    }
    siz.extend_from_slice(&(components.len() as u16).to_be_bytes());
    for c in components {
        siz.extend_from_slice(&[(c.precision - 1) as u8, c.dx as u8, c.dy as u8]);
    }
    marker(&mut out, 0x51, &siz);
    let cblk = (CBLK_EXPONENT - 2) as u8;
    marker(
        &mut out,
        0x52,
        &[
            0,
            0,
            0,
            1,
            0,
            (NUM_RESOLUTIONS - 1) as u8,
            cblk,
            cblk,
            0,
            !irreversible as u8,
        ],
    );
    let precision = components[0].precision;
    let mut qcd = vec![(GUARD_BITS << 5) as u8 | if irreversible { 2 } else { 0 }];
    for resno in 0..NUM_RESOLUTIONS {
        let level = NUM_RESOLUTIONS - 1 - resno;
        let orients: &[u32] = if resno == 0 { &[0] } else { &[1, 2, 3] };
        for &orient in orients {
            let (expn, mant) = band_step(precision, level, orient, irreversible);
            if irreversible {
                qcd.extend_from_slice(&(((expn << 11) | mant) as u16).to_be_bytes());
            } else {
                qcd.push((expn << 3) as u8);
            }
        }
    }
    marker(&mut out, 0x5c, &qcd);
    let mut com = vec![0, 1];
    com.extend_from_slice(b"Created by OpenJPEG version ");
    com.extend_from_slice(OPENJPEG_VERSION.as_bytes());
    marker(&mut out, 0x64, &com);
    out
}

/// Encodes the components into a J2K codestream as libheif's OpenJPEG
/// plugin does.
pub fn encode(
    components: &[Component<'_>],
    width: u32,
    height: u32,
    settings: Settings,
) -> Result<Vec<u8>, EncodeError> {
    if width < 1 << (NUM_RESOLUTIONS - 1) || height < 1 << (NUM_RESOLUTIONS - 1) {
        return Err(EncodeError::StartCompress);
    }
    let irreversible = settings.irreversible;
    let mut out = main_header(components, width, height, irreversible);

    // opj_j2k_setup_encoder and opj_j2k_update_rates: the layer's byte budget
    // from the rate, less the main header.
    let mut rate = if irreversible {
        (1 + (100 - settings.quality) / 2) as f32
    } else {
        0.0
    };
    if rate <= 1.0 {
        rate = 0.0;
    }
    if rate > 0.0 {
        let size_pixel = components.len() as u32 * components[0].precision;
        let bits_empty = (8 * components[0].dx * components[0].dy) as f32;
        rate = ((size_pixel as f64 * width as f64 * height as f64) / (rate * bits_empty) as f64)
            as f32;
        rate -= out.len() as f32;
        if rate < 30.0 {
            rate = 30.0;
        }
    }

    // opj_j2k_update_rates sizes the tile buffer from the raw sample bits,
    // plus the tile-part header, POC and per-component COC/QCC reserves;
    // the packets must fit after SOT and the 4 bytes reserved at SOD.
    let bits: u64 = components
        .iter()
        .map(|c| {
            u64::from(width.div_ceil(c.dx))
                * u64::from(height.div_ceil(c.dy))
                * u64::from(c.precision)
        })
        .sum();
    let mut tile_size = (bits as f64 * 1.4 / 8.0) as u64 + 500;
    tile_size += 12 + 13 + (components.len() as u64 - 1) * 2 * 11;
    let len = tile_size.min(u64::from(u32::MAX)).saturating_sub(12 + 4);

    let mut tiles: Vec<TileComponent> = components
        .iter()
        .map(|c| build_tile_component(c, width, height, settings))
        .collect();
    for tc in &mut tiles {
        dwt_forward(tc, irreversible);
        encode_tile_component(tc, irreversible);
    }

    let goodthresh = if rate > 0.0 {
        let maxlen = ((rate as f64).ceil() as u64).min(len);
        let (mut lo, mut hi) = slopes(&tiles);
        let mut thresh = 0.0f64;
        let mut stable = 0.0f64;
        let mut last_ok = false;
        for i in 0..128 {
            let new_thresh = (lo + hi) / 2.0;
            if (new_thresh - thresh).abs() <= 0.5 * 1e-5 * thresh {
                break;
            }
            thresh = new_thresh;
            let same = make_layer(&mut tiles, thresh) && i != 0;
            if (same && !last_ok) || (!same && encode_packets(&tiles, maxlen, None).is_none()) {
                last_ok = false;
                lo = thresh;
                continue;
            }
            last_ok = true;
            hi = thresh;
            stable = thresh;
        }
        if stable == 0.0 { thresh } else { stable }
    } else {
        -1.0
    };
    make_layer(&mut tiles, goodthresh);

    let mut packets = Vec::new();
    encode_packets(&tiles, len, Some(&mut packets)).ok_or(EncodeError::Encode)?;
    let psot = (12 + 2 + packets.len()) as u32;
    out.extend_from_slice(&[0xff, 0x90, 0, 10, 0, 0]);
    out.extend_from_slice(&psot.to_be_bytes());
    out.extend_from_slice(&[0, 1, 0xff, 0x93]);
    out.extend_from_slice(&packets);
    out.extend_from_slice(&[0xff, 0xd9]);
    Ok(out)
}
