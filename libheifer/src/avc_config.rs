// SPDX-License-Identifier: LGPL-3.0-or-later
// Configuration semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Registered AVC encoder configuration and SPS display dimensions.
use crate::context::ContextError;

struct Bits {
    bytes: Vec<u8>,
    position: usize,
}
impl Bits {
    fn new(data: &[u8]) -> Self {
        let mut bytes = Vec::with_capacity(data.len());
        let mut at = 0;
        while at < data.len() {
            if data.get(at..at + 3) == Some(&[0, 0, 3]) {
                bytes.extend_from_slice(&[0, 0]);
                at += 3;
            } else {
                bytes.push(data[at]);
                at += 1;
            }
        }
        Self { bytes, position: 0 }
    }
    fn get(&mut self, n: usize) -> u32 {
        let mut value = 0;
        for _ in 0..n {
            value = (value << 1)
                | self
                    .bytes
                    .get(self.position / 8)
                    .map_or(0, |b| u32::from((b >> (7 - self.position % 8)) & 1));
            self.position += 1;
        }
        value
    }
    fn ue(&mut self) -> Result<u32, ContextError> {
        let mut zeros = 0;
        while self.get(1) == 0 {
            zeros += 1;
            if zeros > 20 {
                return Err(invalid("Invalid variable length code in AVC SPS header"));
            }
        }
        Ok((1u32 << zeros) - 1 + self.get(zeros))
    }
    fn se(&mut self) -> Result<i32, ContextError> {
        let n = self.ue()?;
        Ok(if n & 1 == 0 {
            -(n as i32) / 2
        } else {
            (n as i32 + 1) / 2
        })
    }
}
fn invalid(message: &str) -> ContextError {
    ContextError::invalid(0, message)
}
#[derive(Default)]
pub struct EncoderConfiguration {
    profile: u8,
    compatibility: u8,
    level: u8,
    chroma: u8,
    luma: u8,
    color: u8,
    sps: Vec<Vec<u8>>,
    pps: Vec<Vec<u8>>,
    extensions: Vec<Vec<u8>>,
    pub size: (u32, u32),
}
impl EncoderConfiguration {
    pub fn update(&mut self, nal: &[u8]) -> Result<bool, ContextError> {
        match nal.first().map(|n| n & 31) {
            Some(7) => {
                let _ = self.parse(nal);
                self.sps.push(nal.to_vec());
            }
            Some(8) => self.pps.push(nal.to_vec()),
            Some(13) => self.extensions.push(nal.to_vec()),
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn parse(&mut self, nal: &[u8]) -> Result<(), ContextError> {
        let mut b = Bits::new(nal);
        b.get(8);
        self.profile = b.get(8) as u8;
        self.compatibility = b.get(8) as u8;
        self.level = b.get(8) as u8;
        b.ue()?;
        if matches!(
            self.profile,
            100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
        ) {
            let chroma = b.ue()?;
            if chroma > 3 {
                return Err(invalid("Invalid chroma format in AVC SPS header"));
            }
            self.chroma = chroma as u8;
            if chroma == 3 {
                b.get(1);
            }
            self.luma = (8 + b.ue()?) as u8;
            self.color = (8 + b.ue()?) as u8;
            b.get(1);
            if b.get(1) != 0 {
                for i in 0..if chroma == 3 { 12 } else { 8 } {
                    if b.get(1) != 0 {
                        let mut last = 8;
                        for _ in 0..if i < 6 { 16 } else { 64 } {
                            let next = (last + b.se()? + 256) % 256;
                            if next == 0 {
                                break;
                            }
                            last = next;
                        }
                    }
                }
            }
        } else {
            self.chroma = 1;
            self.luma = 8;
            self.color = 8;
        }
        b.ue()?;
        match b.ue()? {
            0 => {
                b.ue()?;
            }
            1 => {
                b.get(1);
                b.se()?;
                b.se()?;
                let count = b.ue()?;
                for _ in 0..count {
                    b.ue()?;
                }
            }
            _ => {}
        }
        b.ue()?;
        b.get(1);
        let w = (u64::from(b.ue()?) + 1) * 16;
        let mut h = (u64::from(b.ue()?) + 1) * 16;
        let frame = b.get(1);
        if frame == 0 {
            b.get(1);
        }
        b.get(1);
        h *= u64::from(2 - frame);
        if w > u64::from(u32::MAX) || h > u64::from(u32::MAX) {
            return Err(ContextError::invalid(129, "AVC SPS image size too large"));
        }
        let (mut w, mut h) = (w as u32, h as u32);
        self.size = (w, h);
        if b.get(1) != 0 {
            let left = u64::from(b.ue()?);
            let right = u64::from(b.ue()?);
            let top = u64::from(b.ue()?);
            let bottom = u64::from(b.ue()?);
            let sx = if matches!(self.chroma, 1 | 2) { 2 } else { 1 };
            let sy = if self.chroma == 1 { 2 } else { 1 } * u64::from(2 - frame);
            let dx = (left + right) * sx;
            let dy = (top + bottom) * sy;
            if dx >= u64::from(w) || dy >= u64::from(h) {
                return Err(ContextError::invalid(
                    129,
                    "AVC SPS cropping exceeds image size",
                ));
            }
            w -= dx as u32;
            h -= dy as u32;
        }
        self.size = (w, h);
        Ok(())
    }
    pub fn property(&self) -> crate::properties::Property {
        let mut data = Vec::new();
        let error = self.write_into(&mut data).err();
        let mut property = crate::encoding::property(*b"avcC", data);
        property.write_error = error;
        property
    }
    fn write_into(&self, b: &mut Vec<u8>) -> Result<(), ContextError> {
        b.extend_from_slice(&[1, self.profile, self.compatibility, self.level, 255]);
        if self.sps.len() > 31 {
            return Err(write_error("Cannot write more than 31 PPS into avcC box."));
        }
        b.push(224 | self.sps.len() as u8);
        write_nals(b, &self.sps, "SPS")?;
        if self.pps.len() > 255 {
            return Err(write_error("Cannot write more than 255 PPS into avcC box."));
        }
        b.push(self.pps.len() as u8);
        write_nals(b, &self.pps, "PPS")?;
        if !matches!(self.profile, 66 | 77 | 88) {
            b.extend_from_slice(&[
                self.chroma,
                self.luma.wrapping_sub(8),
                self.color.wrapping_sub(8),
            ]);
            if self.extensions.len() > 255 {
                return Err(write_error(
                    "Cannot write more than 255 SPS-Ext into avcC box.",
                ));
            }
            b.push(self.extensions.len() as u8);
            write_nals(b, &self.extensions, "SPS-Ext")?;
        }
        Ok(())
    }
}
fn write_error(message: &str) -> ContextError {
    ContextError::new(9, 0, format!("Encoding error: Unspecified: {message}"))
}
fn write_nals(out: &mut Vec<u8>, nals: &[Vec<u8>], kind: &str) -> Result<(), ContextError> {
    for nal in nals {
        let n = u16::try_from(nal.len()).map_err(|_| {
            write_error(&format!(
                "Cannot write {kind} larger than 65535 bytes into avcC box."
            ))
        })?;
        out.extend_from_slice(&n.to_be_bytes());
        out.extend_from_slice(nal);
    }
    Ok(())
}
