// SPDX-License-Identifier: LGPL-3.0-or-later
//! Scalar Rust JPEG 2000 samples with the pinned native adapter's plane contract.
use crate::{
    context::{ContextError, Document},
    decoding::DecodeOptions,
    image::Image,
};
fn error(message: &str) -> ContextError {
    ContextError::new(
        7,
        0,
        format!("Decoder plugin generated an error: Unspecified: {message}"),
    )
}
fn limit(message: &str) -> ContextError {
    ContextError::new(
        6,
        1000,
        format!("Memory allocation error: Security limit exceeded: {message}"),
    )
}
pub fn decode(
    document: &Document,
    id: u32,
    options: &DecodeOptions,
) -> Result<Image, ContextError> {
    if options.decoder_id.is_some_and(|id| id != b"hayro-jpeg2000") {
        return Err(ContextError::new(
            11,
            0,
            "Error while loading plugin: Unspecified: No decoder with that ID found.",
        ));
    }
    let data = crate::decoding::decoder_payload(document, id)?;
    if data.is_empty() {
        return Err(ContextError::invalid(
            0,
            "Unspecified: Input with empty data extent.",
        ));
    }
    let h = crate::jpeg2000_config::header(&data).ok_or_else(|| error("opj_read_header()"))?;
    if h.components
        .iter()
        .any(|c| c.dx == 0 || c.dy == 0 || c.depth > 31)
    {
        return Err(error("opj_read_header()"));
    }
    if usize::from(u16::from_be_bytes([data[4], data[5]])) != 38 + 3 * h.components.len() {
        return Err(error("opj_read_header()"));
    }
    let field = |at| u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
    let (x1, y1, tw, th, tx, ty) = (
        field(8),
        field(12),
        field(24),
        field(28),
        field(32),
        field(36),
    );
    if h.x0 >= x1
        || h.y0 >= y1
        || tw == 0
        || th == 0
        || tx > h.x0
        || ty > h.y0
        || tx.saturating_add(tw) <= h.x0
        || ty.saturating_add(th) <= h.y0
    {
        return Err(error("opj_read_header()"));
    }
    if u64::from((x1 - tx).div_ceil(tw)) * u64::from((y1 - ty).div_ceil(th)) > 65535 {
        return Err(error("opj_read_header()"));
    }
    let settings = hayro_jpeg2000::DecodeSettings::default();
    let coded =
        hayro_jpeg2000::Image::new(&data, &settings).map_err(|_| error("opj_read_header()"))?;
    let limits = document.current_limits();
    let (iw, ih) = document.images[&id].ispe;
    let mut max_pixels = limits.max_image_size_pixels;
    if iw != 0 && ih != 0 {
        let maximum = ((u64::from(iw) + 64) * (u64::from(ih) + 64)).max(65536);
        if max_pixels == 0 || maximum < max_pixels {
            max_pixels = maximum;
        }
    }
    if max_pixels > 0 && u64::from(h.width) * u64::from(h.height) > max_pixels {
        return Err(limit("JPEG 2000 image exceeds maximum allowed image size"));
    }
    let mut estimated = 0u64;
    for c in &h.components {
        let w = (h.width + h.x0).div_ceil(u32::from(c.dx)) - h.x0.div_ceil(u32::from(c.dx));
        let height = (h.height + h.y0).div_ceil(u32::from(c.dy)) - h.y0.div_ceil(u32::from(c.dy));
        estimated = estimated.saturating_add(u64::from(w) * u64::from(height) * 4);
    }
    estimated = estimated.saturating_mul(3);
    if limits.max_memory_block_size > 0 && estimated > limits.max_memory_block_size {
        return Err(limit(
            "JPEG 2000 image would require too much memory to decode",
        ));
    }
    if !matches!(h.components.len(), 1 | 3) {
        return Err(ContextError::new(
            4,
            3002,
            "Unsupported feature: Unsupported data version: Number of components must be 3 or 1",
        ));
    }
    validate_tile_headers(&data)?;
    let mut ctx = hayro_jpeg2000::DecoderContext::default();
    let decoded = coded
        .decode_raw_components(&mut ctx)
        .map_err(|_| error("opj_decode()"))?;
    let chroma = if h.components.len() == 1 {
        0
    } else {
        match (h.components[1].dx, h.components[1].dy) {
            (1, 1) => 3,
            (2, 1) => 2,
            (2, 2) => 1,
            _ => return Err(error("unsupported image format")),
        }
    };
    let mut image = Image::new(h.width, h.height, if chroma == 0 { 2 } else { 0 }, chroma)?;
    image.budget = Some(document.budget.clone());
    for (index, c) in h.components.iter().enumerate() {
        let dx = if index > 0 && matches!(chroma, 1 | 2) {
            2
        } else {
            1
        };
        let dy = if index > 0 && chroma == 1 { 2 } else { 1 };
        let w = h.width.div_ceil(dx);
        let height = h.height.div_ceil(dy);
        let cw = (h.width + h.x0).div_ceil(u32::from(c.dx)) - h.x0.div_ceil(u32::from(c.dx));
        let ch = (h.height + h.y0).div_ceil(u32::from(c.dy)) - h.y0.div_ceil(u32::from(c.dy));
        if cw != w || ch != height {
            return Err(error(
                "JPEG 2000 component size does not match the image's chroma subsampling",
            ));
        }
        image.add_plane(index as i32, w, height, i32::from(c.depth))?;
        let p = image.plane_mut(index as i32).unwrap();
        let stride = p.stride;
        let component = decoded.get(index).ok_or_else(|| error("opj_decode()"))?;
        let samples = component.samples();
        let offset = if c.signed { 0 } else { 1i64 << (c.depth - 1) };
        let minimum = if c.signed {
            -(1i64 << (c.depth - 1))
        } else {
            0
        };
        let maximum = (1i64 << (c.depth - u8::from(c.signed))) - 1;
        for y in 0..height as usize {
            for x in 0..w as usize {
                if !component.is_present(x as u32 * dx, y as u32 * dy) {
                    continue;
                }
                let sample = *samples
                    .get((y * dy as usize) * h.width as usize + x * dx as usize)
                    .ok_or_else(|| error("opj_decode()"))?;
                let value =
                    (sample.round_ties_even() as i64 + offset).clamp(minimum, maximum) as u16;
                if c.depth <= 8 {
                    p.data_mut()[y * stride + x] = value as u8;
                } else {
                    p.data_mut()[y * stride + x * 2..y * stride + x * 2 + 2]
                        .copy_from_slice(&value.to_ne_bytes());
                }
            }
        }
    }
    Ok(image)
}

fn validate_tile_headers(data: &[u8]) -> Result<(), ContextError> {
    let mut at = 2usize;
    let mut tile_end = None;
    let mut tile_parts = std::collections::BTreeMap::<u16, u16>::new();
    while at + 2 <= data.len() {
        let marker = [data[at], data[at + 1]];
        if marker == [255, 217] {
            return Ok(());
        }
        if marker == [255, 147] {
            if let Some(end) = tile_end {
                if end < at + 2 {
                    return Err(error("opj_decode()"));
                }
                if end > data.len() {
                    return Ok(());
                }
                if end + 2 > data.len() {
                    return Err(error("opj_decode()"));
                }
                at = end;
                tile_end = None;
                continue;
            }
            return Ok(());
        }
        let length = data
            .get(at + 2..at + 4)
            .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
            .ok_or_else(|| error("opj_decode()"))?;
        if length < 2 {
            return Err(error("opj_decode()"));
        }
        if marker == [255, 144] {
            let record = data
                .get(at + 4..at + 12)
                .ok_or_else(|| error("opj_decode()"))?;
            let tile = u16::from_be_bytes([record[0], record[1]]);
            let next = tile_parts.entry(tile).or_default();
            if *next != u16::from(record[6]) {
                return Err(error("opj_decode()"));
            }
            *next += 1;

            let b = data
                .get(at + 6..at + 10)
                .ok_or_else(|| error("opj_decode()"))?;
            let size = u32::from_be_bytes(b.try_into().unwrap()) as usize;
            if size > 0 {
                tile_end = Some(at + size);
            }
        }
        at += length + 2;
    }
    Err(error("opj_decode()"))
}
