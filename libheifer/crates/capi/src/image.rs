// SPDX-License-Identifier: LGPL-3.0-or-later
use super::{HeifError, SUCCESS};
use libheifer::{error::Error, image::Image};
use std::ffi::c_int;
use std::ptr;

fn dimension(value: u32) -> c_int {
    if value == 0 || value > c_int::MAX as u32 {
        -1
    } else {
        value as c_int
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_create(
    width: c_int,
    height: c_int,
    colorspace: c_int,
    chroma: c_int,
    output: *mut *mut Image,
) -> HeifError {
    if output.is_null() {
        return Error::NULL.into();
    }
    match Image::new(width as u32, height as u32, colorspace, chroma) {
        Ok(img) => {
            unsafe { output.write(Box::into_raw(Box::new(img))) };
            SUCCESS
        }
        Err(err) => {
            unsafe { output.write(ptr::null_mut()) };
            err.into()
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_release(img: *const Image) {
    if !img.is_null() {
        unsafe { drop(Box::from_raw(img.cast_mut())) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_add_plane(
    img: *mut Image,
    channel: c_int,
    width: c_int,
    height: c_int,
    depth: c_int,
) -> HeifError {
    if img.is_null() {
        return Error::NULL.into();
    }
    match unsafe { &mut *img }.add_plane(channel, width as u32, height as u32, depth) {
        Ok(()) => SUCCESS,
        Err(e) => e.into(),
    }
}

macro_rules! query {
    ($name:ident, $body:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(img: *const Image) -> c_int {
            if img.is_null() {
                return -1;
            }
            ($body)(unsafe { &*img })
        }
    };
}
query!(heif_image_get_primary_width, |i: &Image| dimension(i.width));
query!(heif_image_get_primary_height, |i: &Image| dimension(
    i.height
));
query!(heif_image_get_colorspace, |i: &Image| i.colorspace);
query!(heif_image_get_chroma_format, |i: &Image| i.chroma);

macro_rules! plane_query {
    ($name:ident, $default:expr, $body:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(img: *const Image, channel: c_int) -> c_int {
            if img.is_null() {
                return $default;
            }
            unsafe { &*img }.plane(channel).map_or($default, $body)
        }
    };
}
plane_query!(heif_image_get_width, -1, |p| dimension(p.width));
plane_query!(heif_image_get_height, -1, |p| dimension(p.height));
plane_query!(heif_image_get_bits_per_pixel, -1, |p| p.storage_bits());
plane_query!(heif_image_get_bits_per_pixel_range, -1, |p| i32::from(
    p.bit_depth
));
plane_query!(heif_image_has_channel, 0, |_| 1);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_plane_readonly2(
    img: *const Image,
    channel: c_int,
    stride: *mut usize,
) -> *const u8 {
    if stride.is_null() {
        return ptr::null();
    }
    let plane = unsafe { img.as_ref() }.and_then(|i| i.plane(channel));
    unsafe { stride.write(plane.map_or(0, |p| p.stride)) };
    plane.map_or(ptr::null(), |p| p.data().as_ptr())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_plane2(
    img: *mut Image,
    channel: c_int,
    stride: *mut usize,
) -> *mut u8 {
    if stride.is_null() {
        return ptr::null_mut();
    }
    let plane = unsafe { img.as_mut() }.and_then(|i| i.plane_mut(channel));
    unsafe { stride.write(plane.as_ref().map_or(0, |p| p.stride)) };
    plane.map_or(ptr::null_mut(), |p| p.data_mut().as_mut_ptr())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_plane_readonly(
    img: *const Image,
    channel: c_int,
    stride: *mut c_int,
) -> *const u8 {
    if stride.is_null() {
        return ptr::null();
    }
    let mut wide = 0;
    let data = unsafe { heif_image_get_plane_readonly2(img, channel, &mut wide) };
    if wide > c_int::MAX as usize {
        return ptr::null();
    }
    unsafe { stride.write(wide as c_int) };
    data
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_plane(
    img: *mut Image,
    channel: c_int,
    stride: *mut c_int,
) -> *mut u8 {
    if stride.is_null() {
        return ptr::null_mut();
    }
    let mut wide = 0;
    let data = unsafe { heif_image_get_plane2(img, channel, &mut wide) };
    if wide > c_int::MAX as usize {
        return ptr::null_mut();
    }
    unsafe { stride.write(wide as c_int) };
    data
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_premultiplied_alpha(img: *mut Image, value: c_int) {
    if let Some(img) = unsafe { img.as_mut() } {
        img.premultiplied_alpha = value != 0;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_is_premultiplied_alpha(img: *const Image) -> c_int {
    unsafe { img.as_ref() }.map_or(0, |i| i32::from(i.premultiplied_alpha))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_pixel_aspect_ratio(img: *mut Image, h: u32, v: u32) {
    if let Some(img) = unsafe { img.as_mut() } {
        img.pixel_aspect_ratio = (h, v);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_pixel_aspect_ratio(
    img: *const Image,
    h: *mut u32,
    v: *mut u32,
) {
    if let Some(img) = unsafe { img.as_ref() } {
        if !h.is_null() {
            unsafe { h.write(img.pixel_aspect_ratio.0) };
        }
        if !v.is_null() {
            unsafe { v.write(img.pixel_aspect_ratio.1) };
        }
    }
}
