// SPDX-License-Identifier: LGPL-3.0-or-later
// Configuration semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Registered VVC encoder parameter sets and profile-tier-level serialization.
use crate::context::ContextError;

struct Bits {
    data: Vec<u8>,
    position: usize,
}
impl Bits {
    fn new(data: &[u8]) -> Self {
        let mut out = Vec::with_capacity(data.len());
        let mut at = 0;
        while at < data.len() {
            if data.get(at..at + 3) == Some(&[0, 0, 3]) {
                out.extend_from_slice(&[0, 0]);
                at += 3;
            } else {
                out.push(data[at]);
                at += 1;
            }
        }
        Self {
            data: out,
            position: 0,
        }
    }
    fn get(&mut self, n: usize) -> u32 {
        let mut value = 0;
        for _ in 0..n {
            value = (value << 1)
                | self
                    .data
                    .get(self.position / 8)
                    .map_or(0, |b| u32::from((b >> (7 - self.position % 8)) & 1));
            self.position += 1;
        }
        value
    }
    fn align(&mut self) {
        self.position = self.position.next_multiple_of(8);
    }
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.get(1) == 0 {
            zeros += 1;
            if zeros > 20 {
                return None;
            }
        }
        Some((1u32 << zeros) - 1 + self.get(zeros))
    }
}
#[derive(Default)]
struct Configuration {
    layers: u8,
    chroma: u8,
    depth: u8,
    rate: u8,
    profile: u8,
    tier: u8,
    level: u8,
    frame_only: u8,
    multi_layer: u8,
    constraints: Vec<u8>,
    level_flags: Vec<bool>,
    levels: Vec<u8>,
    subprofiles: Vec<u32>,
    width: u16,
    height: u16,
}
impl Configuration {
    // The native encoder retains the fields preceding an SPS parse error and
    // replaces the previous record, even when parsing did not finish.
    fn parse(&mut self, nal: &[u8]) -> Option<()> {
        let mut b = Bits::new(nal);
        b.get(24);
        self.layers = b.get(3) as u8 + 1;
        self.chroma = b.get(2) as u8;
        b.get(2);
        if b.get(1) != 0 {
            self.profile = b.get(7) as u8;
            self.tier = b.get(1) as u8;
            self.level = b.get(8) as u8;
            self.frame_only = b.get(1) as u8;
            self.multi_layer = b.get(1) as u8;
            if b.get(1) != 0 {
                return None;
            }
            self.constraints.push(0);
            b.align();
            self.level_flags.resize(usize::from(self.layers), false);
            self.levels.resize(usize::from(self.layers), 0);
            for i in (0..usize::from(self.layers - 1)).rev() {
                self.level_flags[i] = b.get(1) != 0;
            }
            b.align();
            for i in (0..usize::from(self.layers - 1)).rev() {
                if self.level_flags[i] {
                    self.levels[i] = b.get(8) as u8;
                }
            }
            for _ in 0..b.get(8) {
                self.subprofiles.push(b.get(32));
            }
        }
        b.get(1);
        if b.get(1) != 0 {
            b.get(1);
        }
        let width = b.ue()?;
        let height = b.ue()?;
        if width > 65535 || height > 65535 {
            return None;
        }
        self.width = width as u16;
        self.height = height as u16;
        if b.get(1) != 0 {
            let left = u64::from(b.ue()?);
            let right = u64::from(b.ue()?);
            let top = u64::from(b.ue()?);
            let bottom = u64::from(b.ue()?);
            let sx = if matches!(self.chroma, 1 | 2) { 2 } else { 1 };
            let sy = if self.chroma == 1 { 2 } else { 1 };
            if sx * (left + right) > u64::from(width) || sy * (top + bottom) > u64::from(height) {
                return None;
            }
        }
        if b.get(1) != 0 {
            return None;
        }
        let depth = b.ue()?;
        if depth > 247 {
            return None;
        }
        self.depth = depth as u8;
        self.rate = 1;
        Some(())
    }
}
#[derive(Default)]
pub struct EncoderConfiguration {
    config: Configuration,
    arrays: Vec<(u8, Vec<Vec<u8>>)>,
}
impl EncoderConfiguration {
    pub fn update(&mut self, nal: &[u8]) -> bool {
        let kind = nal.get(1).map_or(0, |v| (v >> 3) & 31);
        if kind == 15 {
            self.config = Configuration::default();
            self.config.parse(nal);
        }
        if !matches!(kind, 14..=16) {
            return false;
        }
        if let Some((_, units)) = self.arrays.iter_mut().find(|(k, _)| *k == kind) {
            units.push(nal.to_vec());
        } else {
            self.arrays.push((kind, vec![nal.to_vec()]));
        }
        true
    }
    pub fn property(&self) -> crate::properties::Property {
        let mut data = Vec::new();
        let error = self.write_into(&mut data).err();
        let mut p = crate::encoding::property(*b"vvcC", data);
        p.write_error = error;
        p
    }
    fn write_into(&self, out: &mut Vec<u8>) -> Result<(), ContextError> {
        let c = &self.config;
        out.extend_from_slice(&[0, 0, 0, 0, 255]);
        let word = (u16::from(c.layers) << 4) | (u16::from(c.rate) << 2) | u16::from(c.chroma);
        out.extend_from_slice(&word.to_be_bytes());
        out.extend_from_slice(&[
            (c.depth << 5) | 31,
            c.constraints.len() as u8 & 63,
            (c.profile << 1) | c.tier,
            c.level,
        ]);
        for (i, byte) in c.constraints.iter().enumerate() {
            out.push(if i == 0 {
                (c.frame_only << 7) | (c.multi_layer << 6) | byte
            } else {
                *byte
            });
        }
        if c.layers > 1 {
            let mut flags = 0;
            for (shift, i) in (0..usize::from(c.layers - 1)).rev().enumerate() {
                if c.level_flags.get(i) == Some(&true) {
                    flags |= 128 >> shift;
                }
            }
            out.push(flags);
        }
        for i in (0..usize::from(c.layers.saturating_sub(1))).rev() {
            if c.level_flags.get(i) == Some(&true) {
                out.push(c.levels[i]);
            }
        }
        out.push(c.subprofiles.len() as u8);
        for value in &c.subprofiles {
            out.extend_from_slice(&value.to_be_bytes());
        }
        out.extend_from_slice(&c.width.to_be_bytes());
        out.extend_from_slice(&c.height.to_be_bytes());
        out.extend_from_slice(&[0, 0]);
        out.push(self.arrays.len() as u8);
        for (kind, units) in &self.arrays {
            out.push(128 | kind);
            let count =
                u16::try_from(units.len()).map_err(|_| write_error("Too many VVC NAL units."))?;
            out.extend_from_slice(&count.to_be_bytes());
            for nal in units {
                let length =
                    u16::try_from(nal.len()).map_err(|_| write_error("VVC NAL too large."))?;
                out.extend_from_slice(&length.to_be_bytes());
                out.extend_from_slice(nal);
            }
        }
        Ok(())
    }
}
fn write_error(message: &str) -> ContextError {
    ContextError::new(9, 0, format!("Encoding error: Unspecified: {message}"))
}
