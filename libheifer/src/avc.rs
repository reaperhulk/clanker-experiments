// SPDX-License-Identifier: LGPL-3.0-or-later
//! AVC (H.264) sample decoding through the vendored Rust-only rusty_h264 decoder.
//!
//! libheif's only AVC decoder is its OpenH264 plugin. This adapter reproduces
//! that plugin's observable behavior: the length-prefixed to Annex B conversion
//! (including its start-code emulation handling), OpenH264's acceptance rules
//! for sequence parameter sets and CAVLC levels, 4:2:0 8-bit output with flat
//! chroma for monochrome streams, and the plugin's error codes and messages.
use crate::{
    context::{ContextError, Document},
    decoding::DecodeOptions,
    image::Image,
};

fn plugin_error(subcode: i32, message: &str) -> ContextError {
    ContextError::new(
        7,
        subcode,
        format!(
            "Decoder plugin generated an error: {}: {message}",
            crate::error_text::subcode_text(subcode)
        ),
    )
}

fn decoder_error() -> ContextError {
    plugin_error(0, "OpenH264 decoder error")
}

/// The plugin's conversion of four-byte length-prefixed NAL units to an Annex B
/// stream. It reproduces the native loop exactly, including its emulation
/// "check" that only inspects the first bytes of each unit.
fn annex_b(input: &[u8]) -> Result<Vec<u8>, ContextError> {
    let eof = || plugin_error(100, "Insufficient input data");
    let mut out = Vec::with_capacity(input.len() + input.len() / 8);
    let mut idx = 0usize;
    while idx < input.len() {
        // `indata.size() - 4 < idx` with unsigned arithmetic; push rejects < 4 bytes.
        if input.len() - 4 < idx {
            return Err(eof());
        }
        let mut size =
            u32::from_be_bytes([input[idx], input[idx + 1], input[idx + 2], input[idx + 3]])
                as usize;
        idx += 4;
        if input.len() < size || input.len() - size < idx {
            return Err(eof());
        }
        out.extend_from_slice(&[0, 0, 1]);
        let mut check = true;
        while check && size >= 3 {
            check = false;
            // The native loop compares the unit's first three bytes on every
            // iteration; a match inserts an emulation prevention byte after two
            // leading zero bytes and restarts. Otherwise the loop runs out.
            // The inner `for (i = 0; i < size - 3; ...)` runs only when size > 3.
            if size > 3 && input[idx] == 0 && input[idx + 1] == 0 && input[idx + 2] <= 3 {
                out.extend_from_slice(&[0, 0, 3]);
                idx += 2;
                size -= 2;
                check = true;
            }
        }
        if size == 0 {
            return Err(plugin_error(2006, "Invalid input data"));
        }
        out.extend_from_slice(&input[idx..idx + size]);
        idx += size;
    }
    Ok(out)
}

/// OpenH264 `ParseSps` returns success without storing a parameter set whose
/// profile it does not decode; slices referring to it then fail. Remove such
/// units before decoding so the Rust decoder observes the same state.
fn openh264_units(stream: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(stream.len());
    let mut units = Vec::new();
    let mut at = 0;
    while at + 3 <= stream.len() {
        if stream[at..at + 3] == [0, 0, 1] {
            units.push(at);
            at += 3;
        } else {
            at += 1;
        }
    }
    for (n, &start) in units.iter().enumerate() {
        let end = units.get(n + 1).copied().unwrap_or(stream.len());
        let unit = &stream[start..end];
        let ignored = unit.get(3).is_some_and(|h| h & 31 == 7)
            && unit
                .get(4)
                .is_some_and(|profile| !matches!(profile, 66 | 77 | 83 | 86 | 88 | 100));
        if !ignored {
            out.extend_from_slice(unit);
        }
    }
    out
}

pub fn decode(
    document: &Document,
    id: u32,
    options: &DecodeOptions,
) -> Result<Image, ContextError> {
    if options.decoder_id.is_some_and(|id| id != b"rusty_h264") {
        return Err(ContextError::new(
            11,
            0,
            "Error while loading plugin: Unspecified: No decoder with that ID found.",
        ));
    }
    let container = document.container()?;
    let config = container
        .property(id, *b"avcC")
        .ok()
        .and_then(crate::avc_config::parse_configuration)
        .unwrap_or_default();
    // libheif tightens the pixel limit to the ispe size padded by one
    // macroblock, then rejects coded SPS sizes beyond it before decoding.
    let info = &document.images[&id];
    let mut limits = document.current_limits();
    if info.ispe.0 != 0
        && info.ispe.1 != 0
        && let Some(padded) = (u64::from(info.ispe.0) + 16).checked_mul(u64::from(info.ispe.1) + 16)
    {
        let maximum = padded.max(65536);
        if limits.max_image_size_pixels == 0 || maximum < limits.max_image_size_pixels {
            limits.max_image_size_pixels = maximum;
        }
    }
    if let Some((width, height)) = crate::avc_config::coded_size(&config)? {
        limits.check_image_size(width, height)?;
    }
    let mut data = config.header_nals();
    data.extend_from_slice(&crate::decoding::decoder_payload(document, id)?);
    if data.is_empty() {
        return Err(ContextError::invalid(
            0,
            "Unspecified: Input with empty data extent.",
        ));
    }
    if data.len() < 4 {
        return Err(plugin_error(2006, "Invalid input data"));
    }
    let stream = openh264_units(&annex_b(&data)?);
    let mut decoder = rusty_h264_decoder::Decoder::new();
    let frame = decoder
        .decode(&stream)
        .map_err(|_| decoder_error())?
        .ok_or_else(|| {
            plugin_error(
                0,
                "Decoding the input data did not give a decompressed image.",
            )
        })?;
    let (width, height) = (frame.width as u32, frame.height as u32);
    // heif_image_add_plane_safe with the tightened limits; the luma plane is first.
    let maximum = limits.max_image_size_pixels;
    if maximum != 0 && height != 0 && maximum / u64::from(height) < u64::from(width) {
        return Err(ContextError::new(
            6,
            1000,
            format!(
                "Memory allocation error: Security limit exceeded: Allocating an image of size {width}x{height} exceeds the security limit of {maximum} pixels"
            ),
        ));
    }
    let mut image = Image::new(width, height, 0, 1)?;
    image.budget = Some(document.budget.clone());
    let planes: [(&[u8], u32, u32); 3] = [
        (&frame.y, width, height),
        (&frame.u, width.div_ceil(2), height.div_ceil(2)),
        (&frame.v, width.div_ceil(2), height.div_ceil(2)),
    ];
    for (channel, (samples, w, h)) in planes.into_iter().enumerate() {
        image.add_plane(channel as i32, w, h, 8)?;
        let plane = image.plane_mut(channel as i32).unwrap();
        let stride = plane.stride;
        let row = w as usize;
        for y in 0..h as usize {
            plane.data_mut()[y * stride..y * stride + row]
                .copy_from_slice(&samples[y * row..y * row + row]);
        }
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annex_b_matches_native_emulation_quirk() {
        // Leading 00 00 0x bytes gain an emulation byte; later ones do not.
        let input = [0, 0, 0, 6, 0, 0, 1, 0x65, 0, 0];
        assert_eq!(annex_b(&input).unwrap(), [0, 0, 1, 0, 0, 3, 1, 0x65, 0, 0]);
        assert_eq!(annex_b(&[0, 0, 0, 9]).unwrap_err().subcode, 100);
        assert_eq!(annex_b(&[0, 0, 0, 0]).unwrap_err().subcode, 2006);
    }
}
