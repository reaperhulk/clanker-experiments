// SPDX-License-Identifier: LGPL-3.0-or-later
//! AV1 sample decoding through rav1d (crates.io, without assembly).
use crate::{
    color::Nclx,
    context::{ContextError, Document},
    decoding::DecodeOptions,
    image::Image,
};
fn codec_error() -> ContextError {
    ContextError::new(7, 0, "Decoder plugin generated an error: Unspecified")
}
pub fn decode(
    document: &Document,
    id: u32,
    options: &DecodeOptions,
) -> Result<Image, ContextError> {
    if options.decoder_id.is_some_and(|id| id != b"rav1d") {
        return Err(ContextError::new(
            11,
            0,
            "Error while loading plugin: Unspecified: No decoder with that ID found.",
        ));
    }
    let info = &document.images[&id];
    let mut max_pixels = document.current_limits().max_image_size_pixels;
    if info.ispe.0 != 0
        && info.ispe.1 != 0
        && let Some(padded) =
            (u64::from(info.ispe.0) + 128).checked_mul(u64::from(info.ispe.1) + 128)
    {
        let maximum = padded.max(65536);
        if max_pixels == 0 || maximum < max_pixels {
            max_pixels = maximum;
        }
    }
    // The pinned native adapter leaves dav1d's strict-compliance setting at its
    // default; strict decoding only controls library warnings outside the codec.
    let mut decoder = crate::rav1d_api::Decoder::new(
        options.num_codec_threads,
        false,
        u32::try_from(max_pixels).unwrap_or(0),
    )
    .map_err(|_| {
        ContextError::new(
            7,
            0,
            "Decoder plugin generated an error: Unspecified: Success",
        )
    })?;
    let container = document.container()?;
    let mut data = crate::decoding::codec_configuration(&container, id, 4)?;
    data.extend_from_slice(&crate::decoding::decoder_payload(document, id)?);
    if data.is_empty() {
        return Err(ContextError::invalid(
            0,
            "Unspecified: Input with empty data extent.",
        ));
    }
    decoder.push(&data).map_err(|_| codec_error())?;
    let mut frame = None;
    for _ in 0..50 {
        frame = decoder.next_frame().map_err(|_| codec_error())?;
        if frame.is_some() {
            break;
        }
    }
    let frame=frame.ok_or_else(||ContextError::new(7,0,"Decoder plugin generated an error: Unspecified: Decoding the input data did not give a decompressed image."))?;
    let mut image = Image::new(
        frame.width,
        frame.height,
        if frame.layout == 0 { 2 } else { 0 },
        i32::from(frame.layout),
    )?;
    image.budget = Some(document.budget.clone());
    image.color.nclx = Some(Nclx {
        primaries: u16::from(frame.color_primaries),
        transfer: u16::from(frame.transfer_characteristics),
        matrix: u16::from(frame.matrix_coefficients),
        full_range: frame.full_range,
    });
    for channel in 0..if frame.layout == 0 { 1 } else { 3 } {
        let w = if channel != 0 && frame.layout != 3 {
            frame.width.div_ceil(2)
        } else {
            frame.width
        };
        let h = if channel != 0 && frame.layout == 1 {
            frame.height.div_ceil(2)
        } else {
            frame.height
        };
        image.add_plane(channel as i32, w, h, i32::from(frame.bit_depth))?;
        let plane = image.plane_mut(channel as i32).unwrap();
        let row = w as usize * usize::from(frame.bit_depth.div_ceil(8));
        let stride = plane.stride;
        for y in 0..h as usize {
            plane.data_mut()[y * stride..y * stride + row]
                .copy_from_slice(&frame.planes[channel][y * row..y * row + row]);
        }
    }
    Ok(image)
}
