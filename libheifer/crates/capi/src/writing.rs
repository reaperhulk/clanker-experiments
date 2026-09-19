// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, lock, report},
};
use libheifer::{context::ContextError, error::Error};
use std::{
    ffi::{CStr, c_char, c_int, c_void},
    ptr,
};
#[repr(C)]
pub struct Writer {
    pub writer_api_version: c_int,
    pub write: Option<
        unsafe extern "C" fn(*mut HeifContext, *const c_void, usize, *mut c_void) -> HeifError,
    >,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_major_brand(ctx: *mut HeifContext, brand: u32) {
    let ctx = unsafe { &*ctx };
    let state = lock(&ctx.shared);
    let mut layout = state.items.layout.lock().unwrap();
    layout.major = brand;
    layout.compatible(brand);
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_compatible_brand(ctx: *mut HeifContext, brand: u32) {
    unsafe { lock(&(*ctx).shared) }
        .items
        .layout
        .lock()
        .unwrap()
        .compatible(brand);
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_unif(ctx: *mut HeifContext, flag: c_int) {
    unsafe { lock(&(*ctx).shared) }
        .items
        .layout
        .lock()
        .unwrap()
        .unif = flag != 0;
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_write_mini_format(ctx: *mut HeifContext, flag: c_int) {
    unsafe { lock(&(*ctx).shared) }
        .items
        .layout
        .lock()
        .unwrap()
        .mini = flag != 0;
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_write(
    ctx: *mut HeifContext,
    writer: *mut Writer,
    userdata: *mut c_void,
) -> HeifError {
    let Some(context) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    let data = {
        let mut state = lock(&context.shared);
        if writer.is_null() {
            return report(
                &mut state,
                ContextError::new(5, 2001, "Usage error: NULL argument received"),
            );
        }
        if unsafe { ptr::addr_of!((*writer).writer_api_version).read() } != 1 {
            return report(
                &mut state,
                ContextError::new(
                    5,
                    2004,
                    "Usage error: The version of the passed writer is not supported",
                ),
            );
        }
        match state.serialize() {
            Ok(data) => data,
            Err(error) => return report(&mut state, error),
        }
    };
    let Some(write) = (unsafe { ptr::addr_of!((*writer).write).read() }) else {
        return Error::NULL.into();
    };
    let mut error = unsafe { write(ctx, data.as_ptr().cast(), data.len(), userdata) };
    if error.message.is_null() {
        if error.code == 0 {
            error.message = SUCCESS.message;
        } else {
            return Error::new(5, 2001, c"heif_writer callback returned a null error text").into();
        }
    }
    error
}
unsafe extern "C" fn file_write(
    ctx: *mut HeifContext,
    data: *const c_void,
    size: usize,
    userdata: *mut c_void,
) -> HeifError {
    if userdata.is_null() {
        return Error::NULL.into();
    }
    let filename = unsafe { CStr::from_ptr(userdata.cast()) };
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(filename.to_bytes()))
    };
    #[cfg(not(unix))]
    let path = std::path::PathBuf::from(filename.to_string_lossy().into_owned());
    let _ = std::fs::write(path, unsafe {
        std::slice::from_raw_parts(data.cast::<u8>(), size)
    });
    let mut state = unsafe { lock(&(*ctx).shared) };
    state.last_error = c"Success".into();
    HeifError {
        message: state.last_error.as_ptr(),
        ..SUCCESS
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_write_to_file(
    ctx: *mut HeifContext,
    filename: *const c_char,
) -> HeifError {
    if filename.is_null() {
        return Error::NULL.into();
    }
    let mut writer = Writer {
        writer_api_version: 1,
        write: Some(file_write),
    };
    unsafe { heif_context_write(ctx, &mut writer, filename.cast_mut().cast()) }
}
