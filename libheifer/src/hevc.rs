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
    Image(crate::error::Error),
    Geometry,
}
impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DecodeError {}

pub fn decode_item(container: &Container<'_>, id: u32) -> Result<Image, DecodeError> {
    let nals = container.hevc_nals(id).map_err(DecodeError::Container)?;
    let mut decoder = rusty_h265::Decoder::new();
    for nal in nals {
        decoder.push_nal(&nal, None).map_err(DecodeError::Codec)?;
    }
    decoder.flush();
    let frame = decoder.next_frame().map_err(DecodeError::Codec)?;
    let picture = &frame.picture;
    let chroma = i32::from(picture.chroma_format_idc);
    if chroma > 3 {
        return Err(DecodeError::Geometry);
    }
    let width = u32::try_from(frame.width).map_err(|_| DecodeError::Geometry)?;
    let height = u32::try_from(frame.height).map_err(|_| DecodeError::Geometry)?;
    let mut image = Image::new(width, height, if chroma == 0 { 2 } else { 0 }, chroma)
        .map_err(DecodeError::Image)?;
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
