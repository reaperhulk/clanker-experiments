// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::context::{HeifHandle, lock};
use libheifer::gimi::c_string;
use std::{
    ffi::{CStr, c_char, c_int},
    ptr,
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_gimi_content_id(
    handle: *const HeifHandle,
) -> *const c_char {
    let Some(h) = (unsafe { handle.as_ref() }) else {
        return ptr::null();
    };
    let v = h
        .image()
        .gimi_content_id
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if v.is_empty() {
        ptr::null()
    } else {
        c_string(&v).into_raw()
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_set_gimi_content_id(
    handle: *mut HeifHandle,
    id: *const c_char,
) {
    if let Some(h) = unsafe { handle.as_ref() }
        && !id.is_null()
    {
        h.image().set_content_id(
            &mut lock(&h.shared),
            unsafe { CStr::from_ptr(id) }.to_bytes().to_vec(),
        );
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_has_gimi_component_content_ids(
    handle: *const HeifHandle,
) -> c_int {
    unsafe { handle.as_ref() }
        .and_then(|h| h.image().component_content_ids())
        .map_or(0, |ids| {
            ids.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len() as c_int
        })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_gimi_component_content_id(
    handle: *const HeifHandle,
    index: u32,
) -> *const c_char {
    unsafe { handle.as_ref() }
        .and_then(|h| h.image().component_content_ids())
        .and_then(|ids| {
            ids.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(index as usize)
                .cloned()
        })
        .map_or(ptr::null(), |v| v.into_raw())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_set_gimi_component_content_id(
    handle: *mut HeifHandle,
    index: u32,
    id: *const c_char,
) {
    if let Some(h) = unsafe { handle.as_ref() }
        && !id.is_null()
    {
        h.image().set_component_content_id(
            &mut lock(&h.shared),
            index,
            unsafe { CStr::from_ptr(id) }.to_owned(),
        );
    }
}
