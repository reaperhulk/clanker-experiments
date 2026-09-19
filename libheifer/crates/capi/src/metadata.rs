// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, HeifHandle, lock, report},
};
use libheifer::error::Error;
use std::ffi::{CStr, CString, c_char, c_int, c_void};

unsafe fn input<'a>(data: *const c_void, size: c_int) -> Result<&'a [u8], Error> {
    if size < 0 {
        return Err(Error::new(5, 2006, c"metadata size must not be negative"));
    }
    if size == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(Error::NULL);
    }
    Ok(unsafe { std::slice::from_raw_parts(data.cast(), size as usize) })
}
unsafe fn add(
    ctx: *mut HeifContext,
    handle: *const HeifHandle,
    kind: [u8; 4],
    content_type: Option<CString>,
    data: &[u8],
    compression: c_int,
    out: *mut u32,
) -> HeifError {
    let (Some(ctx), Some(handle)) = (unsafe { ctx.as_ref() }, unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&ctx.shared);
    match state.add_metadata(handle.id, kind, content_type, data, compression) {
        Ok(id) => {
            if !out.is_null() {
                unsafe {
                    out.write(id);
                }
            }
            SUCCESS
        }
        Err(error) => report(&mut state, error),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_exif_metadata(
    ctx: *mut HeifContext,
    handle: *const HeifHandle,
    data: *const c_void,
    size: c_int,
) -> HeifError {
    let data = match unsafe { input(data, size) } {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let Some(context) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    let data = match libheifer::metadata::exif_payload(data) {
        Ok(v) => v,
        Err(e) => return report(&mut lock(&context.shared), e),
    };
    unsafe { add(ctx, handle, *b"Exif", None, &data, 0, std::ptr::null_mut()) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_XMP_metadata(
    ctx: *mut HeifContext,
    handle: *const HeifHandle,
    data: *const c_void,
    size: c_int,
) -> HeifError {
    unsafe { heif_context_add_XMP_metadata2(ctx, handle, data, size, 0) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_XMP_metadata2(
    ctx: *mut HeifContext,
    handle: *const HeifHandle,
    data: *const c_void,
    size: c_int,
    compression: c_int,
) -> HeifError {
    let data = match unsafe { input(data, size) } {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    unsafe {
        add(
            ctx,
            handle,
            *b"mime",
            Some(c"application/rdf+xml".into()),
            data,
            compression,
            std::ptr::null_mut(),
        )
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_generic_metadata(
    ctx: *mut HeifContext,
    handle: *const HeifHandle,
    data: *const c_void,
    size: c_int,
    kind: *const c_char,
    content_type: *const c_char,
) -> HeifError {
    let kind = if kind.is_null() {
        &[][..]
    } else {
        unsafe { CStr::from_ptr(kind) }.to_bytes()
    };
    let Ok(kind) = <[u8; 4]>::try_from(kind) else {
        return Error::new(
            5,
            2006,
            c"called heif_context_add_generic_metadata() with invalid 'item_type'.",
        )
        .into();
    };
    let data = match unsafe { input(data, size) } {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let content_type = if content_type.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(content_type) }.into())
    };
    unsafe {
        add(
            ctx,
            handle,
            kind,
            content_type,
            data,
            0,
            std::ptr::null_mut(),
        )
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_generic_uri_metadata(
    ctx: *mut HeifContext,
    handle: *const HeifHandle,
    data: *const c_void,
    size: c_int,
    _uri: *const c_char,
    out: *mut u32,
) -> HeifError {
    // The pinned implementation ignores item_uri_type in this entry point.
    let data = match unsafe { input(data, size) } {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    unsafe { add(ctx, handle, *b"uri ", None, data, 0, out) }
}
