// SPDX-License-Identifier: LGPL-3.0-or-later
use super::{
    HeifError, SUCCESS,
    context::{HeifHandle, report_image},
};
use libheifer::{
    camera::{ExtrinsicMatrix, IntrinsicMatrix},
    context::ContextError,
    error::Error,
};
use std::ffi::c_int;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_has_camera_intrinsic_matrix(
    handle: *const HeifHandle,
) -> c_int {
    unsafe { handle.as_ref() }
        .is_some_and(|h| h.image().intrinsic.is_some())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_camera_intrinsic_matrix(
    handle: *const HeifHandle,
    out: *mut IntrinsicMatrix,
) -> HeifError {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let Some(matrix) = handle.image().intrinsic else {
        return report_image(
            handle.image(),
            ContextError::new(5, 138, "Usage error: Camera intrinsic matrix undefined"),
        );
    };
    unsafe {
        out.write(matrix);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_has_camera_extrinsic_matrix(
    handle: *const HeifHandle,
) -> c_int {
    unsafe { handle.as_ref() }
        .is_some_and(|h| h.image().extrinsic.is_some())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_camera_extrinsic_matrix(
    handle: *const HeifHandle,
    out: *mut *mut ExtrinsicMatrix,
) -> HeifError {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let Some(matrix) = handle.image().extrinsic else {
        return report_image(
            handle.image(),
            ContextError::new(5, 139, "Usage error: Camera extrinsic matrix undefined"),
        );
    };
    let matrix = super::color::allocate(matrix);
    if matrix.is_null() {
        return Error::ALLOCATION.into();
    }
    unsafe {
        out.write(matrix);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_camera_extrinsic_matrix_release(matrix: *mut ExtrinsicMatrix) {
    if !matrix.is_null() {
        unsafe {
            drop(Box::from_raw(matrix));
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_camera_extrinsic_matrix_get_rotation_matrix(
    matrix: *const ExtrinsicMatrix,
    out: *mut f64,
) -> HeifError {
    let Some(matrix) = (unsafe { matrix.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    unsafe {
        std::ptr::copy_nonoverlapping(matrix.rotation().as_ptr(), out, 9);
    }
    SUCCESS
}
