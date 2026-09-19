// SPDX-License-Identifier: LGPL-3.0-or-later
use super::context::{HeifHandle, lock};
use libheifer::{
    color::{AmbientViewingEnvironment, ContentLightLevel, MasteringDisplayColourVolume},
    handle_properties::Value,
};
use std::ffi::c_int;

macro_rules! property {
    ($has:ident, $get:ident, $set:ident, $kind:expr, $variant:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $has(handle: *const HeifHandle) -> c_int {
            unsafe { handle.as_ref() }
                .is_some_and(|h| h.image().handle_property($kind).is_some())
                .into()
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(handle: *const HeifHandle, output: *mut $ty) -> c_int {
            let Some(Value::$variant(value)) =
                unsafe { handle.as_ref() }.and_then(|h| h.image().handle_property($kind))
            else {
                return 0;
            };
            if !output.is_null() {
                unsafe { output.write(value) };
            }
            1
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(handle: *const HeifHandle, value: *const $ty) {
            let Some(value) = (unsafe { value.as_ref() }) else {
                return;
            };
            let Some(handle) = (unsafe { handle.as_ref() }) else {
                return;
            };
            let _ = handle
                .image()
                .attach_handle_property(&mut lock(&handle.shared), Value::$variant(*value));
        }
    };
}
property!(
    heif_image_handle_has_content_light_level,
    heif_image_handle_get_content_light_level,
    heif_image_handle_set_content_light_level,
    *b"clli",
    ContentLight,
    ContentLightLevel
);
property!(
    heif_image_handle_has_mastering_display_colour_volume,
    heif_image_handle_get_mastering_display_colour_volume,
    heif_image_handle_set_mastering_display_colour_volume,
    *b"mdcv",
    Mastering,
    MasteringDisplayColourVolume
);
property!(
    heif_image_handle_has_ambient_viewing_environment,
    heif_image_handle_get_ambient_viewing_environment,
    heif_image_handle_set_ambient_viewing_environment,
    *b"amve",
    Ambient,
    AmbientViewingEnvironment
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_has_nominal_diffuse_white_luminance(
    handle: *const HeifHandle,
) -> c_int {
    unsafe { handle.as_ref() }
        .is_some_and(|h| h.image().handle_property(*b"ndwt").is_some())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_nominal_diffuse_white_luminance(
    handle: *const HeifHandle,
) -> u32 {
    match unsafe { handle.as_ref() }.and_then(|h| h.image().handle_property(*b"ndwt")) {
        Some(Value::DiffuseWhite(v)) => v,
        _ => 0,
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_set_nominal_diffuse_white_luminance(
    handle: *const HeifHandle,
    value: u32,
) {
    if let Some(h) = unsafe { handle.as_ref() } {
        let _ = h
            .image()
            .attach_handle_property(&mut lock(&h.shared), Value::DiffuseWhite(value));
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_set_pixel_aspect_ratio(
    handle: *mut HeifHandle,
    h: u32,
    v: u32,
) {
    if let Some(handle) = unsafe { handle.as_ref() } {
        let _ = handle
            .image()
            .attach_handle_property(&mut lock(&handle.shared), Value::PixelAspect(h, v));
    }
}
