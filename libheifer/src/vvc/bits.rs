// SPDX-License-Identifier: LGPL-3.0-or-later
//! NAL unit unescaping and the MSB-first bit reader used for VVC high-level
//! syntax, following vvdec's `InputBitstream`.
use super::Error;

/// Removes emulation prevention bytes (`00 00 03`) from a NAL unit payload,
/// as vvdec's `convertPayloadToRBSP` does, and drops trailing zero bytes
/// (`cabac_zero_words`). Returns the RBSP and the positions of removed bytes.
pub fn unescape(data: &[u8]) -> (Vec<u8>, Vec<usize>) {
    let mut out = Vec::with_capacity(data.len());
    let mut removed = Vec::new();
    let mut zeros = 0;
    for (i, &byte) in data.iter().enumerate() {
        if zeros == 2 && byte == 3 {
            removed.push(i);
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    (out, removed)
}

#[derive(Clone)]
pub struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            end: data.len() * 8,
        }
    }

    pub fn bit_pos(&self) -> usize {
        self.pos
    }

    pub fn bits_left(&self) -> usize {
        self.end - self.pos
    }

    pub fn is_byte_aligned(&self) -> bool {
        self.pos.is_multiple_of(8)
    }

    pub fn byte_pos(&self) -> usize {
        self.pos / 8
    }

    pub fn read(&mut self, n: u32) -> Result<u32, Error> {
        if n == 0 {
            return Ok(0);
        }
        if self.pos + n as usize > self.end {
            return Err(Error::Invalid("Exceeded FIFO size"));
        }
        let mut value = 0u64;
        for _ in 0..n {
            let byte = self.data[self.pos / 8];
            value = (value << 1) | u64::from((byte >> (7 - self.pos % 8)) & 1);
            self.pos += 1;
        }
        Ok(value as u32)
    }

    pub fn flag(&mut self) -> Result<bool, Error> {
        Ok(self.read(1)? != 0)
    }

    pub fn peek(&self, n: u32) -> Result<u32, Error> {
        self.clone().read(n)
    }

    /// `ue(v)`, as vvdec's `xReadUvlc`: up to 32 leading zeros.
    pub fn uvlc(&mut self) -> Result<u32, Error> {
        let mut zeros = 0u32;
        while !self.flag()? {
            zeros += 1;
            if zeros > 32 {
                return Err(Error::Invalid("uvlc prefix too long"));
            }
        }
        if zeros == 0 {
            return Ok(0);
        }
        let suffix = if zeros == 32 {
            u64::from(self.read(32)?)
        } else {
            u64::from(self.read(zeros)?)
        };
        let value = ((1u64 << zeros) - 1) + suffix;
        u32::try_from(value).map_err(|_| Error::Invalid("uvlc overflow"))
    }

    pub fn svlc(&mut self) -> Result<i32, Error> {
        let code = self.uvlc()?;
        Ok(if code & 1 != 0 {
            ((code >> 1) + 1) as i32
        } else {
            -((code >> 1) as i32)
        })
    }

    pub fn uvlc_range(&mut self, min: u32, max: u32, what: &'static str) -> Result<u32, Error> {
        let value = self.uvlc()?;
        if value < min || value > max {
            return Err(Error::Invalid(what));
        }
        Ok(value)
    }

    pub fn svlc_range(&mut self, min: i32, max: i32, what: &'static str) -> Result<i32, Error> {
        let value = self.svlc()?;
        if value < min || value > max {
            return Err(Error::Invalid(what));
        }
        Ok(value)
    }

    pub fn code_range(
        &mut self,
        n: u32,
        min: u32,
        max: u32,
        what: &'static str,
    ) -> Result<u32, Error> {
        let value = self.read(n)?;
        if value < min || value > max {
            return Err(Error::Invalid(what));
        }
        Ok(value)
    }

    pub fn skip_to_byte(&mut self) -> Result<(), Error> {
        while !self.is_byte_aligned() {
            self.read(1)?;
        }
        Ok(())
    }

    /// `rbsp_trailing_bits()`.
    pub fn trailing_bits(&mut self) -> Result<(), Error> {
        if !self.flag()? {
            return Err(Error::Invalid("rbsp_stop_one_bit"));
        }
        while !self.is_byte_aligned() {
            if self.flag()? {
                return Err(Error::Invalid("rbsp_alignment_zero_bit"));
            }
        }
        Ok(())
    }

    /// vvdec's `xMoreRbspData`.
    pub fn more_rbsp_data(&self) -> Result<bool, Error> {
        let left = self.bits_left();
        if left > 8 {
            return Ok(true);
        }
        let mut last = self.peek(left as u32)?;
        let mut count = left as i32;
        while count > 0 && last & 1 == 0 {
            last >>= 1;
            count -= 1;
        }
        count -= 1;
        if count < 0 {
            return Err(Error::Invalid("Negative number of bits"));
        }
        Ok(count > 0)
    }

    /// Reads `byte_alignment()` after a slice header: a one bit then zeros.
    pub fn byte_alignment(&mut self) -> Result<(), Error> {
        if !self.flag()? {
            return Err(Error::Invalid("alignment_bit_equal_to_one"));
        }
        while !self.is_byte_aligned() {
            if self.flag()? {
                return Err(Error::Invalid("alignment_bit_equal_to_zero"));
            }
        }
        Ok(())
    }
}

/// `Ceil( Log2( n ) )` for n >= 1.
pub fn ceil_log2(n: u32) -> u32 {
    if n <= 1 {
        0
    } else {
        32 - (n - 1).leading_zeros()
    }
}

/// MSB-first bit writer for high-level syntax (the encoder's counterpart of
/// [`BitReader`]).
#[derive(Default)]
pub struct BitWriter {
    pub data: Vec<u8>,
    cur: u32,
    n: u32,
}

impl BitWriter {
    pub fn write(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            self.cur = (self.cur << 1) | ((value >> i) & 1);
            self.n += 1;
            if self.n == 8 {
                self.data.push(self.cur as u8);
                self.cur = 0;
                self.n = 0;
            }
        }
    }
    pub fn flag(&mut self, v: bool) {
        self.write(u32::from(v), 1);
    }
    pub fn uvlc(&mut self, v: u32) {
        let v1 = u64::from(v) + 1;
        let len = 64 - v1.leading_zeros();
        self.write(0, len - 1);
        for i in (0..len).rev() {
            self.write(((v1 >> i) & 1) as u32, 1);
        }
    }
    pub fn svlc(&mut self, v: i32) {
        self.uvlc(if v > 0 {
            2 * v as u32 - 1
        } else {
            2 * v.unsigned_abs()
        });
    }
    pub fn align_zero(&mut self) {
        while self.n != 0 {
            self.write(0, 1);
        }
    }
    /// rbsp_trailing_bits / byte_alignment: a one bit then zero bits.
    pub fn trailing_bits(&mut self) {
        self.write(1, 1);
        self.align_zero();
    }
}

/// Inserts emulation prevention bytes into an RBSP.
pub fn escape(rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len() + rbsp.len() / 64);
    let mut zeros = 0;
    for &b in rbsp {
        if zeros >= 2 && b <= 3 {
            out.push(3);
            zeros = 0;
        }
        out.push(b);
        zeros = if b == 0 { zeros + 1 } else { 0 };
    }
    out
}
