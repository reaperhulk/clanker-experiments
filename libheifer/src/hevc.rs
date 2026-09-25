// SPDX-License-Identifier: LGPL-3.0-or-later
//! Experimental direct-item decoding. No transform/grid/alpha orchestration yet.
use crate::{
    container::{Container, ParseError},
    image::Image,
};

#[derive(Debug)]
pub enum DecodeError {
    Container(ParseError),
    Codec(rusty_h265::Error),
    RangeExtensions(String),
    Image(crate::error::Error),
    Geometry,
    NoImage,
}
impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DecodeError {}

pub fn decode_item(container: &Container<'_>, id: u32) -> Result<Image, DecodeError> {
    decode_item_with_budget(container, id, None)
}
pub fn decode_item_with_budget(
    container: &Container<'_>,
    id: u32,
    budget: Option<std::sync::Arc<crate::security::Budget>>,
) -> Result<Image, DecodeError> {
    let nals = container.hevc_nals(id).map_err(DecodeError::Container)?;
    decode_nals(nals, budget)
}
pub fn decode_item_from_payload(
    container: &Container<'_>,
    id: u32,
    payload: &[u8],
    budget: Option<std::sync::Arc<crate::security::Budget>>,
) -> Result<Image, DecodeError> {
    let nals = container
        .hevc_nals_from_payload(id, payload)
        .map_err(DecodeError::Container)?;
    decode_nals(nals, budget)
}
fn decode_nals(
    nals: Vec<Vec<u8>>,
    budget: Option<std::sync::Arc<crate::security::Budget>>,
) -> Result<Image, DecodeError> {
    let mut nclx = crate::color::Nclx {
        primaries: 2,
        transfer: 2,
        matrix: 2,
        full_range: false,
    };
    let mut sequence_sets = [false; 16];
    let range_extensions = needs_range_extensions(&nals);
    let mut decoder = rusty_h265::Decoder::new();
    for nal in &nals {
        if nal.len() >= 2 && (nal[0] >> 1) & 63 == 33 {
            let rbsp = rusty_h265::nal::unescape(&nal[2..]);
            let sps = rusty_h265::ps::parse_sps(&rbsp.data).map_err(DecodeError::Codec)?;
            if let Some(present) = sequence_sets.get_mut(usize::from(sps.id)) {
                *present = true;
            }
            if let Some(vui) = sps.vui {
                // rusty_h265 leaves an absent colour description as zeros;
                // primaries and transfer 0 are reserved, so all zeros means
                // "not present" and the fields stay unspecified (2), as
                // libde265 reports them.
                let absent = (
                    vui.colour_primaries,
                    vui.transfer_characteristics,
                    vui.matrix_coeffs,
                ) == (0, 0, 0);
                nclx = crate::color::Nclx {
                    primaries: if absent {
                        2
                    } else {
                        vui.colour_primaries.into()
                    },
                    transfer: if absent {
                        2
                    } else {
                        vui.transfer_characteristics.into()
                    },
                    matrix: if absent { 2 } else { vui.matrix_coeffs.into() },
                    full_range: vui.video_full_range_flag,
                };
            }
        }
        if nal.len() >= 2 && (nal[0] >> 1) & 63 == 34 {
            let rbsp = rusty_h265::nal::unescape(&nal[2..]);
            let pps = rusty_h265::ps::parse_pps(&rbsp.data).map_err(DecodeError::Codec)?;
            // libde265 validates the SPS reference when the PPS arrives; an
            // unknown reference discards that PPS instead of deferring it.
            if !sequence_sets
                .get(usize::from(pps.sps_id))
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
        }
        if range_extensions {
            continue;
        }
        // libde265 discards slices whose parameter sets have not arrived. Keep
        // accepting later NALs so a subsequent complete picture can be decoded.
        match decoder.push_nal(nal, None) {
            Ok(()) => {}
            Err(rusty_h265::Error::InvalidData(message))
                if message.starts_with("slice refers to unknown PPS ") => {}
            Err(error) => return Err(DecodeError::Codec(error)),
        }
    }
    if range_extensions {
        return decode_range_extensions(&nals, nclx, budget);
    }
    decoder.flush();
    let frame = decoder.next_frame().map_err(|error| match error {
        rusty_h265::Error::Again | rusty_h265::Error::Eof => DecodeError::NoImage,
        error => DecodeError::Codec(error),
    })?;
    let picture = &frame.picture;
    let chroma = i32::from(picture.chroma_format_idc);
    if chroma > 3 {
        return Err(DecodeError::Geometry);
    }
    let width = u32::try_from(frame.width).map_err(|_| DecodeError::Geometry)?;
    let height = u32::try_from(frame.height).map_err(|_| DecodeError::Geometry)?;
    let mut image = Image::new(width, height, if chroma == 0 { 2 } else { 0 }, chroma)
        .map_err(DecodeError::Image)?;
    image.budget = budget;
    image.color.nclx = Some(nclx);
    let (left, top, crop_width, crop_height) = picture.crop;
    if crop_width != frame.width || crop_height != frame.height {
        return Err(DecodeError::Geometry);
    }
    for channel in 0..if chroma == 0 { 1 } else { 3 } {
        let (sx, sy) = if channel == 0 {
            (1, 1)
        } else {
            match chroma {
                1 => (2, 2),
                2 => (2, 1),
                _ => (1, 1),
            }
        };
        let (w, h) = (frame.width.div_ceil(sx), frame.height.div_ceil(sy));
        let (x0, y0) = (left / sx, top / sy);
        let bits = if channel == 0 {
            picture.bit_depth_luma
        } else {
            picture.bit_depth_chroma
        };
        let input = &picture.planes[channel];
        if x0.checked_add(w).is_none_or(|end| end > input.width)
            || y0.checked_add(h).is_none_or(|end| end > input.height)
        {
            return Err(DecodeError::Geometry);
        }
        image
            .add_plane(channel as i32, w as u32, h as u32, i32::from(bits))
            .map_err(DecodeError::Image)?;
        let output = image
            .plane_mut(channel as i32)
            .ok_or(DecodeError::Geometry)?;
        let stride = output.stride;
        for y in 0..h {
            let start = (y0 + y)
                .checked_mul(input.stride)
                .and_then(|v| v.checked_add(x0))
                .ok_or(DecodeError::Geometry)?;
            let row = input
                .data
                .get(start..start + w)
                .ok_or(DecodeError::Geometry)?;
            if bits <= 8 {
                for (dst, src) in output.data_mut()[y * stride..y * stride + w]
                    .iter_mut()
                    .zip(row)
                {
                    *dst = *src as u8;
                }
            } else {
                for (dst, src) in output.data_mut()[y * stride..y * stride + w * 2]
                    .chunks_exact_mut(2)
                    .zip(row)
                {
                    dst.copy_from_slice(&src.to_ne_bytes());
                }
            }
        }
    }
    Ok(image)
}

/// rusty_h265 decodes Main, Main 10 and Main Still Picture (and, patched,
/// monochrome) streams without extension tools. Streams using the format
/// range extensions (4:2:2, 4:4:4, bit depths above 10, or range extension
/// tools) are decoded by oxideav-h265 instead; pictures above level 6.2 stay
/// with rusty_h265, which rejects them.
fn needs_range_extensions(nals: &[Vec<u8>]) -> bool {
    let mut range = false;
    for nal in nals {
        if nal.len() < 2 {
            continue;
        }
        let rbsp = || rusty_h265::nal::unescape(&nal[2..]);
        match (nal[0] >> 1) & 63 {
            33 => {
                let Ok(sps) = rusty_h265::ps::parse_sps(&rbsp().data) else {
                    return false;
                };
                if sps.width > 16888
                    || sps.height > 16888
                    || u64::from(sps.width) * u64::from(sps.height) > 35_651_584
                {
                    return false;
                }
                range |= sps.chroma_format_idc > 1
                    || sps.bit_depth_luma > 10
                    || sps.bit_depth_chroma > 10
                    || sps.range_extension
                    || (sps.ptl.profile() == 4 && sps.chroma_format_idc != 0);
            }
            34 => {
                if let Ok(pps) = rusty_h265::ps::parse_pps(&rbsp().data) {
                    range |= pps.range_extension;
                }
            }
            _ => {}
        }
    }
    range
}

fn decode_range_extensions(
    nals: &[Vec<u8>],
    nclx: crate::color::Nclx,
    budget: Option<std::sync::Arc<crate::security::Budget>>,
) -> Result<Image, DecodeError> {
    use oxideav_h265::picture::Plane;
    let mut stream = Vec::new();
    for nal in nals {
        stream.extend_from_slice(&[0, 0, 0, 1]);
        stream.extend_from_slice(nal);
    }
    let frames = oxideav_h265::sequence::decode_annexb_sequence(&stream)
        .map_err(|error| DecodeError::RangeExtensions(error.to_string()))?;
    let frame = frames
        .iter()
        .find(|frame| frame.output)
        .ok_or(DecodeError::NoImage)?;
    let picture = frame.output_picture();
    let chroma = i32::from(picture.chroma_array_type());
    let width = u32::try_from(picture.width_luma()).map_err(|_| DecodeError::Geometry)?;
    let height = u32::try_from(picture.height_luma()).map_err(|_| DecodeError::Geometry)?;
    let mut image = Image::new(width, height, if chroma == 0 { 2 } else { 0 }, chroma)
        .map_err(DecodeError::Image)?;
    image.budget = budget;
    image.color.nclx = Some(nclx);
    let planes: &[Plane] = if chroma == 0 {
        &[Plane::Luma]
    } else {
        &[Plane::Luma, Plane::Cb, Plane::Cr]
    };
    for (channel, &plane) in planes.iter().enumerate() {
        let (w, h) = picture.plane_dims(plane);
        let bits = picture.bit_depth(plane);
        image
            .add_plane(channel as i32, w as u32, h as u32, i32::from(bits))
            .map_err(DecodeError::Image)?;
        let output = image
            .plane_mut(channel as i32)
            .ok_or(DecodeError::Geometry)?;
        let stride = output.stride;
        let data = output.data_mut();
        for (y, row) in picture
            .plane(plane)
            .chunks_exact(w.max(1))
            .take(h)
            .enumerate()
        {
            let line = &mut data[y * stride..];
            if bits <= 8 {
                for (dst, &src) in line[..w].iter_mut().zip(row) {
                    *dst = src as u8;
                }
            } else {
                for (dst, &src) in line[..w * 2].chunks_exact_mut(2).zip(row) {
                    dst.copy_from_slice(&(src as u16).to_ne_bytes());
                }
            }
        }
    }
    Ok(image)
}
