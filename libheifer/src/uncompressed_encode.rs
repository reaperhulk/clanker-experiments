// SPDX-License-Identifier: LGPL-3.0-or-later
//! Uncompressed planar and packed RGB encoding without native codec dependencies.
use crate::{
    context::ContextError,
    encoding::property,
    image::Image,
    properties::Property,
    uncompressed::{Component, Configuration},
};
type Result<T> = std::result::Result<T, ContextError>;
fn unsupported() -> ContextError {
    ContextError::new(
        3,
        0,
        "Unsupported file-type: Unspecified: Input image configuration unsupported by uncompressed codec.",
    )
}
fn invalid(message: &str) -> ContextError {
    ContextError::new(2, 0, format!("Invalid input: Unspecified: {message}"))
}
type CodedImage = (Vec<u8>, Vec<(Property, bool)>);
pub fn encode(image: &Image, compression: i32) -> Result<CodedImage> {
    encode_layout(image, compression, 1, 1)
}
pub fn encode_tiled(image: &Image, columns: u32, rows: u32) -> Result<CodedImage> {
    encode_layout(image, 0, columns, rows)
}
fn encode_layout(image: &Image, compression: i32, columns: u32, rows: u32) -> Result<CodedImage> {
    let mut descriptions: Vec<_> = image.component_ids.descriptions.iter().collect();
    descriptions.sort_by_key(|d| d.id);
    let planes: Vec<_> = descriptions
        .iter()
        .copied()
        .filter(|d| d.has_data)
        .collect();
    let interleaved = image.plane(10);
    if (10..=15).contains(&image.chroma) && interleaved.is_none() {
        return Err(invalid(
            "Image has an interleaved chroma format, but no interleaved pixel plane.",
        ));
    }
    if interleaved.is_none() && planes.is_empty() {
        return Err(invalid("Image has no pixel planes."));
    }
    let mut config = Configuration::default();
    let mut data = Vec::new();
    for d in &planes {
        let w = if matches!(d.channel, 1 | 2) && matches!(image.chroma, 1 | 2) {
            image.width.div_ceil(2)
        } else {
            image.width
        };
        let h = if matches!(d.channel, 1 | 2) && image.chroma == 1 {
            image.height.div_ceil(2)
        } else {
            image.height
        };
        if d.width != w || d.height != h {
            return Err(invalid(
                "Image component plane size does not match the image dimensions",
            ));
        }
    }
    if let Some(plane) = interleaved {
        if image.colorspace != 1 || !(10..=15).contains(&image.chroma) {
            return Err(unsupported());
        }
        let bits = u16::from(plane.bit_depth);
        let count = if matches!(image.chroma, 11 | 13 | 15) {
            4
        } else {
            3
        };
        let block = matches!(image.chroma, 12 | 14) && bits < 14;
        let bytes = if image.chroma <= 11 { 1 } else { 2 };
        let align = if block || bits == bytes * 8 {
            0
        } else {
            bytes as u8
        };
        config.interleave = 1;
        if bits == 8 && image.chroma <= 11 {
            config.version = 1;
            config.profile = if count == 3 { *b"rgb3" } else { *b"rgba" };
        }
        if image.chroma >= 12 {
            config.pixel_size = if block {
                (u32::from(bits) * 3).div_ceil(8)
            } else {
                count * 2
            };
        }
        if block {
            config.block_size = config.pixel_size as u8;
        }
        for id in &plane.component_ids {
            let index = descriptions
                .iter()
                .position(|d| d.id == *id)
                .ok_or_else(unsupported)?;
            config.components.push(Component {
                index: index as u32,
                bits,
                format: 0,
                align,
            });
        }
        for y in 0..image.height as usize {
            let row = &plane.data()[y * plane.stride..];
            if block {
                for x in 0..image.width as usize {
                    let sample = |c: usize| {
                        u16::from_ne_bytes(
                            row[x * 6 + c * 2..x * 6 + c * 2 + 2].try_into().unwrap(),
                        ) as u64
                    };
                    let value = (sample(0) << (2 * bits)) | (sample(1) << bits) | sample(2);
                    data.extend(&value.to_be_bytes()[8 - config.pixel_size as usize..]);
                }
            } else {
                let row = &row[..image.width as usize * count as usize * bytes as usize];
                if image.chroma >= 14 {
                    for pair in row.chunks_exact(2) {
                        data.extend([pair[1], pair[0]]);
                    }
                } else {
                    data.extend(row);
                }
            }
        }
    } else {
        let dense = planes.iter().any(|d| d.bit_depth % 8 != 0);
        if planes.iter().any(|d| {
            if dense {
                d.bit_depth > 32
            } else {
                !matches!(d.bit_depth, 8 | 16 | 32 | 64 | 128 | 256)
            }
        }) {
            return Err(unsupported());
        }
        config.flags = if !dense && cfg!(target_endian = "little") {
            0x80
        } else {
            0
        };
        config.sampling = match image.chroma {
            1 => 2,
            2 => 1,
            _ => 0,
        };
        for d in planes {
            let index = descriptions.iter().position(|p| p.id == d.id).unwrap();
            config.components.push(Component {
                index: index as u32,
                bits: d.bit_depth,
                format: if d.datatype == 255 {
                    0
                } else {
                    d.datatype as u8
                },
                align: 0,
            });
            let plane = image.component_plane(d.id).ok_or_else(unsupported)?;
            for y in 0..d.height as usize {
                let row = &plane.data()[y * plane.stride..];
                if !dense {
                    data.extend(&row[..d.width as usize * usize::from(d.bit_depth / 8)]);
                } else {
                    let mut acc = 0u64;
                    let mut n = 0u16;
                    for x in 0..d.width as usize {
                        let value = if d.bit_depth <= 8 {
                            u64::from(row[x])
                        } else if d.bit_depth <= 16 {
                            u64::from(u16::from_ne_bytes(
                                row[x * 2..x * 2 + 2].try_into().unwrap(),
                            ))
                        } else {
                            u64::from(u32::from_ne_bytes(
                                row[x * 4..x * 4 + 4].try_into().unwrap(),
                            ))
                        };
                        acc = (acc << d.bit_depth) | value;
                        n += d.bit_depth;
                        while n >= 8 {
                            n -= 8;
                            data.push((acc >> n) as u8);
                            acc &= (1u64 << n) - 1;
                        }
                    }
                    if n != 0 {
                        data.push((acc << (8 - n)) as u8);
                    }
                }
            }
        }
    }
    if columns != 1 || rows != 1 {
        config.version = 0;
    }
    let mut unc = vec![config.version, 0, 0, 0];
    unc.extend(config.profile);
    let mut props = Vec::new();
    if config.version == 0 {
        unc.extend((config.components.len() as u32).to_be_bytes());
        for c in &config.components {
            unc.extend((c.index as u16).to_be_bytes());
            unc.extend([(c.bits - 1) as u8, c.format, c.align]);
        }
        unc.extend([
            config.sampling,
            config.interleave,
            config.block_size,
            config.flags,
        ]);
        for n in [config.pixel_size, 0, 0, columns - 1, rows - 1] {
            unc.extend(n.to_be_bytes());
        }
    }
    props.push((property(*b"uncC", unc), true));
    if config.version == 0 {
        let mut cmpd = (descriptions.len() as u32).to_be_bytes().to_vec();
        for d in descriptions {
            cmpd.extend(d.kind.to_be_bytes());
            if d.kind >= 0x8000 {
                cmpd.push(0);
            }
        }
        props.push((property(*b"cmpd", cmpd), true));
    }
    if compression != 0 {
        let kind = match compression {
            3 => *b"defl",
            4 => *b"zlib",
            _ => {
                return Err(ContextError::new(
                    4,
                    3006,
                    "Unsupported feature: Unsupported generic compression method: Unsupported unci compression method.",
                ));
            }
        };
        data = crate::compression::compress(&data, compression)?;
        let mut cmp = vec![0; 4];
        cmp.extend(kind);
        cmp.push(2);
        props.push((property(*b"cmpC", cmp), false));
    }
    Ok((data, props))
}
