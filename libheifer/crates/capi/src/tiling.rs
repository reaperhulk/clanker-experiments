// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifHandle, lock, report},
};
use libheifer::{error::Error, tiling::Tiling};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_image_tiling(
    handle: *const HeifHandle,
    process: i32,
    out: *mut Tiling,
) -> HeifError {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let mut state = lock(&handle.shared);
    let Some(doc) = state.decoding_document() else {
        return Error::NULL.into();
    };
    let mut tiling = match Tiling::for_image(&doc, handle.image()) {
        Ok(t) => t,
        Err(e) => return report(&mut state, e),
    };
    unsafe { out.write(tiling) };
    if tiling.tile_width != 0 && tiling.tile_height != 0 {
        let limit = *state.limits.read().unwrap();
        if let Err(e) = limit.check_image_size(tiling.tile_width, tiling.tile_height) {
            return report(&mut state, e);
        }
    }
    if process != 0 {
        let result = state
            .properties
            .get(handle.id)
            .and_then(|p| tiling.transform(&p));
        unsafe { out.write(tiling) };
        if let Err(e) = result {
            return report(&mut state, e);
        }
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_grid_image_tile_id(
    handle: *const HeifHandle,
    process: i32,
    x: u32,
    y: u32,
    out: *mut u32,
) -> HeifError {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let image = handle.image();
    if image.kind != *b"grid" {
        return Error::new(5, 0, c"Image is no grid image").into();
    }
    let mut state = lock(&handle.shared);
    let Some(doc) = state.decoding_document() else {
        return Error::NULL.into();
    };
    let t = match Tiling::for_image(&doc, image) {
        Ok(t) => t,
        Err(e) => return report(&mut state, e),
    };
    let (x, y) = if process != 0 {
        match state
            .properties
            .get(handle.id)
            .and_then(|p| t.original_position(&p, x, y))
        {
            Ok(v) => v,
            Err(e) => return report(&mut state, e),
        }
    } else {
        if x >= t.num_columns || y >= t.num_rows {
            return Error::new(5, 0, c"Grid tile index out of range").into();
        }
        (x, y)
    };
    let ids = image
        .grid_tiles
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(id) = ids.get((u64::from(y) * u64::from(t.num_columns) + u64::from(x)) as usize)
    else {
        return Error::new(5, 0, c"Grid tile index out of range").into();
    };
    unsafe { out.write(*id) };
    SUCCESS
}

use crate::{
    context::{HeifContext, HeifHandle as Handle},
    encoder::Encoder,
    encoding_options::{EncodingOptions, UnciParameters, heif_encoding_options_copy},
};
use libheifer::{
    context::{Context, ContextError, ImageInfo},
    encoding::Options,
    image::Image,
};
use std::{ptr, sync::Arc};
unsafe fn options(input: *const EncodingOptions, fallback: Option<&Image>) -> (Options, i32) {
    let mut o = EncodingOptions::default();
    unsafe { heif_encoding_options_copy(&mut o, input) };
    let nclx = unsafe { o.output_nclx_profile.as_ref() }
        .map(|n| libheifer::color::Nclx {
            primaries: n.color_primaries as u16,
            transfer: n.transfer_characteristics as u16,
            matrix: n.matrix_coefficients as u16,
            full_range: n.full_range_flag != 0,
        })
        .or_else(|| {
            if input.is_null() {
                None
            } else {
                fallback.and_then(|i| i.color.nclx)
            }
        });
    (
        Options {
            orientation: o.image_orientation,
            nclx,
            two_profiles: o.save_two_colr_boxes_when_ICC_and_nclx_available != 0,
            no_nclx: o.macOS_compatibility_workaround_no_nclx_profile != 0,
        },
        unsafe { o.unci_parameters.as_ref() }.map_or(0, |p| p.compression),
    )
}
unsafe fn finish(
    ctx: &HeifContext,
    state: &mut Context,
    result: Result<Arc<ImageInfo>, ContextError>,
    out: *mut *mut Handle,
) -> HeifError {
    match result {
        Err(e) => report(state, e),
        Ok(image) => {
            if state.document.as_ref().is_none_or(|d| d.primary == 0) {
                state.set_primary(image.clone());
            }
            if !out.is_null() {
                unsafe {
                    out.write(Box::into_raw(Box::new(Handle {
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
pub unsafe extern "C" fn heif_context_add_grid_image(
    ctx: *mut HeifContext,
    width: u32,
    height: u32,
    columns: u32,
    rows: u32,
    input: *const EncodingOptions,
    out: *mut *mut Handle,
) -> HeifError {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&ctx.shared);
    if columns == 0 || rows == 0 {
        return report(
            &mut state,
            ContextError::new(5, 2006, "Usage error: Invalid parameter value"),
        );
    }
    if columns > 65535 || rows > 65535 {
        return Error::new(5, 129, c"Number of tile rows/columns may not exceed 65535").into();
    }
    let (o, compression) = unsafe { options(input, None) };
    let result = state.add_grid((width, height), columns, rows, o, compression, None);
    unsafe { finish(ctx, &mut state, result, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_encode_grid(
    ctx: *mut HeifContext,
    tiles: *const *const Image,
    columns: u16,
    rows: u16,
    encoder: *mut Encoder,
    input: *const EncodingOptions,
    out: *mut *mut Handle,
) -> HeifError {
    let (Some(ctx), Some(encoder)) = (unsafe { ctx.as_ref() }, unsafe { encoder.as_ref() }) else {
        return Error::NULL.into();
    };
    if tiles.is_null() {
        return Error::NULL.into();
    }
    let mut state = lock(&ctx.shared);
    if columns == 0 || rows == 0 {
        return report(
            &mut state,
            ContextError::new(5, 2006, "Usage error: Invalid parameter value"),
        );
    }
    let tiles =
        unsafe { std::slice::from_raw_parts(tiles, usize::from(columns) * usize::from(rows)) };
    let Some(first) = (unsafe { tiles[0].as_ref() }) else {
        return Error::NULL.into();
    };
    let (o, compression) = unsafe { options(input, Some(first)) };
    let mut ids = Vec::new();
    for &tile in tiles {
        let Some(tile) = (unsafe { tile.as_ref() }) else {
            return Error::NULL.into();
        };
        match state.encode_format(tile, encoder.source.format(), &o, compression) {
            Err(e) => return report(&mut state, e),
            Ok(image) => {
                state.items.items.get_mut(&image.id).unwrap().hidden = true;
                ids.push(image.id);
            }
        }
    }
    let result = state
        .add_grid(
            (
                first.width.wrapping_mul(u32::from(columns)),
                first.height.wrapping_mul(u32::from(rows)),
            ),
            u32::from(columns),
            u32::from(rows),
            o,
            compression,
            Some(&ids),
        )
        .and_then(|g| {
            state.finish_full_grid(&g, first, ids[0])?;
            Ok(g)
        });
    unsafe { finish(ctx, &mut state, result, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_image_tile(
    ctx: *mut HeifContext,
    tiled: *mut Handle,
    x: u32,
    y: u32,
    image: *const Image,
    encoder: *mut Encoder,
) -> HeifError {
    let (Some(ctx), Some(tiled), Some(image), Some(encoder)) = (
        unsafe { ctx.as_ref() },
        unsafe { tiled.as_ref() },
        unsafe { image.as_ref() },
        unsafe { encoder.as_ref() },
    ) else {
        return Error::NULL.into();
    };
    let mut state = lock(&ctx.shared);
    let result = match &tiled.image().kind {
        b"grid" => state.add_grid_tile(tiled.image(), x, y, image, encoder.source.format()),
        b"unci" => state.add_uncompressed_tile(tiled.image(), x, y, image),
        _ => return Error::new(5, 0, c"Cannot add tile to a non-tiled image").into(),
    };
    report(
        &mut state,
        result
            .err()
            .unwrap_or_else(|| ContextError::new(0, 0, "Success")),
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_empty_unci_image(
    ctx: *mut HeifContext,
    params: *const UnciParameters,
    input: *const EncodingOptions,
    prototype: *const Image,
    out: *mut *mut Handle,
) -> HeifError {
    let (Some(ctx), Some(prototype)) = (unsafe { ctx.as_ref() }, unsafe { prototype.as_ref() })
    else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let mut state = lock(&ctx.shared);
    let params = if params.is_null()
        && !input.is_null()
        && unsafe { ptr::addr_of!((*input).version).read() } >= 8
    {
        unsafe { ptr::addr_of!((*input).unci_parameters).read() }
    } else {
        params
    };
    let Some(params) = (unsafe { params.as_ref() }) else {
        return report(
            &mut state,
            ContextError::new(
                5,
                2006,
                "Usage error: Invalid parameter value: heif_context_add_empty_unci_image: either the 'parameters' argument or heif_encoding_options::unci_parameters must be non-null.",
            ),
        );
    };
    let (o, _) = unsafe { options(input, None) };
    let result = state.add_empty_uncompressed(
        (params.image_width, params.image_height),
        (params.tile_width, params.tile_height),
        params.compression,
        &o,
        prototype,
    );
    unsafe { finish(ctx, &mut state, result, out) }
}
