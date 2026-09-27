// SPDX-License-Identifier: LGPL-3.0-or-later
//! JPEG sample decoding using the reviewed, scalar Rust jpeg-decoder backend.
use crate::{
    context::{ContextError, Document},
    decoding::DecodeOptions,
    image::Image,
};
use jpeg_decoder::{ColorTransform, Decoder, PixelFormat};

pub(crate) fn codec_error(error: impl std::fmt::Display) -> ContextError {
    ContextError::new(
        7,
        0,
        format!("Decoder plugin generated an error: Unspecified: {error}"),
    )
}

pub fn decode(
    document: &Document,
    id: u32,
    options: &DecodeOptions,
) -> Result<Image, ContextError> {
    if options.decoder_id.is_some_and(|id| id != b"jpeg-decoder") {
        return Err(ContextError::new(
            11,
            0,
            "Error while loading plugin: Unspecified: No decoder with that ID found.",
        ));
    }
    let container = document.container()?;
    let mut data = crate::decoding::codec_configuration(&container, id, 3)?;
    data.extend_from_slice(&crate::decoding::decoder_payload(document, id)?);
    if data.is_empty() {
        return Err(ContextError::invalid(
            0,
            "Unspecified: Input with empty data extent.",
        ));
    }
    let header = crate::jpeg_header::read(&data)?;
    let mut decoder = Decoder::new(PaddedInput { data: &data, at: 0 });
    decoder.read_info().map_err(|e| decoder_error(e, &data))?;
    let info = decoder
        .info()
        .ok_or_else(|| codec_error("JPEG datastream contains no image"))?;
    let width = u32::from(info.width);
    let height = u32::from(info.height);
    let pixels = u64::from(width) * u64::from(height);
    let limits = document.current_limits();
    let (ispe_w, ispe_h) = document.images[&id].ispe;
    let mut max_pixels = limits.max_image_size_pixels;
    if ispe_w != 0 && ispe_h != 0 {
        let padded = ((u64::from(ispe_w) + 16) * (u64::from(ispe_h) + 16)).max(65536);
        if max_pixels == 0 || padded < max_pixels {
            max_pixels = padded;
        }
    }
    if max_pixels > 0 && pixels > max_pixels {
        return Err(ContextError::new(
            6,
            1000,
            "Memory allocation error: Security limit exceeded: JPEG image exceeds maximum allowed image size",
        ));
    }
    let estimated = pixels * info.pixel_format.pixel_bytes() as u64 * 3;
    if limits.max_memory_block_size > 0 && estimated > limits.max_memory_block_size {
        return Err(ContextError::new(
            6,
            1000,
            "Memory allocation error: Security limit exceeded: JPEG image would require too much memory to decode",
        ));
    }
    header.validate_decode()?;
    let gray = info.pixel_format == PixelFormat::L8;
    if !gray && info.pixel_format != PixelFormat::RGB24 {
        return Err(codec_error("Unsupported color conversion request"));
    }
    // RGB's identity interleaver preserves the encoded Y, Cb and Cr components.
    // The None transform in upstream jpeg-decoder includes padded row samples.
    decoder.set_color_transform(ColorTransform::RGB);
    decoder.set_max_decoding_buffer_size(usize::try_from(estimated).unwrap_or(usize::MAX));
    let samples = decoder.decode().map_err(|e| decoder_error(e, &data))?;
    let mut image = Image::new(
        width,
        height,
        if gray { 2 } else { 0 },
        if gray { 0 } else { 1 },
    )?;
    image.budget = Some(document.budget.clone());
    let components = if gray { 1 } else { 3 };
    for channel in 0..components {
        let (w, h) = if channel == 0 {
            (width, height)
        } else {
            (width.div_ceil(2), height.div_ceil(2))
        };
        image.add_plane(channel as i32, w, h, 8)?;
        let plane = image.plane_mut(channel as i32).unwrap();
        let stride = plane.stride;
        for y in 0..h as usize {
            // The native adapter retains chroma from the first JPEG scanline
            // in each row pair and the first sample of each horizontal pair.
            let sy = if channel == 0 { y } else { y * 2 };
            for x in 0..w as usize {
                let sx = if channel == 0 { x } else { x * 2 };
                plane.data_mut()[y * stride + x] =
                    samples[(sy * width as usize + sx) * components + channel];
            }
        }
    }
    Ok(image)
}

// libjpeg's memory source supplies repeated synthetic EOI markers at EOF.
// Preserve that read behavior for truncated JPEG marker and entropy segments.
struct PaddedInput<'a> {
    data: &'a [u8],
    at: usize,
}
impl std::io::Read for PaddedInput<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        for value in out.iter_mut() {
            *value = self.data.get(self.at).copied().unwrap_or_else(|| {
                if (self.at - self.data.len()).is_multiple_of(2) {
                    255
                } else {
                    217
                }
            });
            self.at += 1;
        }
        Ok(out.len())
    }
}
fn decoder_error(error: jpeg_decoder::Error, data: &[u8]) -> ContextError {
    use jpeg_decoder::{Error, UnsupportedFeature};
    let description = match error {
        Error::Format(ref text)
            if text.starts_with("Bogus ")
                || text.starts_with("Invalid component ID ")
                || text.starts_with("Unsupported marker type ")
                || text.starts_with("Unsupported JPEG process: ")
                || text.starts_with("Invalid JPEG file structure: two SOF")
                || text.starts_with("Invalid progressive/lossless parameters ")
                || text.starts_with("Huffman table ")
                || text.starts_with("Quantization table ") =>
        {
            text.clone()
        }
        Error::Format(ref text) if text == "first two bytes are not an SOI marker" => format!(
            "Not a JPEG file: starts with 0x{:02x} 0x{:02x}",
            data.first().copied().unwrap_or(255),
            data.get(1).copied().unwrap_or(255)
        ),
        Error::Format(ref text) if text == "end of image encountered before frame" => {
            "JPEG datastream contains no image".into()
        }
        Error::Format(ref text) if text == "not all components have data" => {
            "Invalid JPEG file structure: missing SOS marker".into()
        }
        Error::Format(ref text) if text.contains("precision") => {
            let precision = text
                .split_whitespace()
                .find_map(|s| s.parse::<u8>().ok())
                .unwrap_or(0);
            format!("Unsupported JPEG data precision {precision}")
        }
        Error::Unsupported(UnsupportedFeature::SamplePrecision(precision)) => {
            format!("Unsupported JPEG data precision {precision}")
        }
        Error::Unsupported(UnsupportedFeature::DNL) => {
            "Empty JPEG image (DNL not supported)".into()
        }
        Error::Format(ref text)
            if text == "zero component count in frame header"
                || text == "zero width in frame header" =>
        {
            "Empty JPEG image (DNL not supported)".into()
        }
        Error::Format(ref text) if text.contains("invalid length") => "Bogus marker length".into(),
        Error::Format(ref text) if text.contains("sampling factor") => {
            "Bogus sampling factors".into()
        }
        _ => error.to_string(),
    };
    codec_error(description)
}
