// SPDX-License-Identifier: LGPL-3.0-or-later
// Callback sequencing adapted from libheif, Copyright Dirk Farin and contributors.
use crate::{
    encoder::Encoder,
    encoding_options::EncodingOptions,
    plugin_registry::{EncoderSource, field},
};
use libheifer::{
    av1_config::Configuration,
    color::Nclx,
    context::ContextError,
    encoding::{Options, property},
    image::Image,
    properties::Property,
};
use std::ptr;
use std::sync::Arc;

fn alpha_encoder(source: &Encoder) -> Result<Encoder, ContextError> {
    let EncoderSource::External(p) = source.source else {
        unreachable!()
    };
    let mut out = Encoder {
        source: source.source,
        state: ptr::null_mut(),
    };
    let allocate = field!(p, new_encoder)
        .ok_or_else(|| ContextError::new(5, 2001, "Usage error: NULL argument received"))?;
    let err = unsafe { allocate(&mut out.state) };
    if err.code != 0 {
        return Err(crate::plugin_decoding::callback_error(err, false));
    }
    for (get, set) in [
        (
            field!(p, get_parameter_quality),
            field!(p, set_parameter_quality),
        ),
        (
            field!(p, get_parameter_lossless),
            field!(p, set_parameter_lossless),
        ),
        (
            field!(p, get_parameter_logging_level),
            field!(p, set_parameter_logging_level),
        ),
    ] {
        if let (Some(get), Some(set)) = (get, set) {
            let mut value = 0;
            unsafe {
                get(source.state, &mut value);
                set(out.state, value);
            }
        }
    }
    if let Some(list) = field!(p, list_parameters) {
        let mut parameters = unsafe { list(source.state) };
        while !parameters.is_null() && !unsafe { parameters.read() }.is_null() {
            let parameter = unsafe { parameters.read() };
            let name = unsafe { ptr::addr_of!((*parameter).name).read() };
            let kind = unsafe { ptr::addr_of!((*parameter).kind).read() };
            match kind {
                1 | 2 => {
                    let (get, set) = if kind == 1 {
                        (
                            field!(p, get_parameter_integer),
                            field!(p, set_parameter_integer),
                        )
                    } else {
                        (
                            field!(p, get_parameter_boolean),
                            field!(p, set_parameter_boolean),
                        )
                    };
                    if let (Some(get), Some(set)) = (get, set) {
                        let mut value = 0;
                        if unsafe { get(source.state, name, &mut value) }.code == 0 {
                            unsafe {
                                set(out.state, name, value);
                            }
                        }
                    }
                }
                3 => {
                    if let (Some(get), Some(set)) = (
                        field!(p, get_parameter_string),
                        field!(p, set_parameter_string),
                    ) {
                        let mut value = [0; 256];
                        if unsafe { get(source.state, name, value.as_mut_ptr(), 256) }.code == 0 {
                            unsafe {
                                set(out.state, name, value.as_ptr());
                            }
                        }
                    }
                }
                _ => {}
            }
            parameters = unsafe { parameters.add(1) };
        }
    }
    Ok(out)
}

pub(super) fn encode_image(
    context: &crate::context::SharedContext,
    image: &Image,
    encoder: &Encoder,
    copied: &EncodingOptions,
    options: &Options,
    input_class: i32,
) -> Result<Arc<libheifer::context::ImageInfo>, ContextError> {
    let encoded = encode_av1(image, encoder, copied, options, input_class)?;
    let output = crate::context::lock(context).insert_coded(
        &encoded.image,
        *b"av01",
        encoded.data,
        encoded.properties,
        options,
        encoded.size,
    )?;
    if copied.save_alpha_channel != 0
        && (encoded.image.plane(6).is_some() || matches!(encoded.image.chroma, 11 | 13 | 15))
    {
        let color = &encoded.image;
        let mut alpha = Image::new(color.width, color.height, 2, 0)?;
        alpha.budget = Some(crate::context::lock(context).budget.clone());
        if let Some(plane) = color.plane(6) {
            alpha.add_plane(0, plane.width, plane.height, i32::from(plane.bit_depth))?;
            let output = alpha.plane_mut(0).unwrap();
            let row = plane.width as usize * plane.bytes_per_pixel;
            let stride = output.stride;
            for y in 0..plane.height as usize {
                output.data_mut()[y * stride..y * stride + row]
                    .copy_from_slice(&plane.data()[y * plane.stride..y * plane.stride + row]);
            }
        } else if color.chroma == 11 {
            let source = color.plane(10).ok_or(libheifer::error::Error::NULL)?;
            if source.bit_depth != 8 {
                return Err(ContextError::new(
                    4,
                    0,
                    "Unsupported feature: Unspecified: extract_alpha_from_RGBA only supports 8-bit interleaved RGBA",
                ));
            }
            alpha.add_plane(0, color.width, color.height, 8)?;
            let output = alpha.plane_mut(0).unwrap();
            let stride = output.stride;
            for y in 0..color.height as usize {
                for x in 0..color.width as usize {
                    output.data_mut()[y * stride + x] =
                        source.data()[y * source.stride + x * 4 + 3];
                }
            }
        }
        alpha.color.nclx = Some(Nclx {
            primaries: 2,
            transfer: 2,
            matrix: 2,
            full_range: true,
        });
        let alpha_encoder = alpha_encoder(encoder)?;
        let mut encoded = encode_av1(&alpha, &alpha_encoder, copied, options, 2)?;
        encoded.image.color.raw = None;
        let alpha_options = Options {
            nclx: None,
            ..options.clone()
        };
        let mut state = crate::context::lock(context);
        let alpha = state.insert_coded(
            &encoded.image,
            *b"av01",
            encoded.data,
            encoded.properties,
            &alpha_options,
            encoded.size,
        )?;
        state.attach_encoded_alpha(&output, &alpha, image.premultiplied_alpha)?;
    }
    Ok(output)
}

pub(super) struct Encoded {
    pub image: Image,
    pub data: Vec<u8>,
    pub properties: Vec<(Property, bool)>,
    pub size: (u32, u32),
}

pub(super) fn encode_av1(
    image: &Image,
    encoder: &Encoder,
    copied: &EncodingOptions,
    options: &Options,
    input_class: i32,
) -> Result<Encoded, ContextError> {
    let EncoderSource::External(p) = encoder.source else {
        unreachable!()
    };
    let version = field!(p, plugin_api_version);
    let (mut cs, mut ch) = (image.colorspace, image.chroma);
    if version >= 2 {
        if let Some(query) = field!(p, query_input_colorspace2) {
            unsafe { query(encoder.state, &mut cs, &mut ch) };
        }
    } else if let Some(query) = field!(p, query_input_colorspace) {
        unsafe { query(&mut cs, &mut ch) };
    }
    let fallback = Nclx {
        primaries: 1,
        transfer: 13,
        matrix: 6,
        full_range: true,
    };
    let mut target = options.nclx.or(image.color.nclx).unwrap_or(fallback);
    if target.primaries == 2 {
        target.primaries = 1;
    }
    if target.transfer == 2 {
        target.transfer = 13;
    }
    if target.matrix == 2 {
        target.matrix = 6;
    }
    let source = image.color.nclx.unwrap_or(fallback);
    let profile_matches = cs != 0
        || options.nclx.is_none_or(|n| {
            (source.primaries, source.matrix, source.full_range)
                == (n.primaries, n.matrix, n.full_range)
        });
    let mut image = image.rotate(0)?;
    if cs != image.colorspace || ch != image.chroma || !profile_matches {
        image = libheifer::conversion::convert(
            image,
            cs,
            ch,
            target,
            0,
            copied.color_conversion_options,
        )?;
    }
    let mut config = Configuration::from_image(&image);
    let encode = field!(p, encode_image)
        .ok_or_else(|| ContextError::new(8, 0, "Encoder plugin generated an error: Unspecified"))?;
    let err = unsafe { encode(encoder.state, &image, input_class) };
    if err.code != 0 {
        return Err(crate::plugin_decoding::callback_error(err, false));
    }
    let get = field!(p, get_compressed_data)
        .ok_or_else(|| ContextError::new(8, 0, "Encoder plugin generated an error: Unspecified"))?;
    let mut data = Vec::new();
    loop {
        let mut packet = ptr::null_mut();
        let mut size = 0;
        let err = unsafe { get(encoder.state, &mut packet, &mut size, ptr::null_mut()) };
        if err.code != 0 {
            return Err(crate::plugin_decoding::callback_error(err, false));
        }
        if packet.is_null() {
            break;
        }
        let size = usize::try_from(size).map_err(|_| {
            ContextError::new(
                8,
                2006,
                "Encoder plugin generated an error: Invalid parameter value: Negative packet size",
            )
        })?;
        let packet = unsafe { std::slice::from_raw_parts(packet, size) };
        config.update(packet);
        data.try_reserve(size)
            .map_err(|_| libheifer::error::Error::ALLOCATION)?;
        data.extend_from_slice(packet);
    }
    let mut size = (image.width, image.height);
    if version >= 3
        && let Some(query) = field!(p, query_encoded_size)
    {
        unsafe {
            query(
                encoder.state,
                image.width,
                image.height,
                &mut size.0,
                &mut size.1,
            )
        };
    }
    Ok(Encoded {
        image,
        data,
        properties: vec![(property(*b"av1C", config.bytes()), true)],
        size,
    })
}
