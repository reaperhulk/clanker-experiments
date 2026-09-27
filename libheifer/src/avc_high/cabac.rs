// SPDX-License-Identifier: LGPL-3.0-or-later
//! H.264 CABAC (ITU-T H.264 9.3.4): the arithmetic encoder, and a rate
//! estimator that updates the same context states without producing bits.

use rusty_h264_common::cabac_tables::{RANGE_LPS, STATE_TRANS};

/// Context states for ctxIdx 0-1023: `pStateIdx << 1 | valMPS`.
#[derive(Clone)]
pub(super) struct Contexts(pub [u8; 1024]);

impl Contexts {
    /// 9.3.1.1 for an I slice at `slice_qp` (SliceQPY, clipped to 0..=51).
    pub fn new(slice_qp: i32) -> Self {
        let qp = slice_qp.clamp(0, 51);
        let mut states = [0u8; 1024];
        for (state, &(m, n)) in states.iter_mut().zip(super::init::INIT_I.iter()) {
            let pre = (((i32::from(m) * qp) >> 4) + i32::from(n)).clamp(1, 126);
            *state = if pre <= 63 {
                ((63 - pre) << 1) as u8
            } else {
                (((pre - 64) << 1) | 1) as u8
            };
        }
        Contexts(states)
    }

    #[inline]
    fn update(&mut self, ctx: usize, bin: bool) -> (u8, bool) {
        let s = self.0[ctx];
        let (state, mps) = (s >> 1, s & 1 != 0);
        let next = if bin == mps {
            (STATE_TRANS[state as usize][1] << 1) | u8::from(mps)
        } else {
            let mps = if state == 0 { !mps } else { mps };
            (STATE_TRANS[state as usize][0] << 1) | u8::from(mps)
        };
        self.0[ctx] = next;
        (state, mps)
    }
}

/// Everything the syntax writer needs: context-coded, bypass and terminate
/// bins. Implemented by the arithmetic encoder and the rate estimator.
pub(super) trait Sink {
    fn decision(&mut self, ctx: usize, bin: bool);
    fn bypass(&mut self, bin: bool);
    fn terminate(&mut self, bin: bool);
}

/// 9.3.4.2-9.3.4.5.
pub(super) struct Encoder {
    pub contexts: Contexts,
    low: u32,
    range: u32,
    outstanding: u32,
    first: bool,
    bytes: Vec<u8>,
    bits: u32,
}

impl Encoder {
    pub fn new(contexts: Contexts, header: Vec<u8>, header_bits: u32) -> Self {
        Encoder {
            contexts,
            low: 0,
            range: 510,
            outstanding: 0,
            first: true,
            bytes: header,
            bits: header_bits,
        }
    }

    fn write(&mut self, bit: bool) {
        if self.bits.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if bit {
            *self.bytes.last_mut().unwrap() |= 0x80 >> (self.bits % 8);
        }
        self.bits += 1;
    }

    fn put(&mut self, bit: bool) {
        if self.first {
            self.first = false;
        } else {
            self.write(bit);
        }
        while self.outstanding > 0 {
            self.write(!bit);
            self.outstanding -= 1;
        }
    }

    fn renormalize(&mut self) {
        while self.range < 256 {
            if self.low < 256 {
                self.put(false);
            } else if self.low >= 512 {
                self.low -= 512;
                self.put(true);
            } else {
                self.low -= 256;
                self.outstanding += 1;
            }
            self.range <<= 1;
            self.low <<= 1;
        }
    }

    /// The slice data after `end_of_slice_flag` = 1: the flush's last bit
    /// is the rbsp_stop_one_bit, followed by alignment zero bits.
    pub fn finish(mut self) -> Vec<u8> {
        self.range = 2;
        self.renormalize();
        self.put((self.low >> 9) & 1 != 0);
        let tail = ((self.low >> 7) & 3) | 1;
        self.write(tail & 2 != 0);
        self.write(tail & 1 != 0);
        self.bytes
    }
}

impl Sink for Encoder {
    fn decision(&mut self, ctx: usize, bin: bool) {
        let s = self.contexts.0[ctx] >> 1;
        let lps = u32::from(RANGE_LPS[s as usize][((self.range >> 6) & 3) as usize]);
        self.range -= lps;
        let (_, mps) = self.contexts.update(ctx, bin);
        if bin != mps {
            self.low += self.range;
            self.range = lps;
        }
        self.renormalize();
    }

    fn bypass(&mut self, bin: bool) {
        self.low <<= 1;
        if bin {
            self.low += self.range;
        }
        if self.low >= 1024 {
            self.put(true);
            self.low -= 1024;
        } else if self.low < 512 {
            self.put(false);
        } else {
            self.low -= 512;
            self.outstanding += 1;
        }
    }

    fn terminate(&mut self, bin: bool) {
        self.range -= 2;
        if bin {
            self.low += self.range;
            // Only end_of_slice_flag is coded as a terminating 1 here;
            // `finish` flushes.
        } else {
            self.renormalize();
        }
    }
}

/// Fractional bits (1/256) to code a bin in state `p` (the MPS or the LPS).
fn cost_table() -> &'static [[u32; 2]; 64] {
    static TABLE: std::sync::OnceLock<[[u32; 2]; 64]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let alpha = (0.01875f64 / 0.5).powf(1.0 / 63.0);
        let mut t = [[0u32; 2]; 64];
        for (s, row) in t.iter_mut().enumerate() {
            let lps = 0.5 * alpha.powi(s as i32);
            row[0] = (-(1.0 - lps).log2() * 256.0).round() as u32;
            row[1] = (-lps.log2() * 256.0).round() as u32;
        }
        t
    })
}

/// Rate estimator: context states evolve as in the encoder; `bits` counts
/// in 1/256 bit.
#[derive(Clone)]
pub(super) struct Counter {
    pub contexts: Contexts,
    pub bits: u32,
}

impl Sink for Counter {
    fn decision(&mut self, ctx: usize, bin: bool) {
        let (state, mps) = self.contexts.update(ctx, bin);
        self.bits += cost_table()[state as usize][usize::from(bin != mps)];
    }
    fn bypass(&mut self, _: bool) {
        self.bits += 256;
    }
    fn terminate(&mut self, bin: bool) {
        // P(terminate) = 2/range: a zero costs almost nothing.
        self.bits += if bin { 7 * 256 } else { 1 };
    }
}
