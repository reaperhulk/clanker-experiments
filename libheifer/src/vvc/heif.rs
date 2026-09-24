// SPDX-License-Identifier: LGPL-3.0-or-later
// Decoder input and plugin semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! `vvc1` item decoding with the built-in VVC decoder.
//!
//! libheif's only VVC decoder is its vvdec plugin. This adapter reproduces
//! the observable behavior of libheif's VVC item path and that plugin: the
//! `vvcC` configuration NALs prepended with four-byte lengths, the coded
//! size check from the configuration SPS, the plugin's NAL splitting and
//! error codes, and its planar output in the stream's chroma format and bit
//! depth without colour information.
use crate::{
    context::{ContextError, Document},
    decoding::DecodeOptions,
    error_text,
    image::Image,
};

/// The built-in decoder's plugin id.
pub const DECODER_ID: &[u8] = b"libheifer-vvc";

fn error(code: i32, subcode: i32, message: &str) -> ContextError {
    ContextError::new(
        code,
        subcode,
        format!("{}: {message}", error_text::message(code, subcode)),
    )
}

fn plugin_error(subcode: i32, message: &str) -> ContextError {
    if message.is_empty() {
        ContextError::new(7, subcode, error_text::message(7, subcode))
    } else {
        error(7, subcode, message)
    }
}

/// libheif's `BitReader` over an SPS with emulation prevention removed.
struct Bits {
    data: Vec<u8>,
    position: usize,
}

impl Bits {
    fn new(nal: &[u8]) -> Self {
        let mut data = Vec::with_capacity(nal.len());
        let mut i = 0;
        while i < nal.len() {
            if i + 2 < nal.len() && nal[i] == 0 && nal[i + 1] == 0 && nal[i + 2] == 3 {
                data.extend_from_slice(&[0, 0]);
                i += 3;
            } else {
                data.push(nal[i]);
                i += 1;
            }
        }
        Self { data, position: 0 }
    }

    fn get(&mut self, n: usize) -> u32 {
        let mut value = 0;
        for _ in 0..n {
            let bit = self
                .data
                .get(self.position / 8)
                .map_or(0, |b| u32::from((b >> (7 - self.position % 8)) & 1));
            value = (value << 1) | bit;
            self.position += 1;
        }
        value
    }

    fn align(&mut self) {
        self.position = self.position.next_multiple_of(8);
    }

    fn uvlc(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.get(1) == 0 {
            zeros += 1;
            if zeros > 20 {
                return None;
            }
        }
        Some(if zeros == 0 {
            0
        } else {
            self.get(zeros) + (1u32 << zeros) - 1
        })
    }
}

/// libheif's `parse_sps_for_vvcC_configuration`, returning the coded size.
fn sps_coded_size(sps: &[u8]) -> Result<(u32, u32), ContextError> {
    let invalid_uvlc = || error(2, 2006, "Invalid variable length code in VVC SPS header");
    let mut r = Bits::new(sps);
    r.get(16 + 8);
    let sublayers = r.get(3) + 1;
    let chroma = r.get(2);
    r.get(2);
    if r.get(1) != 0 {
        r.get(7 + 1 + 8 + 1 + 1);
        if r.get(1) != 0 {
            return Err(error(
                4,
                3002,
                "VVC SPS with general_constraints_info is not supported yet",
            ));
        }
        r.align();
        let mut present = vec![false; sublayers as usize];
        for i in (0..sublayers.saturating_sub(1) as usize).rev() {
            present[i] = r.get(1) != 0;
        }
        r.align();
        for i in (0..sublayers.saturating_sub(1) as usize).rev() {
            if present[i] {
                r.get(8);
            }
        }
        let sub_profiles = r.get(8);
        for _ in 0..sub_profiles {
            r.get(32);
        }
    }
    r.get(1);
    if r.get(1) != 0 {
        r.get(1);
    }
    let width = r.uvlc().ok_or_else(invalid_uvlc)?;
    let height = r.uvlc().ok_or_else(invalid_uvlc)?;
    if width > 0xFFFF || height > 0xFFFF {
        return Err(error(
            9,
            2006,
            "SPS max picture width or height exceeds maximum (65535)",
        ));
    }
    if r.get(1) != 0 {
        let left = r.uvlc().ok_or_else(invalid_uvlc)?;
        let right = r.uvlc().ok_or_else(invalid_uvlc)?;
        let top = r.uvlc().ok_or_else(invalid_uvlc)?;
        let bottom = r.uvlc().ok_or_else(invalid_uvlc)?;
        let (sx, sy) = match chroma {
            1 => (2u64, 2u64),
            2 => (2, 1),
            _ => (1, 1),
        };
        if sx * (u64::from(left) + u64::from(right)) > u64::from(width)
            || sy * (u64::from(top) + u64::from(bottom)) > u64::from(height)
        {
            return Err(error(
                2,
                2006,
                "SPS conformance window exceeds image dimensions",
            ));
        }
    }
    if r.get(1) != 0 {
        return Err(error(
            4,
            3002,
            "VVC SPS with subpicture info is not supported yet",
        ));
    }
    let depth = r.uvlc().ok_or_else(invalid_uvlc)?;
    if depth > 0xFF - 8 {
        return Err(error(9, 0, "VCC bit depth out of range."));
    }
    Ok((width, height))
}

/// The plugin's `push_data2`: four-byte length-prefixed NAL units.
fn split_units(mut data: &[u8]) -> Result<Vec<&[u8]>, ContextError> {
    let mut out = Vec::new();
    while !data.is_empty() {
        if data.len() < 4 {
            return Err(plugin_error(100, ""));
        }
        let size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() - 4 < size {
            return Err(plugin_error(100, ""));
        }
        out.push(&data[4..4 + size]);
        data = &data[4 + size..];
    }
    Ok(out)
}

pub fn decode(
    document: &Document,
    id: u32,
    options: &DecodeOptions,
) -> Result<Image, ContextError> {
    let container = document.container()?;
    let config = container
        .property(id, *b"vvcC")
        .map_err(|_| ContextError::new(2, 141, error_text::message(2, 141)))?;
    let arrays = crate::vvc_config::DecoderConfiguration::parse(config)?.arrays;
    if options.decoder_id.is_some_and(|id| id != DECODER_ID) {
        return Err(ContextError::new(
            11,
            0,
            "Error while loading plugin: Unspecified: No decoder with that ID found.",
        ));
    }
    // libheif tightens the pixel limit to the ispe size padded by one CTU,
    // then rejects a configuration SPS coded size beyond it.
    let info = &document.images[&id];
    let mut limits = document.current_limits();
    if info.ispe.0 != 0
        && info.ispe.1 != 0
        && let Some(padded) =
            (u64::from(info.ispe.0) + 128).checked_mul(u64::from(info.ispe.1) + 128)
    {
        let maximum = padded.max(65536);
        if limits.max_image_size_pixels == 0 || maximum < limits.max_image_size_pixels {
            limits.max_image_size_pixels = maximum;
        }
    }
    if let Some(sps) = arrays
        .iter()
        .find(|(kind, _)| *kind == 15)
        .and_then(|(_, units)| units.first())
        .filter(|s| !s.is_empty())
    {
        let (width, height) = sps_coded_size(sps)?;
        limits.check_image_size(width, height)?;
    }
    let mut data = Vec::new();
    for (_, units) in &arrays {
        for nal in units {
            data.extend_from_slice(&[0, 0]);
            data.extend_from_slice(&(nal.len() as u16).to_be_bytes());
            data.extend_from_slice(nal);
        }
    }
    data.extend_from_slice(&crate::decoding::decoder_payload(document, id)?);
    if data.is_empty() {
        return Err(ContextError::invalid(
            0,
            "Unspecified: Input with empty data extent.",
        ));
    }
    let units = split_units(&data)?;
    let frame = match super::decode_nals(units) {
        Ok(frame) => frame,
        Err(super::Error::NoPicture) => {
            return Err(plugin_error(
                0,
                "Decoding the input data did not give a decompressed image.",
            ));
        }
        Err(_) => return Err(plugin_error(0, "vvdec decoding error")),
    };
    frame_image(&frame, document, limits.max_image_size_pixels)
}

/// The plugin's output image, with planes added under the pixel limit.
fn frame_image(
    frame: &super::Frame,
    document: &Document,
    maximum: u64,
) -> Result<Image, ContextError> {
    let (width, height) = (frame.width, frame.height);
    let chroma = frame.chroma_format as i32;
    let mut image = Image::new(width, height, if chroma == 0 { 2 } else { 0 }, chroma)?;
    image.budget = Some(document.budget.clone());
    let depth = frame.bit_depth;
    for (channel, (samples, w, h)) in frame.planes.iter().enumerate() {
        let (w32, h32) = (*w as u32, *h as u32);
        if maximum != 0 && h32 != 0 && maximum / u64::from(h32) < u64::from(w32) {
            return Err(ContextError::new(
                6,
                1000,
                format!(
                    "Memory allocation error: Security limit exceeded: Allocating an image of size {w32}x{h32} exceeds the security limit of {maximum} pixels"
                ),
            ));
        }
        image.add_plane(channel as i32, w32, h32, depth as i32)?;
        let plane = image
            .plane_mut(channel as i32)
            .ok_or_else(|| plugin_error(0, "vvdec decoding error"))?;
        let stride = plane.stride;
        for y in 0..*h {
            let row = &samples[y * w..(y + 1) * w];
            let out = plane.data_mut();
            if depth <= 8 {
                for (d, s) in out[y * stride..y * stride + w].iter_mut().zip(row) {
                    *d = *s as u8;
                }
            } else {
                for (d, s) in out[y * stride..y * stride + w * 2]
                    .chunks_exact_mut(2)
                    .zip(row)
                {
                    d.copy_from_slice(&s.to_ne_bytes());
                }
            }
        }
    }
    Ok(image)
}
