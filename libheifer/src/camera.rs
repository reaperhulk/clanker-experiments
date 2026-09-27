// SPDX-License-Identifier: LGPL-3.0-or-later
// Camera property semantics adapted from libheif, Copyright Dirk Farin and contributors.
use crate::context::ContextError;
type Result<T> = std::result::Result<T, ContextError>;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct IntrinsicMatrix {
    pub focal_length_x: f64,
    pub focal_length_y: f64,
    pub principal_point_x: f64,
    pub principal_point_y: f64,
    pub skew: f64,
}
impl IntrinsicMatrix {
    pub fn mirror(&mut self, horizontal: bool, width: u32, height: u32) {
        if horizontal {
            self.focal_length_x *= -1.0;
            self.skew *= -1.0;
            self.principal_point_x = f64::from(width.wrapping_sub(1)) - self.principal_point_x;
        } else {
            self.focal_length_y *= -1.0;
            self.principal_point_y = f64::from(height.wrapping_sub(1)) - self.principal_point_y;
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    failed: bool,
}
impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            failed: false,
        }
    }
    fn number(&mut self, size: usize) -> u32 {
        if self.failed || self.data.len() < size {
            self.failed = true;
            return 0;
        }
        let value = self.data[..size]
            .iter()
            .fold(0, |a, b| a << 8 | u32::from(*b));
        self.data = &self.data[size..];
        value
    }
    fn signed(&mut self, wide: bool) -> f64 {
        if wide {
            f64::from(self.number(4) as i32)
        } else {
            f64::from(self.number(2) as i16)
        }
    }
    fn header(&mut self, kind: &str) -> Result<u32> {
        let header = self.number(4);
        let version = header >> 24;
        if version != 0 {
            return Err(ContextError::new(
                4,
                3002,
                format!(
                    "Unsupported feature: Unsupported data version: {kind} box data version {version} is not implemented yet"
                ),
            ));
        }
        Ok(header & 0x00ff_ffff)
    }
    fn finish(self) -> Result<()> {
        if self.failed {
            Err(ContextError::invalid(100, "Unexpected end of file"))
        } else {
            Ok(())
        }
    }
}

pub fn intrinsic(data: &[u8], width: u32, height: u32) -> Result<IntrinsicMatrix> {
    let mut r = Reader::new(data);
    let flags = r.header("cmin")?;
    let denominator = f64::from(1u32 << ((flags >> 8) & 31));
    let fx = r.signed(true) / denominator;
    let px = r.signed(true) / denominator;
    let py = r.signed(true) / denominator;
    let mut m = IntrinsicMatrix {
        focal_length_x: fx * f64::from(width as i32),
        focal_length_y: fx * f64::from(width as i32),
        principal_point_x: px * f64::from(width as i32),
        principal_point_y: py * f64::from(height as i32),
        skew: 0.0,
    };
    if flags & 1 != 0 {
        m.focal_length_y = r.signed(true) / denominator * f64::from(height as i32);
        m.skew = r.signed(true) / f64::from(1u32 << ((flags >> 16) & 31));
    }
    r.finish()?;
    Ok(m)
}

#[derive(Clone, Copy, Debug)]
pub struct ExtrinsicMatrix {
    pub quaternion: [f64; 4],
}
impl ExtrinsicMatrix {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let flags = r.header("cmex")?;
        for flag in [1, 2, 4] {
            if flags & flag != 0 {
                r.number(4);
            }
        }
        let mut quaternion = [0.0, 0.0, 0.0, 1.0];
        if flags & 8 != 0 {
            let wide = flags & 16 != 0;
            let denominator = f64::from(1u32 << if wide { 30 } else { 14 });
            for value in &mut quaternion[..3] {
                *value = r.signed(wide) / denominator;
            }
            let [x, y, z, _] = quaternion;
            let sum = x * x + y * y + z * z;
            if sum > 1.0 {
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: Invalid quaternion in extrinsic rotation matrix",
                ));
            }
            quaternion[3] = (1.0 - sum).sqrt();
        }
        if flags & 32 != 0 {
            r.number(4);
        }
        r.finish()?;
        Ok(Self { quaternion })
    }
    pub fn rotation(&self) -> [f64; 9] {
        let [x, y, z, w] = self.quaternion;
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ]
    }
}

pub(crate) fn kind(kind: [u8; 4], uuid: Option<[u8; 16]>) -> [u8; 4] {
    if kind == *b"uuid" {
        match uuid {
            Some(
                [
                    0x22,
                    0xcc,
                    0x04,
                    0xc7,
                    0xd6,
                    0xd9,
                    0x4e,
                    0x07,
                    0x9d,
                    0x90,
                    0x4e,
                    0xb6,
                    0xec,
                    0xba,
                    0xf3,
                    0xa3,
                ],
            ) => return *b"cmin",
            Some(
                [
                    0x43,
                    0x63,
                    0xe9,
                    0x14,
                    0x5b,
                    0x7d,
                    0x4a,
                    0xab,
                    0x97,
                    0xae,
                    0xbe,
                    0xa6,
                    0x98,
                    0x03,
                    0xb4,
                    0x34,
                ],
            ) => return *b"cmex",
            _ => {}
        }
    }
    kind
}
