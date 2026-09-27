// SPDX-License-Identifier: LGPL-3.0-or-later
// Configuration-prefix semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Inspect HEVC coded geometry before entering the codec or reading media data.
use crate::context::ContextError;

struct Bits {
    data: Vec<u8>,
    position: usize,
}
impl Bits {
    fn new(data: &[u8]) -> Self {
        let mut bytes = Vec::with_capacity(data.len());
        let mut i = 0;
        while i < data.len() {
            if data.get(i..i + 3) == Some(&[0, 0, 3]) {
                bytes.extend_from_slice(&[0, 0]);
                i += 3;
            } else {
                bytes.push(data[i]);
                i += 1;
            }
        }
        Self {
            data: bytes,
            position: 0,
        }
    }
    fn get(&mut self, n: usize) -> u32 {
        let mut value = 0;
        for _ in 0..n {
            value = (value << 1)
                | u32::from(
                    self.data
                        .get(self.position / 8)
                        .map_or(0, |byte| (byte >> (7 - self.position % 8)) & 1),
                );
            self.position += 1;
        }
        value
    }
    fn skip(&mut self, n: usize) {
        self.position += n;
    }
    fn ue(&mut self) -> Result<u32, ContextError> {
        let mut zeros = 0;
        while self.get(1) == 0 {
            zeros += 1;
            if zeros > 20 {
                return Err(invalid("Invalid variable length code in HEVC SPS header"));
            }
        }
        Ok((1u32 << zeros) - 1 + self.get(zeros))
    }
}
fn invalid(message: &str) -> ContextError {
    ContextError::invalid(2006, &format!("Invalid parameter value: {message}"))
}
fn sps_geometry(data: &[u8]) -> Result<(u32, u32), ContextError> {
    let mut bits = Bits::new(data);
    bits.skip(20);
    let layers = bits.get(3) as usize;
    bits.skip(1 + 96);
    let mut flags = Vec::new();
    for _ in 0..layers {
        flags.push((bits.get(1), bits.get(1)));
    }
    if layers != 0 {
        bits.skip((8 - layers) * 2);
    }
    for (profile, level) in flags {
        // Preserve the pinned parser's sub-layer prefix lengths.
        if profile != 0 {
            bits.skip(8 + 32 + 16);
        }
        if level != 0 {
            bits.skip(8);
        }
    }
    bits.ue()?;
    let chroma = bits.ue()?;
    if chroma > 3 {
        return Err(invalid("SPS chroma_format_idc out of range"));
    }
    if chroma == 3 {
        bits.skip(1);
    }
    let width = bits.ue()?;
    let height = bits.ue()?;
    if bits.get(1) != 0 {
        let left = u64::from(bits.ue()?);
        let right = u64::from(bits.ue()?);
        let top = u64::from(bits.ue()?);
        let bottom = u64::from(bits.ue()?);
        let sx = if chroma == 1 || chroma == 2 { 2 } else { 1 };
        let sy = if chroma == 1 { 2 } else { 1 };
        if sx * (left + right) > u64::from(width) || sy * (top + bottom) > u64::from(height) {
            return Err(invalid("SPS conformance window exceeds image dimensions"));
        }
    }
    if bits.ue()? > 8 {
        return Err(invalid("SPS bit_depth_luma_minus8 out of range"));
    }
    if bits.ue()? > 8 {
        return Err(invalid("SPS bit_depth_chroma_minus8 out of range"));
    }
    Ok((width, height))
}
pub fn coded_size(config: &[u8]) -> Result<Option<(u32, u32)>, ContextError> {
    let truncated = || ContextError::invalid(100, "Unexpected end of file");
    let mut at = 23usize;
    for _ in 0..*config.get(22).ok_or_else(truncated)? {
        let header = config.get(at..at + 3).ok_or_else(truncated)?;
        at += 3;
        let count = u16::from_be_bytes([header[1], header[2]]);
        for _ in 0..count {
            let size = config.get(at..at + 2).ok_or_else(truncated)?;
            let n = usize::from(u16::from_be_bytes([size[0], size[1]]));
            at += 2;
            let nal = config.get(at..at + n).ok_or_else(truncated)?;
            at += n;
            if header[0] & 63 == 33 && !nal.is_empty() {
                return sps_geometry(nal).map(Some);
            }
        }
    }
    Ok(None)
}

/// HEVC packet configuration for registered still-image encoders.
#[derive(Default)]
pub struct EncoderConfiguration {
    header: [u8; 22],
    arrays: Vec<(u8, Vec<Vec<u8>>)>,
    pub size: (u32, u32),
}
impl EncoderConfiguration {
    pub fn update(&mut self, nal: &[u8]) -> Result<bool, ContextError> {
        let Some(&first) = nal.first() else {
            return Ok(false);
        };
        let kind = first >> 1;
        if kind == 33 {
            let _ = self.sps(nal);
        }
        if !matches!(kind, 32..=34) {
            return Ok(false);
        }
        if let Some((_, units)) = self.arrays.iter_mut().find(|(t, _)| *t == kind) {
            for existing in units.iter_mut() {
                let common = existing.len().min(nal.len());
                if existing[..common] == nal[..common] {
                    if nal.len() < existing.len() {
                        *existing = nal.to_vec();
                    }
                    return Ok(true);
                }
            }
            units.push(nal.to_vec());
        } else {
            self.arrays.push((kind, vec![nal.to_vec()]));
        }
        Ok(true)
    }
    fn sps(&mut self, nal: &[u8]) -> Result<(), ContextError> {
        let mut bits = Bits::new(nal);
        bits.skip(20);
        let layers = bits.get(3) as usize;
        let nested = bits.get(1) as u8;
        self.header[21] = (self.header[21] & !4) | (nested << 2);
        self.header[1] = bits.get(8) as u8;
        self.header[2..6].copy_from_slice(&bits.get(32).to_be_bytes());
        bits.skip(48);
        self.header[12] = bits.get(8) as u8;
        let flags: Vec<_> = (0..layers).map(|_| (bits.get(1), bits.get(1))).collect();
        if layers != 0 {
            bits.skip((8 - layers) * 2);
        }
        for (profile, level) in flags {
            if profile != 0 {
                bits.skip(56);
            }
            if level != 0 {
                bits.skip(8);
            }
        }
        bits.ue()?;
        let chroma = bits.ue()?;
        if chroma > 3 {
            return Err(invalid("SPS chroma_format_idc out of range"));
        }
        self.header[16] = 0xfc | chroma as u8;
        if chroma == 3 {
            bits.skip(1);
        }
        self.size.0 = bits.ue()?;
        self.size.1 = bits.ue()?;
        let (mut width, mut height) = self.size;
        if bits.get(1) != 0 {
            let left = u64::from(bits.ue()?);
            let right = u64::from(bits.ue()?);
            let top = u64::from(bits.ue()?);
            let bottom = u64::from(bits.ue()?);
            let sx = if matches!(chroma, 1 | 2) { 2 } else { 1 };
            let sy = if chroma == 1 { 2 } else { 1 };
            let crop_x = sx * (left + right);
            let crop_y = sy * (top + bottom);
            if crop_x > u64::from(width) || crop_y > u64::from(height) {
                return Err(invalid("SPS conformance window exceeds image dimensions"));
            }
            width -= crop_x as u32;
            height -= crop_y as u32;
        }
        self.size = (width, height);
        let luma = bits.ue()?;
        if luma > 8 {
            return Err(invalid("SPS bit_depth_luma_minus8 out of range"));
        }
        self.header[17] = 0xf8 | luma as u8;
        let color = bits.ue()?;
        if color > 8 {
            return Err(invalid("SPS bit_depth_chroma_minus8 out of range"));
        }
        self.header[0] = 1;
        self.header[13..22].copy_from_slice(&[
            0xf0,
            0,
            0xfc,
            0xfc | chroma as u8,
            0xf8 | luma as u8,
            0xf8 | color as u8,
            0,
            0,
            11 | (nested << 2),
        ]);
        self.size = (width, height);
        Ok(())
    }
    pub fn property(&self) -> crate::properties::Property {
        let mut bytes = Vec::new();
        let error = self.write_into(&mut bytes).err();
        let mut property = crate::encoding::property(*b"hvcC", bytes);
        property.write_error = error;
        property
    }
    fn write_into(&self, bytes: &mut Vec<u8>) -> Result<(), ContextError> {
        bytes.extend_from_slice(&self.header);
        bytes.push(self.arrays.len() as u8);
        for (kind, units) in &self.arrays {
            bytes.push(64 | kind);
            let count = u16::try_from(units.len())
                .map_err(|_| ContextError::invalid(0, "Too many NAL units in hvcC"))?;
            bytes.extend_from_slice(&count.to_be_bytes());
            for nal in units {
                let n = u16::try_from(nal.len()).map_err(|_| {
                    ContextError::invalid(0, "hvcC NAL unit exceeds maximum size (64kB)")
                })?;
                bytes.extend_from_slice(&n.to_be_bytes());
                bytes.extend_from_slice(nal);
            }
        }
        Ok(())
    }
}
