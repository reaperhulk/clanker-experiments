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
