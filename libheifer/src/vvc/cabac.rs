// SPDX-License-Identifier: LGPL-3.0-or-later
//! VVC arithmetic decoding engine (H.266 clause 9.3.4.3) with the dual-rate
//! probability estimator, equivalent to vvdec's `BinDecoder`.
use super::ctx::{INIT, NUM_CTX};

#[derive(Clone, Copy, Default)]
pub struct Model {
    s0: u16,
    s1: u16,
    shift0: u8,
    shift1: u8,
}

impl Model {
    fn init(init_value: u8, window: u8, qp: i32) -> Self {
        let slope = i32::from(init_value >> 3) - 4;
        let offset = i32::from(init_value & 7) * 18 + 1;
        let state = (((slope * (qp - 16)) >> 1) + offset).clamp(1, 127);
        let shift0 = 2 + ((window >> 2) & 3);
        let shift1 = 3 + shift0 + (window & 3);
        Self {
            s0: (state << 3) as u16,
            s1: (state << 7) as u16,
            shift0,
            shift1,
        }
    }
}

#[derive(Clone)]
pub struct Contexts(pub [Model; NUM_CTX]);

impl Contexts {
    /// Estimated cost of coding `bin` with context `ctx_id`, in 1/32768 bits.
    pub fn cost(&self, ctx_id: usize, bin: u32) -> u32 {
        self.0[ctx_id].cost(bin)
    }

    /// `init_type` indexes vvdec's tables: 0 = B, 1 = P, 2 = I.
    pub fn new(init_type: usize, qp: i32) -> Self {
        let qp = qp.clamp(0, 63);
        let mut models = [Model::default(); NUM_CTX];
        for (i, m) in models.iter_mut().enumerate() {
            *m = Model::init(INIT[init_type][i], INIT[3][i], qp);
        }
        Self(models)
    }
}

pub struct Cabac<'a> {
    data: &'a [u8],
    pos: usize,
    range: u32,
    value: u32,
    bits_needed: i32,
    /// Set when the decoder consumed bytes past the end of its data.
    pub overrun: bool,
    pub ctx: Contexts,
}

impl<'a> Cabac<'a> {
    pub fn new(data: &'a [u8], ctx: Contexts) -> Self {
        let mut c = Self {
            data,
            pos: 0,
            range: 510,
            value: 0,
            bits_needed: -8,
            overrun: false,
            ctx,
        };
        c.start();
        c
    }

    fn read_byte(&mut self) -> u32 {
        match self.data.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                u32::from(b)
            }
            None => {
                self.pos += 1;
                self.overrun = true;
                0
            }
        }
    }

    fn start(&mut self) {
        self.range = 510;
        let hi = self.read_byte();
        let lo = self.read_byte();
        self.value = (hi << 8) + lo;
        self.bits_needed = -8;
    }

    /// Re-initializes the arithmetic decoder at a byte position (entry points).
    pub fn restart(&mut self, data: &'a [u8]) {
        self.data = data;
        self.pos = 0;
        self.overrun = false;
        self.start();
    }

    /// Re-initializes the arithmetic decoder at the current byte position.
    pub fn restart_here(&mut self) {
        self.start();
    }

    pub fn decode_bin(&mut self, ctx_id: usize) -> u32 {
        let m = &self.ctx.0[ctx_id];
        // pState = s1 + 16 * s0 (15 bits); valMps = pState >> 14.
        let p_state = u32::from(m.s1) + 16 * u32::from(m.s0);
        let mps = p_state >> 14;
        let lps = (((self.range >> 5) * ((if mps != 0 { 32767 - p_state } else { p_state }) >> 9))
            >> 1)
            + 4;
        self.range -= lps;
        let scaled = self.range << 7;
        let bin;
        if self.value < scaled {
            bin = mps;
            if self.range < 256 {
                self.range <<= 1;
                self.value <<= 1;
                self.bits_needed += 1;
                if self.bits_needed == 0 {
                    self.value += self.read_byte();
                    self.bits_needed = -8;
                }
            }
        } else {
            bin = 1 - mps;
            let num_bits = RENORM[(lps >> 3) as usize] as u32;
            self.value = (self.value - scaled) << num_bits;
            self.range = lps << num_bits;
            self.bits_needed += num_bits as i32;
            if self.bits_needed >= 0 {
                self.value += self.read_byte() << self.bits_needed;
                self.bits_needed -= 8;
            }
        }
        let m = &mut self.ctx.0[ctx_id];
        let s0 = i32::from(m.s0);
        let s1 = i32::from(m.s1);
        let s0 = s0 - (s0 >> m.shift0) + ((1023 * bin as i32) >> m.shift0);
        let s1 = s1 - (s1 >> m.shift1) + ((16383 * bin as i32) >> m.shift1);
        m.s0 = s0 as u16;
        m.s1 = s1 as u16;
        bin
    }

    pub fn decode_bypass(&mut self) -> u32 {
        self.value <<= 1;
        self.bits_needed += 1;
        if self.bits_needed >= 0 {
            self.value += self.read_byte();
            self.bits_needed = -8;
        }
        let scaled = self.range << 7;
        if self.value >= scaled {
            self.value -= scaled;
            1
        } else {
            0
        }
    }

    pub fn decode_bypass_bins(&mut self, n: u32) -> u32 {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.decode_bypass();
        }
        v
    }

    pub fn decode_terminate(&mut self) -> u32 {
        self.range -= 2;
        let scaled = self.range << 7;
        if self.value >= scaled {
            1
        } else {
            if self.range < 256 {
                self.range <<= 1;
                self.value <<= 1;
                self.bits_needed += 1;
                if self.bits_needed == 0 {
                    self.value += self.read_byte();
                    self.bits_needed = -8;
                }
            }
            0
        }
    }

    /// vvdec's `decodeRemAbsEP`.
    pub fn decode_rem_abs(&mut self, rice: u32, cutoff: u32, max_log2_range: u32) -> u32 {
        let max_prefix = 32 - max_log2_range;
        let mut prefix = 0u32;
        loop {
            prefix += 1;
            let bit = self.decode_bypass();
            if !(bit != 0 && prefix < max_prefix) {
                prefix -= 1 - bit;
                break;
            }
        }
        let mut length = rice;
        let offset;
        if prefix < cutoff {
            offset = prefix << rice;
        } else {
            offset = ((1u32 << (prefix - cutoff)) + cutoff - 1) << rice;
            length += if prefix == 32 - max_log2_range {
                max_log2_range - rice
            } else {
                prefix - cutoff
            };
        }
        offset.wrapping_add(self.decode_bypass_bins(length))
    }

    /// vvdec's `BinDecoder::finish` check after `end_of_slice_segment_flag`
    /// or `end_of_subset_one_bit` equal to 1.
    pub fn finish_ok(&self) -> bool {
        if self.pos == 0 || self.pos > self.data.len() {
            return false;
        }
        let last = u32::from(self.data[self.pos - 1]);
        ((last << (8 + self.bits_needed)) & 0xff) == 0x80
    }
}

static RENORM: [u8; 32] = [
    6, 5, 4, 4, 3, 3, 3, 3, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

impl Model {
    /// The most probable symbol and the LPS range for `range`, as in
    /// `Cabac::decode_bin`.
    #[inline]
    fn lps(&self, range: u32) -> (u32, u32) {
        let p_state = u32::from(self.s1) + 16 * u32::from(self.s0);
        let mps = p_state >> 14;
        let q = if mps != 0 { 32767 - p_state } else { p_state };
        (mps, (((range >> 5) * (q >> 9)) >> 1) + 4)
    }

    #[inline]
    fn update(&mut self, bin: u32) {
        let s0 = i32::from(self.s0);
        let s1 = i32::from(self.s1);
        let s0 = s0 - (s0 >> self.shift0) + ((1023 * bin as i32) >> self.shift0);
        let s1 = s1 - (s1 >> self.shift1) + ((16383 * bin as i32) >> self.shift1);
        self.s0 = s0 as u16;
        self.s1 = s1 as u16;
    }

    /// Estimated cost of coding `bin`, in 1/32768 bits.
    #[inline]
    fn cost(&self, bin: u32) -> u32 {
        let p_state = u32::from(self.s1) + 16 * u32::from(self.s0);
        // Probability of a one in 1/32768.
        let p1 = p_state.clamp(1, 32767);
        let p = if bin != 0 { p1 } else { 32768 - p1 };
        COST[(p >> 6) as usize]
    }
}

/// -log2(p / 512) in 1/32768 bits for p = 0..512 (p = 0 treated as 1/2).
static COST: std::sync::LazyLock<[u32; 513]> = std::sync::LazyLock::new(|| {
    let mut t = [0u32; 513];
    for (p, v) in t.iter_mut().enumerate() {
        let q = (p as f64).max(0.5) / 512.0;
        *v = (-q.log2() * 32768.0).round() as u32;
    }
    t
});

/// A consumer of context-coded, bypass and terminating bins: the arithmetic
/// encoder or a rate estimator.
pub trait BinSink {
    fn bin(&mut self, ctx_id: usize, bin: u32);
    fn ep(&mut self, bin: u32);
    /// `n` bypass bins, most significant first.
    fn eps(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            self.ep((value >> i) & 1);
        }
    }
    fn term(&mut self, bin: u32);

    /// Inverse of `Cabac::decode_rem_abs` (vvdec's `decodeRemAbsEP`).
    fn rem_abs(&mut self, value: u32, rice: u32, cutoff: u32, max_log2_range: u32) {
        let max_prefix = 32 - max_log2_range;
        if (value >> rice) < cutoff {
            let prefix = value >> rice;
            self.eps((1 << (prefix + 1)) - 2, prefix + 1);
            self.eps(value & ((1 << rice) - 1), rice);
            return;
        }
        let mut prefix = cutoff;
        while prefix < max_prefix
            && value >= (((1u32 << (prefix + 1 - cutoff)) + cutoff - 1) << rice)
        {
            prefix += 1;
        }
        let offset = ((1u32 << (prefix - cutoff)) + cutoff - 1) << rice;
        let length = if prefix == max_prefix {
            max_log2_range
        } else {
            rice + prefix - cutoff
        };
        for _ in 0..prefix {
            self.ep(1);
        }
        if prefix < max_prefix {
            self.ep(0);
        }
        self.eps(value - offset, length);
    }
}

/// Number of bypass bins `BinSink::rem_abs` writes for a value.
pub fn rem_abs_bins(value: u32, rice: u32, cutoff: u32, max_log2_range: u32) -> u32 {
    let max_prefix = 32 - max_log2_range;
    if (value >> rice) < cutoff {
        return (value >> rice) + 1 + rice;
    }
    let mut prefix = cutoff;
    while prefix < max_prefix && value >= (((1u32 << (prefix + 1 - cutoff)) + cutoff - 1) << rice) {
        prefix += 1;
    }
    let length = if prefix == max_prefix {
        max_log2_range
    } else {
        rice + prefix - cutoff
    };
    prefix + u32::from(prefix < max_prefix) + length
}

/// Rate estimation over a copy of the context models.
#[derive(Clone)]
pub struct Estimator {
    pub ctx: Contexts,
    /// Accumulated cost in 1/32768 bits.
    pub bits: u64,
}

impl Estimator {
    pub fn new(ctx: Contexts) -> Self {
        Self { ctx, bits: 0 }
    }
}

impl BinSink for Estimator {
    #[inline]
    fn bin(&mut self, ctx_id: usize, bin: u32) {
        let m = &mut self.ctx.0[ctx_id];
        self.bits += u64::from(m.cost(bin));
        m.update(bin);
    }
    #[inline]
    fn ep(&mut self, _bin: u32) {
        self.bits += 32768;
    }
    #[inline]
    fn eps(&mut self, _value: u32, n: u32) {
        self.bits += 32768 * u64::from(n);
    }
    fn term(&mut self, _bin: u32) {}
}

/// The arithmetic encoder matching `Cabac` (VTM's `BinEncoder`).
pub struct CabacWriter {
    pub ctx: Contexts,
    low: u32,
    range: u32,
    bits_left: i32,
    buffered_byte: u32,
    num_buffered: u32,
    out: Vec<u8>,
    /// Pending bits of the output, MSB first.
    held: u64,
    held_bits: u32,
}

impl CabacWriter {
    pub fn new(ctx: Contexts) -> Self {
        Self {
            ctx,
            low: 0,
            range: 510,
            bits_left: 23,
            buffered_byte: 0xff,
            num_buffered: 0,
            out: Vec::new(),
            held: 0,
            held_bits: 0,
        }
    }

    fn put(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            self.held = (self.held << 1) | u64::from((value >> i) & 1);
            self.held_bits += 1;
            if self.held_bits == 8 {
                self.out.push(self.held as u8);
                self.held = 0;
                self.held_bits = 0;
            }
        }
    }

    fn test_and_write_out(&mut self) {
        if self.bits_left < 12 {
            self.write_out();
        }
    }

    fn write_out(&mut self) {
        let lead = self.low >> (24 - self.bits_left);
        self.bits_left += 8;
        self.low &= 0xffff_ffffu32 >> self.bits_left;
        if lead == 0xff {
            self.num_buffered += 1;
        } else if self.num_buffered > 0 {
            let carry = lead >> 8;
            let byte = self.buffered_byte + carry;
            self.buffered_byte = lead & 0xff;
            self.put(byte, 8);
            let byte = (0xff + carry) & 0xff;
            while self.num_buffered > 1 {
                self.put(byte, 8);
                self.num_buffered -= 1;
            }
        } else {
            self.num_buffered = 1;
            self.buffered_byte = lead;
        }
    }

    /// Flushes the arithmetic coder after a terminating bin equal to 1 and
    /// appends the stop bit and alignment (rbsp_slice_segment_trailing_bits).
    pub fn finish(mut self) -> Vec<u8> {
        if (self.low >> (32 - self.bits_left)) != 0 {
            self.put(self.buffered_byte + 1, 8);
            while self.num_buffered > 1 {
                self.put(0, 8);
                self.num_buffered -= 1;
            }
            self.low -= 1 << (32 - self.bits_left);
        } else {
            if self.num_buffered > 0 {
                self.put(self.buffered_byte, 8);
            }
            while self.num_buffered > 1 {
                self.put(0xff, 8);
                self.num_buffered -= 1;
            }
        }
        let n = (24 - self.bits_left) as u32;
        self.put(self.low >> 8, n);
        self.put(1, 1);
        while self.held_bits != 0 {
            self.put(0, 1);
        }
        self.out
    }
}

impl BinSink for CabacWriter {
    fn bin(&mut self, ctx_id: usize, bin: u32) {
        let m = self.ctx.0[ctx_id];
        let (mps, lps) = m.lps(self.range);
        self.range -= lps;
        if bin != mps {
            let num_bits = u32::from(RENORM[(lps >> 3) as usize]);
            self.low = (self.low + self.range) << num_bits;
            self.range = lps << num_bits;
            self.bits_left -= num_bits as i32;
            self.test_and_write_out();
        } else if self.range < 256 {
            self.low <<= 1;
            self.range <<= 1;
            self.bits_left -= 1;
            self.test_and_write_out();
        }
        self.ctx.0[ctx_id].update(bin);
    }

    fn ep(&mut self, bin: u32) {
        self.low <<= 1;
        if bin != 0 {
            self.low += self.range;
        }
        self.bits_left -= 1;
        self.test_and_write_out();
    }

    fn term(&mut self, bin: u32) {
        self.range -= 2;
        if bin != 0 {
            self.low += self.range;
            self.low <<= 7;
            self.range = 2 << 7;
            self.bits_left -= 7;
        } else if self.range >= 256 {
            return;
        } else {
            self.low <<= 1;
            self.range <<= 1;
            self.bits_left -= 1;
        }
        self.test_and_write_out();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_round_trips() {
        let mut state = 0x1234_5678u32;
        let mut rnd = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let mut ops = Vec::new();
        for _ in 0..20000 {
            let r = rnd();
            ops.push(match r % 4 {
                0 | 1 => (0u8, (r >> 8) as usize % 40, u32::from((r >> 20) % 7 == 0)),
                2 => (1, 0, (r >> 9) & 1),
                _ => (2, ((r >> 8) % 6) as usize, (r >> 12) % 300),
            });
        }
        let mut w = CabacWriter::new(Contexts::new(2, 32));
        for &(k, c, v) in &ops {
            match k {
                0 => w.bin(c, v),
                1 => w.ep(v),
                _ => w.rem_abs(v, c as u32 % 4, 5, 15),
            }
        }
        w.term(1);
        let data = w.finish();
        let mut r = Cabac::new(&data, Contexts::new(2, 32));
        for &(k, c, v) in &ops {
            let got = match k {
                0 => r.decode_bin(c),
                1 => r.decode_bypass(),
                _ => r.decode_rem_abs(c as u32 % 4, 5, 15),
            };
            assert_eq!(got, v);
        }
        assert_eq!(r.decode_terminate(), 1);
        assert!(r.finish_ok());
    }
}
