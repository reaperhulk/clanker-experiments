// SPDX-License-Identifier: LGPL-3.0-or-later
use super::context::HeifHandle;
use std::{ffi::c_char, ptr};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_number_of_cmpd_components(
    handle: *const HeifHandle,
) -> u32 {
    unsafe { handle.as_ref() }.map_or(0, |h| h.image().cmpd_components().len() as u32)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_cmpd_component_type(
    handle: *const HeifHandle,
    index: u32,
) -> u16 {
    unsafe { handle.as_ref() }
        .and_then(|h| {
            h.image()
                .cmpd_components()
                .get(index as usize)
                .map(|d| d.kind)
        })
        .unwrap_or(0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_cmpd_component_type_uri(
    handle: *const HeifHandle,
    index: u32,
) -> *const c_char {
    unsafe { handle.as_ref() }
        .and_then(|h| {
            h.image()
                .cmpd_components()
                .get(index as usize)
                .filter(|d| !d.uri.is_empty())
                .map(|d| d.uri.clone().into_raw() as *const c_char)
        })
        .unwrap_or(ptr::null())
}
