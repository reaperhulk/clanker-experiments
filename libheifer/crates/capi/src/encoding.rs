// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, HeifHandle, lock, report},
    encoder::Encoder,
    encoding_options::{EncodingOptions, heif_encoding_options_copy},
};
use libheifer::{
    color::Nclx, context::ContextError, encoding::Options, error::Error, image::Image,
};
use std::ptr;
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_encode_image(
    ctx: *mut HeifContext,
    image: *const Image,
    encoder: *mut Encoder,
    options: *const EncodingOptions,
    out: *mut *mut HeifHandle,
) -> HeifError {
    let Some(encoder) = (unsafe { encoder.as_ref() }) else {
        return Error::NULL.into();
    };
    if !out.is_null() {
        unsafe { out.write(ptr::null_mut()) };
    }
    let (Some(ctx), Some(image)) = (unsafe { ctx.as_ref() }, unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut copied = EncodingOptions::default();
    unsafe { heif_encoding_options_copy(&mut copied, options) };
    let nclx = if let Some(n) = unsafe { copied.output_nclx_profile.as_ref() } {
        Some(Nclx {
            primaries: n.color_primaries as u16,
            transfer: n.transfer_characteristics as u16,
            matrix: n.matrix_coefficients as u16,
            full_range: n.full_range_flag != 0,
        })
    } else if !options.is_null() {
        image.color.nclx
    } else {
        None
    };
    let opts = Options {
        orientation: copied.image_orientation,
        nclx,
        two_profiles: copied.save_two_colr_boxes_when_ICC_and_nclx_available != 0,
        no_nclx: copied.macOS_compatibility_workaround_no_nclx_profile != 0,
    };
    let external = if matches!(
        encoder.source,
        crate::plugin_registry::EncoderSource::External(_)
    ) && matches!(encoder.source.format(), 1 | 4)
    {
        Some(crate::plugin_encoding::encode_image(
            &ctx.shared,
            image,
            encoder,
            &copied,
            &opts,
            1,
        ))
    } else {
        None
    };
    let mut state = lock(&ctx.shared);
    let result = if let Some(encoded) = external {
        encoded
    } else {
        match encoder.source.format() {
            9 => state.encode_mask(image, &opts),
            8 => state.encode_uncompressed(
                image,
                &opts,
                if copied.unci_parameters.is_null() {
                    0
                } else {
                    unsafe { ptr::addr_of!((*copied.unci_parameters).compression).read() }
                },
            ),
            _ => Err(ContextError::new(
                4,
                6003,
                "Unsupported feature: Support for this compression format has not been built in",
            )),
        }
    };
    match result {
        Err(error) => report(&mut state, error),
        Ok(image) => {
            if state.document.as_ref().is_none_or(|doc| doc.primary == 0) {
                state.set_primary(image.clone());
            }
            if !out.is_null() {
                unsafe {
                    out.write(Box::into_raw(Box::new(HeifHandle {
                        shared: ctx.shared.clone(),
                        images: state.document.as_ref().unwrap().images.clone(),
                        id: image.id,
                    })))
                }
            };
            SUCCESS
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_primary_image(
    ctx: *mut HeifContext,
    image: *mut HeifHandle,
) -> HeifError {
    let (Some(ctx), Some(image)) = (unsafe { ctx.as_ref() }, unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    lock(&ctx.shared).set_primary(image.images[&image.id].clone());
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_overlay_image(
    ctx: *mut HeifContext,
    width: u32,
    height: u32,
    count: u16,
    ids: *const u32,
    offsets: *const i32,
    background: *const u16,
    out: *mut *mut HeifHandle,
) -> HeifError {
    if ids.is_null() {
        return Error::NULL.into();
    }
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&ctx.shared);
    if count == 0 {
        return report(
            &mut state,
            ContextError::new(5, 2006, "Usage error: Invalid parameter value"),
        );
    }
    let ids = unsafe { std::slice::from_raw_parts(ids, count as usize) };
    let offsets = (0..count as usize)
        .map(|i| {
            if offsets.is_null() {
                (0, 0)
            } else {
                unsafe { (offsets.add(i * 2).read(), offsets.add(i * 2 + 1).read()) }
            }
        })
        .collect();
    let background = if background.is_null() {
        [0; 4]
    } else {
        std::array::from_fn(|i| unsafe { background.add(i).read() })
    };
    match state.add_overlay(width, height, ids, offsets, background) {
        Err(error) => report(&mut state, error),
        Ok(image) => {
            if !out.is_null() {
                unsafe {
                    out.write(Box::into_raw(Box::new(HeifHandle {
                        shared: ctx.shared.clone(),
                        images: state.document.as_ref().unwrap().images.clone(),
                        id: image.id,
                    })))
                }
            };
            SUCCESS
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_assign_thumbnail(
    ctx: *mut HeifContext,
    master: *const HeifHandle,
    thumbnail: *const HeifHandle,
) -> HeifError {
    let (Some(ctx), Some(master), Some(thumbnail)) = (
        unsafe { ctx.as_ref() },
        unsafe { master.as_ref() },
        unsafe { thumbnail.as_ref() },
    ) else {
        return Error::NULL.into();
    };
    // The public adapter reverses the arguments to the core's assignment routine.
    let mut state = lock(&ctx.shared);
    state.items.add_reference(libheifer::items::Reference {
        from: master.id,
        kind: u32::from_be_bytes(*b"thmb"),
        to: vec![thumbnail.id],
    });
    state.last_error = c"Success".into();
    HeifError {
        message: state.last_error.as_ptr(),
        ..SUCCESS
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_encode_thumbnail(
    ctx: *mut HeifContext,
    image: *const Image,
    master: *const HeifHandle,
    encoder: *mut Encoder,
    options: *const EncodingOptions,
    bbox: i32,
    out: *mut *mut HeifHandle,
) -> HeifError {
    let (Some(ctx), Some(image), Some(master), Some(encoder)) = (
        unsafe { ctx.as_ref() },
        unsafe { image.as_ref() },
        unsafe { master.as_ref() },
        unsafe { encoder.as_ref() },
    ) else {
        return Error::NULL.into();
    };
    let mut state = lock(&ctx.shared);
    if i64::from(image.width) <= i64::from(bbox) && i64::from(image.height) <= i64::from(bbox) {
        // Returning Error::Ok from the native Result-valued core takes its
        // error alternative: success with the output pointer left untouched.
        return report(&mut state, ContextError::new(0, 0, "Success"));
    }
    let (w, h) = if image.width > image.height {
        (
            bbox,
            (image.height as i32).wrapping_mul(bbox) / (image.width as i32),
        )
    } else {
        (
            (image.width as i32).wrapping_mul(bbox) / (image.height as i32),
            bbox,
        )
    };
    let scaled =
        match image.scale_with_budget((w & !1) as u32, (h & !1) as u32, Some(state.budget.clone()))
        {
            Ok(image) => image,
            Err(error) => return report(&mut state, error.into()),
        };
    let mut copied = EncodingOptions::default();
    unsafe { heif_encoding_options_copy(&mut copied, options) };
    let nclx = unsafe { copied.output_nclx_profile.as_ref() }.map(|n| Nclx {
        primaries: n.color_primaries as u16,
        transfer: n.transfer_characteristics as u16,
        matrix: n.matrix_coefficients as u16,
        full_range: n.full_range_flag != 0,
    });
    let opts = Options {
        orientation: copied.image_orientation,
        nclx,
        two_profiles: copied.save_two_colr_boxes_when_ICC_and_nclx_available != 0,
        no_nclx: copied.macOS_compatibility_workaround_no_nclx_profile != 0,
    };
    let external = if matches!(
        encoder.source,
        crate::plugin_registry::EncoderSource::External(_)
    ) && matches!(encoder.source.format(), 1 | 4)
    {
        drop(state);
        let result =
            crate::plugin_encoding::encode_image(&ctx.shared, &scaled, encoder, &copied, &opts, 4);
        state = lock(&ctx.shared);
        Some(result)
    } else {
        None
    };
    let result = if let Some(result) = external {
        result
    } else {
        match encoder.source.format() {
            9 => state.encode_mask(&scaled, &opts),
            8 => state.encode_uncompressed(
                &scaled,
                &opts,
                if copied.unci_parameters.is_null() {
                    0
                } else {
                    unsafe { ptr::addr_of!((*copied.unci_parameters).compression).read() }
                },
            ),
            _ => Err(ContextError::new(
                4,
                6003,
                "Unsupported feature: Support for this compression format has not been built in",
            )),
        }
    };
    match result {
        Err(error) => report(&mut state, error),
        Ok(image) => {
            state.items.add_reference(libheifer::items::Reference {
                from: image.id,
                kind: u32::from_be_bytes(*b"thmb"),
                to: vec![master.id],
            });
            if !out.is_null() {
                unsafe {
                    out.write(Box::into_raw(Box::new(HeifHandle {
                        shared: ctx.shared.clone(),
                        images: state.document.as_ref().unwrap().images.clone(),
                        id: image.id,
                    })))
                }
            };
            SUCCESS
        }
    }
}
