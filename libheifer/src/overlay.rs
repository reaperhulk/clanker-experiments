// SPDX-License-Identifier: LGPL-3.0-or-later
// Overlay semantics adapted from libheif, Copyright Dirk Farin and contributors.
use crate::{container::Container, context::ContextError};
use crate::{
    context::Document,
    decoding::{DecodeOptions, DecodeState},
    image::Image,
};

pub struct Overlay {
    pub width: u32,
    pub height: u32,
    pub background: [u16; 4],
    pub offsets: Vec<(i32, i32)>,
}
impl Overlay {
    pub fn parse(data: &[u8], count: usize) -> Result<Self, ContextError> {
        let incomplete =
            || ContextError::invalid(121, "Invalid overlay data: Overlay image data incomplete");
        if data.len() < 10 {
            return Err(incomplete());
        }
        if data[0] != 0 {
            return Err(ContextError::new(
                4,
                3002,
                format!(
                    "Unsupported feature: Unsupported data version: Overlay image data version {} is not implemented yet",
                    data[0]
                ),
            ));
        }
        let bytes = if data[1] & 1 != 0 { 4 } else { 2 };
        if count
            .checked_mul(2)
            .and_then(|n| n.checked_add(2))
            .and_then(|n| n.checked_mul(bytes))
            .and_then(|n| n.checked_add(10))
            .is_none_or(|n| n > data.len())
        {
            return Err(incomplete());
        }
        let background =
            std::array::from_fn(|i| u16::from_be_bytes([data[2 + i * 2], data[3 + i * 2]]));
        let mut fields = data[10..].chunks_exact(bytes);
        let mut next = || {
            fields
                .next()
                .unwrap()
                .iter()
                .fold(0u32, |v, b| (v << 8) | u32::from(*b))
        };
        let (width, height) = (next(), next());
        if width == 0 || height == 0 {
            return Err(ContextError::invalid(
                121,
                "Invalid overlay data: Overlay image with zero width or height.",
            ));
        }
        // The pinned narrow-field parser sets bit 31 for negative 16-bit values,
        // rather than sign-extending bit 15. Preserve that observable behavior.
        let signed = |value: u32| {
            if value & (0x80 << ((bytes - 1) * 8)) != 0 {
                (value | 0x8000_0000) as i32
            } else {
                value as i32
            }
        };
        let offsets = (0..count)
            .map(|_| (signed(next()), signed(next())))
            .collect();
        Ok(Self {
            width,
            height,
            background,
            offsets,
        })
    }
    pub fn load(container: &Container<'_>, id: u32) -> Result<Self, ContextError> {
        if !container.has_references {
            return Err(ContextError::invalid(
                113,
                "No 'iref' box: No iref box available, but needed for iovl image",
            ));
        }
        let count = container.items[&id]
            .references
            .get(b"dimg")
            .map_or(0, Vec::len);
        if count == 0 {
            return Err(ContextError::invalid(
                119,
                "Missing grid images: 'iovl' image has no referenced input images",
            ));
        }
        if container.limits.max_items != 0 && count > 5 {
            return Err(ContextError::invalid(
                1000,
                "Security limit exceeded: 'iovl' image composites more input images than allowed",
            ));
        }
        Self::parse(&container.payload(id)?, count)
    }
}

pub(crate) fn decode(
    document: &Document,
    id: u32,
    options: &DecodeOptions<'_>,
    visiting: &mut DecodeState,
) -> Result<Image, ContextError> {
    let container = document.container()?;
    if document.current_limits().max_items != 0
        && visiting
            .ids
            .iter()
            .filter(|id| container.items[id].kind == *b"iovl")
            .count()
            > 3
    {
        return Err(ContextError::invalid(
            1000,
            "Security limit exceeded: 'iovl' overlay images nested too deeply",
        ));
    }
    let overlay = document.images[&id]
        .overlay
        .as_ref()
        .expect("loaded overlay header");
    let (w, h) = (overlay.width, overlay.height);
    document.current_limits().check_image_size(w, h)?;
    let mut canvas = Image::new(w, h, 1, 3)?.with_budget(Some(document.budget.clone()));
    for channel in 3..=5 {
        canvas.add_plane(channel, w, h, 8)?;
        let plane = canvas.plane_mut(channel).unwrap();
        plane
            .data_mut()
            .fill((overlay.background[channel as usize - 3] >> 8) as u8);
    }
    for (&child, &(dx, dy)) in container.items[&id].references[b"dimg"]
        .iter()
        .zip(&overlay.offsets)
    {
        if !document.images.contains_key(&child) {
            return Err(ContextError::invalid(
                2000,
                "Non-existing item ID referenced: 'iovl' image references a non-existing item.",
            ));
        }
        let mut tile = crate::decoding::decode_native(document, child, options, visiting)?;
        if tile.colorspace != 1 || tile.chroma != 3 {
            tile = crate::conversion::convert(
                tile,
                1,
                3,
                crate::color::Nclx {
                    primaries: 2,
                    transfer: 2,
                    matrix: 2,
                    full_range: true,
                },
                0,
                options.color_conversion,
            )?;
        }
        blend(&mut canvas, &tile, dx, dy);
    }
    Ok(canvas)
}

// Upstream's overlay loop is byte based, including its negative-offset clipping
// asymmetry. Bounds below use signed wide arithmetic before narrowing to indices.
fn blend(canvas: &mut Image, tile: &Image, dx: i32, dy: i32) {
    let alpha = tile.plane(6);
    for channel in tile.channels() {
        let input = tile.plane(channel).unwrap();
        let Some(output) = canvas.plane_mut(channel) else {
            continue;
        };
        if i64::from(dx) >= i64::from(output.width)
            || i64::from(dy) >= i64::from(output.height)
            || i64::from(dx) + i64::from(input.width) <= 0
            || i64::from(dy) + i64::from(input.height) <= 0
        {
            continue;
        }
        let in_x = (-i64::from(dx)).max(0) as usize;
        let in_y = (-i64::from(dy)).max(0) as usize;
        let out_x = i64::from(dx).max(0) as usize;
        let out_y = i64::from(dy).max(0) as usize;
        let width =
            i64::from(input.width).min(i64::from(output.width) - i64::from(dx)) as usize - in_x;
        let height =
            i64::from(input.height).min(i64::from(output.height) - i64::from(dy)) as usize - in_y;
        for y in in_y..height {
            let from = in_x + y * input.stride;
            let to = out_x + (out_y + y - in_y) * output.stride;
            if let Some(alpha) = alpha {
                for x in in_x..width {
                    let a = u32::from(alpha.data()[in_x + y * alpha.stride + x]);
                    let src = u32::from(input.data()[from + x]);
                    let dst = u32::from(output.data()[to + x]);
                    output.data_mut()[to + x] = ((src * a + dst * (255 - a)) / 255) as u8;
                }
            } else {
                output.data_mut()[to..to + width]
                    .copy_from_slice(&input.data()[from..from + width]);
            }
        }
    }
}
