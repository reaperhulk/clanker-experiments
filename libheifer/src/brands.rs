// SPDX-License-Identifier: LGPL-3.0-or-later
// Compatibility semantics adapted from libheif, Copyright Dirk Farin and contributors.
use crate::error::Error;
use std::ffi::CStr;

pub const fn fourcc(bytes: [u8; 4]) -> u32 {
    u32::from_be_bytes(bytes)
}

/// libheif's conversion rejects a zero byte anywhere in the fourcc.
pub fn to_brand(bytes: [u8; 4]) -> u32 {
    if bytes.contains(&0) { 0 } else { fourcc(bytes) }
}

pub fn main_brand(data: &[u8]) -> u32 {
    data.get(8..12)
        .map_or(0, |s| to_brand(s.try_into().unwrap()))
}

pub fn minor_version_brand(data: &[u8]) -> u32 {
    data.get(12..16)
        .map_or(0, |s| to_brand(s.try_into().unwrap()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum FileType {
    No = 0,
    Supported = 1,
    Unsupported = 2,
    Maybe = 3,
}

pub fn check_filetype(data: &[u8]) -> FileType {
    if data.len() < 8 {
        return FileType::Maybe;
    }
    if &data[4..8] != b"ftyp" {
        return FileType::No;
    }
    if data.len() < 12 {
        return FileType::Maybe;
    }
    match &data[8..12] {
        b"heic" | b"heix" | b"avif" | b"jpeg" | b"j2ki" => FileType::Supported,
        b"mif1" | b"mif2" => FileType::Maybe,
        _ => FileType::Unsupported,
    }
}

pub fn check_jpeg(data: &[u8]) -> i32 {
    if data.len() < 4 {
        -1
    } else {
        i32::from(data[0..3] == [0xff, 0xd8, 0xff] && data[3] & 0xf0 == 0xe0)
    }
}

pub fn mime_type(data: &[u8]) -> &'static CStr {
    // Deliberately follows upstream precedence: brand detection precedes JPEG/PNG.
    let brand = data.get(8..12).unwrap_or(&[]);
    match brand {
        b"heic" | b"heix" | b"heim" | b"heis" => c"image/heic",
        b"mif1" => c"image/heif",
        b"hevc" | b"hevx" | b"hevm" | b"hevs" => c"image/heic-sequence",
        b"msf1" => c"image/heif-sequence",
        b"avif" => c"image/avif",
        b"avis" => c"image/avif-sequence",
        b"avci" => c"image/avci",
        b"avcs" => c"image/avcs",
        b"j2ki" => c"image/hej2k",
        b"j2is" => c"image/j2is",
        b"mif3" => match data.get(12..16).unwrap_or(&[]) {
            b"avif" => c"image/avif",
            b"heic" | b"heix" | b"heim" | b"heis" => c"image/heic",
            _ => c"image/heif",
        },
        _ if data.starts_with(&[
            0xff, 0xd8, 0xff, 0xe0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 1,
        ]) || (data.len() >= 12
            && data[..4] == [0xff, 0xd8, 0xff, 0xe1]
            && &data[6..12] == b"Exif\0\0") =>
        {
            c"image/jpeg"
        }
        _ if data.starts_with(b"\x89PNG\r\n\x1a\n") => c"image/png",
        _ => c"",
    }
}

pub fn legacy_brand(data: &[u8]) -> i32 {
    match data.get(8..12).unwrap_or(&[]) {
        b"heic" => 1,
        b"heix" => 2,
        b"hevc" => 3,
        b"hevx" => 4,
        b"heim" => 5,
        b"heis" => 6,
        b"hevm" => 7,
        b"hevs" => 8,
        b"mif1" => 9,
        b"msf1" => 10,
        b"avif" => 11,
        b"avis" => 12,
        b"vvic" => 13,
        b"j2ki" => 17,
        b"j2is" => 18,
        _ => 0,
    }
}

/// Borrowed, validated compatible-brand list. Parsing does not allocate.
#[derive(Clone, Copy, Debug)]
pub struct CompatibleBrands<'a> {
    bytes: &'a [u8],
}

impl<'a> CompatibleBrands<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        if data.is_empty() {
            return Err(Error::new(5, 2006, c"data length must be positive"));
        }
        let header_error = Error::new(2, 100, c"error reading ftype box header");
        if data.len() < 8 {
            return Err(header_error);
        }
        let size32 = u32::from_be_bytes(data[..4].try_into().unwrap());
        let (size, mut header) = if size32 == 1 {
            if data.len() < 16 {
                return Err(header_error);
            }
            let n = u64::from_be_bytes(data[8..16].try_into().unwrap());
            if n > 0x0fff_ffff_ffff_ffff {
                return Err(Error::new(6, 1000, c"error reading ftype box header"));
            }
            (n, 16_usize)
        } else {
            (u64::from(size32), 8_usize)
        };
        if &data[4..8] == b"uuid" {
            header += 16;
            if data.len() < header {
                return Err(header_error);
            }
        }
        if &data[4..8] != b"ftyp" {
            return Err(Error::new(2, 0, c"File does not begin with 'ftyp' box."));
        }
        if size != 0 && size < header as u64 {
            return Err(Error::new(2, 101, c"error reading ftyp box"));
        }
        if size > data.len() as u64 {
            return Err(Error::new(2, 100, c"insufficient input data"));
        }
        if size < (header + 8) as u64 {
            return Err(Error::new(2, 101, c"error reading ftyp box"));
        }
        let count = (size as usize - header - 8) / 4;
        if count > 1000 {
            return Err(Error::new(6, 1000, c"error reading ftyp box"));
        }
        Ok(Self {
            bytes: &data[header + 8..header + 8 + count * 4],
        })
    }
    pub fn len(self) -> usize {
        self.bytes.len() / 4
    }
    pub fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }
    pub fn iter(self) -> impl ExactSizeIterator<Item = u32> + 'a {
        self.bytes
            .chunks_exact(4)
            .map(|v| u32::from_be_bytes(v.try_into().unwrap()))
    }
    pub fn contains(self, brand: u32) -> bool {
        self.iter().any(|b| b == brand)
    }
}

pub fn has_compatible_brand(data: &[u8], brand: [u8; 4]) -> i32 {
    if data.is_empty() || brand.contains(&0) {
        return -1;
    }
    // Unlike list_compatible_brands, this entry point asks Box::read to read
    // *any* box before testing whether it was ftyp. Preserve that error order.
    if data.len() >= 8 && &data[4..8] != b"ftyp" {
        return if crate::box_probe::first_box_truncated(data) {
            -1
        } else {
            -2
        };
    }
    match CompatibleBrands::parse(data) {
        Ok(brands) => i32::from(brands.contains(fourcc(brand))),
        Err(e) if e.subcode == 100 => -1,
        Err(_) => -2,
    }
}

pub fn has_compatible_filetype(data: &[u8]) -> Result<(), Error> {
    let brands = CompatibleBrands::parse(data)?;
    fn supported(brand: u32) -> bool {
        matches!(
            &brand.to_be_bytes(),
            b"avif"
                | b"heic"
                | b"heix"
                | b"j2ki"
                | b"jpeg"
                | b"miaf"
                | b"mif1"
                | b"mif2"
                | b"mif3"
                | b"msf1"
                | b"isom"
                | b"mp41"
                | b"mp42"
        )
    }
    if supported(main_brand(data)) || brands.iter().any(supported) {
        Ok(())
    } else {
        Err(Error::new(2, 3001, c"No supported brands found."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_compatibility_brands_preserves_order_and_duplicates() {
        let data = b"\0\0\0\x1cftypavif\0\0\0\0mif1avifmif1";
        let brands = CompatibleBrands::parse(data).unwrap();
        assert_eq!(
            brands.iter().collect::<Vec<_>>(),
            vec![fourcc(*b"mif1"), fourcc(*b"avif"), fourcc(*b"mif1")]
        );
        assert_eq!(has_compatible_brand(data, *b"avif"), 1);
        assert_eq!(has_compatible_filetype(data), Ok(()));
    }
    #[test]
    fn truncated_inputs_do_not_panic() {
        let data = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic";
        for len in 0..data.len() {
            assert!(CompatibleBrands::parse(&data[..len]).is_err());
            let _ = (
                main_brand(&data[..len]),
                mime_type(&data[..len]),
                check_filetype(&data[..len]),
            );
        }
    }
}
