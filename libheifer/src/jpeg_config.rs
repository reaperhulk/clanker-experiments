// SPDX-License-Identifier: LGPL-3.0-or-later
//! The pinned JPEG decoder's SOF description scan; entropy decoding is separate.
use crate::context::ContextError;
#[derive(Clone, Copy, Debug)]
pub struct Description {
    pub precision: u8,
    pub colorspace: i32,
    pub chroma: i32,
}
pub fn parse(data: &[u8]) -> Result<Description, ContextError> {
    let invalid = || ContextError::new(2, 0, "Invalid input: Unspecified: Invalid JPEG SOF header");
    for i in 0..data.len().saturating_sub(1) {
        if data[i] != 0xff
            || data[i + 1] & 0xf0 != 0xc0
            || matches!(data[i + 1], 0xc4 | 0xc8 | 0xcc)
        {
            continue;
        }
        let data = &data[i..];
        if data.len() <= 9 {
            return Err(invalid());
        }
        let count = usize::from(data[9]);
        // Upstream requires bytes past the nominal component table.
        if 11 + 3 * count >= data.len() {
            return Err(invalid());
        }
        let chroma = match count {
            1 => 0,
            3 if data[14] == data[17] && data[14] == 0x11 => match data[11] {
                0x22 => 1,
                0x21 => 2,
                0x11 => 3,
                _ => return Err(invalid()),
            },
            _ => return Err(invalid()),
        };
        return Ok(Description {
            precision: data[4],
            colorspace: if chroma == 0 { 2 } else { 0 },
            chroma,
        });
    }
    Err(invalid())
}
