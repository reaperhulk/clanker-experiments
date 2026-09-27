// SPDX-License-Identifier: LGPL-3.0-or-later
//! Auxiliary image descriptions and HEVC depth-representation SEI semantics.
use crate::context::ContextError;
use std::ffi::CString;

#[derive(Default)]
pub struct Auxiliary {
    pub kind: CString,
    pub is_alpha: bool,
    pub is_depth: bool,
    pub depth_image: Option<u32>,
    pub images: Vec<u32>,
    pub depth_info: Option<DepthInfo>,
}
#[derive(Clone, Copy, Default)]
pub struct DepthInfo {
    pub flags: [u8; 4],
    pub values: [f64; 4],
    pub representation_type: i32,
    pub disparity_reference_view: u32,
}
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn get(&mut self, count: usize) -> u32 {
        let mut n = 0;
        for _ in 0..count {
            n = (n << 1)
                | u32::from(
                    self.data
                        .get(self.at / 8)
                        .map_or(0, |v| (v >> (7 - self.at % 8)) & 1),
                );
            self.at += 1;
        }
        n
    }
    fn ue(&mut self, message: &str) -> Result<u32, ContextError> {
        let mut zeros = 0;
        while self.get(1) == 0 {
            zeros += 1;
            if zeros > 20 {
                return Err(invalid(message));
            }
        }
        Ok((1 << zeros) - 1 + self.get(zeros))
    }
    fn element(&mut self) -> f64 {
        let negative = self.get(1) != 0;
        let exponent = self.get(7) as i32;
        let length = self.get(5) as usize + 1;
        let mantissa = f64::from(self.get(length));
        let value = if exponent > 0 {
            2f64.powi(exponent - 31) * (1.0 + mantissa / 2f64.powi(length as i32))
        } else {
            2f64.powi(-(30 + length as i32)) * mantissa
        };
        if negative { -value } else { value }
    }
}
fn invalid(message: &str) -> ContextError {
    ContextError::invalid(2006, &format!("Invalid parameter value: {message}"))
}
/// The pinned reference reads only the first NAL and one-byte SEI headers. Its
/// declared NAL/payload lengths do not bound the zero-padded depth bit reader.
pub fn depth_info(data: &[u8]) -> Result<Option<DepthInfo>, ContextError> {
    let short = || ContextError::invalid(100, "Unexpected end of file: HEVC SEI NAL too short");
    if data.len() < 4 {
        return Err(short());
    }
    let mut bits = Bits { data, at: 0 };
    if bits.get(32) <= 4 {
        return Ok(None);
    }
    if data.len() < 9 {
        return Err(short());
    }
    bits.get(32); // NAL length, currently ignored upstream.
    let kind = bits.get(8) >> 1;
    bits.get(8);
    if kind != 39 && kind != 40 {
        return Ok(None);
    }
    if data.len() < 12 {
        return Err(short());
    }
    let payload = bits.get(8);
    bits.get(8); // Payload length, currently ignored upstream.
    if payload != 177 {
        return Ok(None);
    }
    let mut info = DepthInfo::default();
    for flag in &mut info.flags {
        *flag = bits.get(1) as u8;
    }
    let kind = bits.ue("invalid depth representation type in input")?;
    if kind > 3 {
        return Err(invalid("input depth representation type out of range"));
    }
    info.representation_type = kind as i32;
    if info.flags[2] != 0 || info.flags[3] != 0 {
        info.disparity_reference_view = bits.ue("invalid disparity_reference_view in input")?;
    }
    for (flag, value) in info.flags.iter().zip(&mut info.values) {
        if *flag != 0 {
            *value = bits.element();
        }
    }
    Ok(Some(info))
}
