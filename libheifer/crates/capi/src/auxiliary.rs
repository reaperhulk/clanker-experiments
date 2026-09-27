// SPDX-License-Identifier: LGPL-3.0-or-later
use super::{
    HeifError, SUCCESS,
    context::{HeifHandle, create_handle_images, report_image},
};
use libheifer::{context::ContextError, error::Error};
use std::{
    ffi::{CString, c_char, c_int},
    ptr,
};

#[repr(C)]
pub struct DepthRepresentationInfo {
    pub version: u8,
    pub has_z_near: u8,
    pub has_z_far: u8,
    pub has_d_min: u8,
    pub has_d_max: u8,
    pub z_near: f64,
    pub z_far: f64,
    pub d_min: f64,
    pub d_max: f64,
    pub depth_representation_type: c_int,
    pub disparity_reference_view: u32,
    pub depth_nonlinear_representation_model_size: u32,
    pub depth_nonlinear_representation_model: *mut u8,
}
fn invalid(handle: &HeifHandle) -> HeifError {
    report_image(
        handle.image(),
        ContextError::new(5, 2000, "Usage error: Non-existing item ID referenced"),
    )
}
unsafe fn related(handle: &HeifHandle, id: u32, out: *mut *mut HeifHandle) -> HeifError {
    if let Some(error) = &handle.images[&id].error {
        return report_image(handle.image(), error.clone());
    }
    unsafe { create_handle_images(&handle.shared, &handle.images, id, out) }
}
fn auxiliary_ids(handle: &HeifHandle, filter: c_int) -> impl Iterator<Item = u32> + '_ {
    handle
        .image()
        .auxiliary
        .images
        .iter()
        .copied()
        .filter(move |id| {
            let aux = &handle.images[id].auxiliary;
            (filter & 2 == 0 || !aux.is_alpha) && (filter & 4 == 0 || !aux.is_depth)
        })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_number_of_auxiliary_images(
    handle: *const HeifHandle,
    filter: c_int,
) -> c_int {
    unsafe { handle.as_ref() }.map_or(0, |h| auxiliary_ids(h, filter).count() as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_list_of_auxiliary_image_IDs(
    handle: *const HeifHandle,
    filter: c_int,
    ids: *mut u32,
    count: c_int,
) -> c_int {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    if ids.is_null() {
        return 0;
    }
    if count < 0 {
        return count;
    }
    let mut n = 0;
    for id in auxiliary_ids(handle, filter).take(count as usize) {
        unsafe { ids.add(n).write(id) };
        n += 1;
    }
    n as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_auxiliary_type(
    handle: *const HeifHandle,
    out: *mut *const c_char,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    unsafe { out.write(ptr::null()) };
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    unsafe { out.write(handle.image().auxiliary.kind.clone().into_raw()) };
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_release_auxiliary_type(
    _handle: *const HeifHandle,
    out: *mut *const c_char,
) {
    if let Some(value) = unsafe { out.as_mut() }
        && !value.is_null()
    {
        unsafe { drop(CString::from_raw(value.cast_mut())) };
        *value = ptr::null();
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_free_auxiliary_types(
    handle: *const HeifHandle,
    out: *mut *const c_char,
) {
    unsafe { heif_image_handle_release_auxiliary_type(handle, out) };
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_auxiliary_image_handle(
    handle: *const HeifHandle,
    id: u32,
    out: *mut *mut HeifHandle,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    unsafe { out.write(ptr::null_mut()) };
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    if !handle.image().auxiliary.images.contains(&id) {
        return invalid(handle);
    }
    unsafe { related(handle, id, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_has_depth_image(handle: *const HeifHandle) -> c_int {
    unsafe { handle.as_ref() }
        .is_some_and(|h| h.image().auxiliary.depth_image.is_some())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_number_of_depth_images(
    handle: *const HeifHandle,
) -> c_int {
    unsafe { heif_image_handle_has_depth_image(handle) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_list_of_depth_image_IDs(
    handle: *const HeifHandle,
    ids: *mut u32,
    count: c_int,
) -> c_int {
    if count == 0 || ids.is_null() {
        return 0;
    }
    if let Some(id) = unsafe { handle.as_ref() }.and_then(|h| h.image().auxiliary.depth_image) {
        unsafe { ids.write(id) };
        1
    } else {
        0
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_depth_image_handle(
    handle: *const HeifHandle,
    id: u32,
    out: *mut *mut HeifHandle,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    unsafe { out.write(ptr::null_mut()) };
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    if handle.image().auxiliary.depth_image != Some(id) {
        return invalid(handle);
    }
    unsafe { related(handle, id, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_depth_image_representation_info(
    handle: *const HeifHandle,
    _id: u32,
    out: *mut *const DepthRepresentationInfo,
) -> c_int {
    if out.is_null() {
        return 0;
    }
    unsafe { out.write(ptr::null()) };
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    let info = &handle.image().auxiliary;
    let depth = if info.is_depth {
        Some(handle.image())
    } else {
        info.depth_image
            .and_then(|id| handle.images.get(&id).map(AsRef::as_ref))
    };
    let Some(info) = depth.and_then(|d| d.auxiliary.depth_info) else {
        return 0;
    };
    let value = super::color::allocate(DepthRepresentationInfo {
        version: 1,
        has_z_near: info.flags[0],
        has_z_far: info.flags[1],
        has_d_min: info.flags[2],
        has_d_max: info.flags[3],
        z_near: info.values[0],
        z_far: info.values[1],
        d_min: info.values[2],
        d_max: info.values[3],
        depth_representation_type: info.representation_type,
        disparity_reference_view: info.disparity_reference_view,
        depth_nonlinear_representation_model_size: 0,
        depth_nonlinear_representation_model: ptr::null_mut(),
    });
    unsafe { out.write(value) };
    (!value.is_null()).into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_depth_representation_info_free(info: *const DepthRepresentationInfo) {
    if !info.is_null() {
        unsafe { drop(Box::from_raw(info.cast_mut())) };
    }
}
