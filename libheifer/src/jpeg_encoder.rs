// SPDX-License-Identifier: LGPL-3.0-or-later
//! Baseline JPEG encoding as libheif's JPEG encoder plugin produces it with
//! libjpeg-turbo (scalar build): 4:2:0 YCbCr input, the islow forward DCT,
//! reciprocal quantization, the standard Huffman tables and a JFIF header.
//!
//! Tables and arithmetic follow libjpeg-turbo's jcparam.c, jfdctint.c,
//! jcdctmgr.c, jchuff.c, jstdhuff.c, jcmarker.c, jcsample.c, jcprepct.c and
//! jccoefct.c (Independent JPEG Group; libjpeg-turbo contributors).

/// Zigzag position to natural (row-major) position.
const NATURAL_ORDER: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

const STD_LUMINANCE_QUANT: [u32; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const STD_CHROMINANCE_QUANT: [u32; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

const BITS_DC_LUMINANCE: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const VAL_DC_LUMINANCE: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const BITS_DC_CHROMINANCE: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const VAL_DC_CHROMINANCE: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const BITS_AC_LUMINANCE: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125];
const VAL_AC_LUMINANCE: [u8; 162] = [
    1, 2, 3, 0, 4, 17, 5, 18, 33, 49, 65, 6, 19, 81, 97, 7, 34, 113, 20, 50, 129, 145, 161, 8, 35,
    66, 177, 193, 21, 82, 209, 240, 36, 51, 98, 114, 130, 9, 10, 22, 23, 24, 25, 26, 37, 38, 39,
    40, 41, 42, 52, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88,
    89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 131,
    132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164,
    165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197,
    198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 225, 226, 227, 228, 229,
    230, 231, 232, 233, 234, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250,
];
const BITS_AC_CHROMINANCE: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119];
const VAL_AC_CHROMINANCE: [u8; 162] = [
    0, 1, 2, 3, 17, 4, 5, 33, 49, 6, 18, 65, 81, 7, 97, 113, 19, 34, 50, 129, 8, 20, 66, 145, 161,
    177, 193, 9, 35, 51, 82, 240, 21, 98, 114, 209, 10, 22, 36, 52, 225, 37, 241, 23, 24, 25, 26,
    38, 39, 40, 41, 42, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87,
    88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 130,
    131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163,
    164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196,
    197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 226, 227, 228, 229,
    230, 231, 232, 233, 234, 242, 243, 244, 245, 246, 247, 248, 249, 250,
];

/// `jpeg_quality_scaling` and `jpeg_add_quant_table` with forced baseline.
fn quant_table(basic: &[u32; 64], quality: i32) -> [u16; 64] {
    let quality = quality.clamp(1, 100);
    let scale = if quality < 50 {
        5000 / quality
    } else {
        200 - quality * 2
    } as i64;
    let mut table = [0u16; 64];
    for (out, &value) in table.iter_mut().zip(basic) {
        *out = ((i64::from(value) * scale + 50) / 100).clamp(1, 255) as u16;
    }
    table
}

/// `compute_reciprocal` for a 32-bit DCTELEM: (reciprocal, correction, shift).
fn reciprocal(divisor: u16) -> (u32, u32, i32) {
    if divisor == 1 {
        return (1, 0, -32);
    }
    let b = 15 - divisor.leading_zeros() as i32; // flss(divisor) - 1
    let mut r = 32 + b;
    let mut fq = (1u64 << r) / u64::from(divisor);
    let fr = (1u64 << r) % u64::from(divisor);
    let mut c = u32::from(divisor / 2);
    if fr == 0 {
        fq >>= 1;
        r -= 1;
    } else if fr <= u64::from(divisor / 2) {
        c += 1;
    } else {
        fq += 1;
    }
    (fq as u32, c, r - 32)
}

struct Divisors([(u32, u32, i32); 64]);

impl Divisors {
    fn new(table: &[u16; 64]) -> Self {
        Divisors(std::array::from_fn(|i| reciprocal(table[i] << 3)))
    }

    /// `quantize`, 8-bit C version.
    fn quantize(&self, workspace: &[i32; 64], out: &mut [i16; 64]) {
        for i in 0..64 {
            let (recip, corr, shift) = self.0[i];
            let temp = workspace[i];
            let magnitude = temp.unsigned_abs().wrapping_add(corr);
            let product = u64::from(magnitude) * u64::from(recip);
            let q = (product >> (shift + 32)) as i32;
            out[i] = if temp < 0 { -q } else { q } as i16;
        }
    }
}

/// `jpeg_fdct_islow` (CONST_BITS 13, PASS1_BITS 2).
fn fdct_islow(data: &mut [i32; 64]) {
    const CONST_BITS: i32 = 13;
    const PASS1_BITS: i32 = 2;
    let descale = |x: i64, n: i32| ((x + (1 << (n - 1))) >> n) as i32;
    for pass in 0..2 {
        for ctr in 0..8 {
            let at = |k: usize| if pass == 0 { ctr * 8 + k } else { k * 8 + ctr };
            let d = |k: usize| i64::from(data[at(k)]);
            let tmp0 = d(0) + d(7);
            let tmp7 = d(0) - d(7);
            let tmp1 = d(1) + d(6);
            let tmp6 = d(1) - d(6);
            let tmp2 = d(2) + d(5);
            let tmp5 = d(2) - d(5);
            let tmp3 = d(3) + d(4);
            let tmp4 = d(3) - d(4);
            let tmp10 = tmp0 + tmp3;
            let tmp13 = tmp0 - tmp3;
            let tmp11 = tmp1 + tmp2;
            let tmp12 = tmp1 - tmp2;
            let (even, odd) = if pass == 0 {
                (CONST_BITS - PASS1_BITS, CONST_BITS - PASS1_BITS)
            } else {
                (CONST_BITS + PASS1_BITS, CONST_BITS + PASS1_BITS)
            };
            if pass == 0 {
                data[at(0)] = ((tmp10 + tmp11) << PASS1_BITS) as i32;
                data[at(4)] = ((tmp10 - tmp11) << PASS1_BITS) as i32;
            } else {
                data[at(0)] = descale(tmp10 + tmp11, PASS1_BITS);
                data[at(4)] = descale(tmp10 - tmp11, PASS1_BITS);
            }
            let z1 = (tmp12 + tmp13) * 4433;
            data[at(2)] = descale(z1 + tmp13 * 6270, even);
            data[at(6)] = descale(z1 + tmp12 * -15137, even);
            let z1 = tmp4 + tmp7;
            let z2 = tmp5 + tmp6;
            let z3 = tmp4 + tmp6;
            let z4 = tmp5 + tmp7;
            let z5 = (z3 + z4) * 9633;
            let tmp4 = tmp4 * 2446;
            let tmp5 = tmp5 * 16819;
            let tmp6 = tmp6 * 25172;
            let tmp7 = tmp7 * 12299;
            let z1 = z1 * -7373;
            let z2 = z2 * -20995;
            let z3 = z3 * -16069 + z5;
            let z4 = z4 * -3196 + z5;
            data[at(7)] = descale(tmp4 + z1 + z3, odd);
            data[at(5)] = descale(tmp5 + z2 + z4, odd);
            data[at(3)] = descale(tmp6 + z2 + z3, odd);
            data[at(1)] = descale(tmp7 + z1 + z4, odd);
        }
    }
}

/// Canonical Huffman codes (`jpeg_make_c_derived_tbl`): (code, length) per symbol.
fn derive(bits: &[u8; 16], values: &[u8]) -> [(u16, u8); 256] {
    let mut table = [(0u16, 0u8); 256];
    let mut code = 0u16;
    let mut k = 0;
    for (length, &count) in bits.iter().enumerate() {
        for _ in 0..count {
            table[usize::from(values[k])] = (code, length as u8 + 1);
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    table
}

struct BitWriter {
    out: Vec<u8>,
    buffer: u64,
    bits: u32,
}

impl BitWriter {
    fn put(&mut self, code: u32, size: u32) {
        self.buffer = (self.buffer << size) | u64::from(code & ((1u32 << size) - 1));
        self.bits += size;
        while self.bits >= 8 {
            self.bits -= 8;
            let byte = (self.buffer >> self.bits) as u8;
            self.out.push(byte);
            if byte == 0xFF {
                self.out.push(0);
            }
        }
    }

    /// `flush_bits`: fill the partial byte with ones.
    fn flush(&mut self) {
        if self.bits > 0 {
            let byte = ((self.buffer << (8 - self.bits)) as u8) | (0xFF >> self.bits);
            self.out.push(byte);
            if byte == 0xFF {
                self.out.push(0);
            }
        }
        self.bits = 0;
        self.buffer = 0;
    }
}

/// `encode_one_block`.
fn encode_block(
    w: &mut BitWriter,
    block: &[i16; 64],
    last_dc: &mut i32,
    dc: &[(u16, u8); 256],
    ac: &[(u16, u8); 256],
) {
    let nbits = |v: i32| 32 - v.unsigned_abs().leading_zeros();
    // Negative values are sent as v - 1 in n bits.
    let value_bits = |v: i32, n: u32| (if v < 0 { v - 1 } else { v }) as u32 & ((1u32 << n) - 1);
    let diff = i32::from(block[0]) - *last_dc;
    *last_dc = i32::from(block[0]);
    let n = nbits(diff);
    let (code, size) = dc[n as usize];
    w.put(u32::from(code), u32::from(size));
    if n > 0 {
        w.put(value_bits(diff, n), n);
    }
    let mut run = 0;
    for &natural in &NATURAL_ORDER[1..] {
        let v = i32::from(block[natural]);
        if v == 0 {
            run += 1;
            continue;
        }
        while run > 15 {
            let (code, size) = ac[0xF0];
            w.put(u32::from(code), u32::from(size));
            run -= 16;
        }
        let n = nbits(v);
        let (code, size) = ac[(run << 4) + n as usize];
        w.put(u32::from(code), u32::from(size));
        w.put(value_bits(v, n), n);
        run = 0;
    }
    if run > 0 {
        let (code, size) = ac[0];
        w.put(u32::from(code), u32::from(size));
    }
}

/// A component's samples, padded by edge replication to whole blocks and
/// whole MCU rows (`expand_right_edge` / `expand_bottom_edge`).
struct Samples {
    data: Vec<u8>,
    width: usize,
}

impl Samples {
    fn new(
        plane: &[u8],
        stride: usize,
        width: usize,
        height: usize,
        padded_w: usize,
        padded_h: usize,
    ) -> Self {
        let mut data = vec![0u8; padded_w * padded_h];
        for y in 0..padded_h {
            let row = &plane[y.min(height - 1) * stride..];
            for x in 0..padded_w {
                data[y * padded_w + x] = row[x.min(width - 1)];
            }
        }
        Samples {
            data,
            width: padded_w,
        }
    }

    fn block(&self, bx: usize, by: usize) -> [i32; 64] {
        std::array::from_fn(|i| {
            i32::from(self.data[(by * 8 + i / 8) * self.width + bx * 8 + i % 8]) - 128
        })
    }
}

/// One 8-bit plane: samples, row stride, width, height.
pub struct PlaneRef<'a> {
    pub data: &'a [u8],
    pub stride: usize,
    pub width: usize,
    pub height: usize,
}

/// Encodes a 4:2:0 YCbCr image. `density` is the JFIF pixel aspect ratio,
/// written with density unit 0 (libjpeg's default is 1:1).
pub fn encode(
    y: &PlaneRef,
    cb: &PlaneRef,
    cr: &PlaneRef,
    quality: i32,
    density: (u16, u16),
) -> Vec<u8> {
    let (width, height) = (y.width, y.height);
    let tables = [
        quant_table(&STD_LUMINANCE_QUANT, quality),
        quant_table(&STD_CHROMINANCE_QUANT, quality),
    ];
    let mut out = vec![0xFF, 0xD8];
    // JFIF APP0 1.01
    out.extend_from_slice(&[0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 0]);
    out.extend_from_slice(&density.0.to_be_bytes());
    out.extend_from_slice(&density.1.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    for (index, table) in tables.iter().enumerate() {
        out.extend_from_slice(&[0xFF, 0xDB, 0, 67, index as u8]);
        out.extend(NATURAL_ORDER.iter().map(|&n| table[n] as u8));
    }
    out.extend_from_slice(&[0xFF, 0xC0, 0, 17, 8]);
    out.extend_from_slice(&(height as u16).to_be_bytes());
    out.extend_from_slice(&(width as u16).to_be_bytes());
    out.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    let huffman: [(&[u8; 16], &[u8], u8); 4] = [
        (&BITS_DC_LUMINANCE, &VAL_DC_LUMINANCE, 0x00),
        (&BITS_AC_LUMINANCE, &VAL_AC_LUMINANCE, 0x10),
        (&BITS_DC_CHROMINANCE, &VAL_DC_CHROMINANCE, 0x01),
        (&BITS_AC_CHROMINANCE, &VAL_AC_CHROMINANCE, 0x11),
    ];
    for (bits, values, index) in huffman {
        let length = 2 + 1 + 16 + values.len();
        out.extend_from_slice(&[0xFF, 0xC4]);
        out.extend_from_slice(&(length as u16).to_be_bytes());
        out.push(index);
        out.extend_from_slice(bits);
        out.extend_from_slice(values);
    }
    out.extend_from_slice(&[0xFF, 0xDA, 0, 12, 3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0]);

    let dc = [
        derive(&BITS_DC_LUMINANCE, &VAL_DC_LUMINANCE),
        derive(&BITS_DC_CHROMINANCE, &VAL_DC_CHROMINANCE),
    ];
    let ac = [
        derive(&BITS_AC_LUMINANCE, &VAL_AC_LUMINANCE),
        derive(&BITS_AC_CHROMINANCE, &VAL_AC_CHROMINANCE),
    ];
    let divisors = [Divisors::new(&tables[0]), Divisors::new(&tables[1])];
    let mcus_x = width.div_ceil(16);
    let mcus_y = height.div_ceil(16);
    // Blocks each component really has; MCU positions beyond them are dummy
    // blocks (zero AC, the previous block's DC).
    let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
    let luma_blocks = (width.div_ceil(8), height.div_ceil(8));
    let chroma_blocks = (cw.div_ceil(8), ch.div_ceil(8));
    let luma = Samples::new(
        y.data,
        y.stride,
        width,
        height,
        luma_blocks.0 * 8,
        mcus_y * 16,
    );
    let chroma = [
        Samples::new(cb.data, cb.stride, cw, ch, chroma_blocks.0 * 8, mcus_y * 8),
        Samples::new(cr.data, cr.stride, cw, ch, chroma_blocks.0 * 8, mcus_y * 8),
    ];
    let mut writer = BitWriter {
        out,
        buffer: 0,
        bits: 0,
    };
    let mut last_dc = [0i32; 3];
    let mut previous = [0i16; 3];
    let code = |samples: &Samples,
                bx: usize,
                by: usize,
                real: bool,
                d: &Divisors,
                prev: &mut i16|
     -> [i16; 64] {
        let mut block = [0i16; 64];
        if real {
            let mut workspace = samples.block(bx, by);
            fdct_islow(&mut workspace);
            d.quantize(&workspace, &mut block);
        } else {
            block[0] = *prev;
        }
        *prev = block[0];
        block
    };
    for my in 0..mcus_y {
        for mx in 0..mcus_x {
            for v in 0..2 {
                for h in 0..2 {
                    let (bx, by) = (mx * 2 + h, my * 2 + v);
                    // A dummy row repeats the DC of the block above it in the MCU.
                    let real = bx < luma_blocks.0 && by < luma_blocks.1;
                    let block = code(&luma, bx, by, real, &divisors[0], &mut previous[0]);
                    encode_block(&mut writer, &block, &mut last_dc[0], &dc[0], &ac[0]);
                }
            }
            for c in 0..2 {
                let real = mx < chroma_blocks.0 && my < chroma_blocks.1;
                let block = code(&chroma[c], mx, my, real, &divisors[1], &mut previous[c + 1]);
                encode_block(&mut writer, &block, &mut last_dc[c + 1], &dc[1], &ac[1]);
            }
        }
    }
    writer.flush();
    let mut out = writer.out;
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}
