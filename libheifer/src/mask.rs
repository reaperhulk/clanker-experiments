// SPDX-License-Identifier: LGPL-3.0-or-later
// Mask decoding adapted from libheif, Copyright Dirk Farin and contributors.
use crate::{container::Container, context::ContextError, image::Image};

pub fn decode(
    container: &Container<'_>,
    id: u32,
    budget: Option<std::sync::Arc<crate::security::Budget>>,
) -> Result<Image, ContextError> {
    // Payload errors precede configuration errors in the reference decoder.
    let data = container.payload(id)?;
    let (size, config) = (container.dimensions(id), container.property(id, *b"mskC"));
    let (Ok((width, height)), Ok(config)) = (size, config) else {
        return Err(ContextError::new(
            4,
            3002,
            "Unsupported feature: Unsupported data version: Missing required box for mask codec",
        ));
    };
    let depth = config.get(4).copied().unwrap_or(0);
    if depth != 8 && depth != 16 {
        return Err(ContextError::new(
            4,
            3002,
            "Unsupported feature: Unsupported data version: Unsupported bit depth for mask item",
        ));
    }
    let row_bytes = width as usize * usize::from(depth / 8);
    if row_bytes == 0 || data.len() / row_bytes < height as usize {
        return Err(ContextError::invalid(
            0,
            "Unspecified: Mask image data is too short",
        ));
    }
    container.limits.check_image_size(width, height)?;
    let mut image = Image::new(width, height, 2, 0)
        .map_err(|e| ContextError::new(e.code, e.subcode, e.message.to_string_lossy()))?;
    image.budget = budget;
    image
        .add_plane(0, width, height, depth.into())
        .map_err(|e| ContextError::new(e.code, e.subcode, e.message.to_string_lossy()))?;
    let plane = image.plane_mut(0).unwrap();
    for y in 0..height as usize {
        let target = y * plane.stride;
        plane.data_mut()[target..target + row_bytes]
            .copy_from_slice(&data[y * row_bytes..(y + 1) * row_bytes]);
    }
    Ok(image)
}
