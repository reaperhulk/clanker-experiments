// SPDX-License-Identifier: LGPL-3.0-or-later
use super::context::{HeifHandle, lock};
use libheifer::{image::Image, omaf::FLAT};
use std::ffi::c_int;
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_omaf_image_projection(
    handle: *const HeifHandle,
) -> c_int {
    unsafe { handle.as_ref() }.map_or(FLAT, |h| h.image().projection())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_set_omaf_image_projection(
    handle: *mut HeifHandle,
    value: c_int,
) {
    if let Some(h) = unsafe { handle.as_ref() } {
        let _ = h.image().set_projection(&mut lock(&h.shared), value);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_omaf_image_projection(image: *const Image) -> c_int {
    unsafe { image.as_ref() }.map_or(FLAT, |i| i.projection)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_omaf_image_projection(image: *mut Image, value: c_int) {
    if let Some(i) = unsafe { image.as_mut() } {
        i.projection = value;
    }
}
