// SPDX-License-Identifier: LGPL-3.0-or-later
use super::{HeifError, SUCCESS};
use libheifer::{
    components::{Complex32, Complex64},
    error::Error,
    image::Image,
};
use std::{
    ffi::{CStr, c_char, c_int},
    ptr,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_number_of_used_components(image: *const Image) -> u32 {
    unsafe { image.as_ref() }.map_or(0, |i| i.component_ids.descriptions.len() as u32)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_used_component_ids(image: *const Image, out: *mut u32) {
    if let Some(image) = unsafe { image.as_ref() }
        && !out.is_null()
    {
        for (index, description) in image.component_ids.descriptions.iter().enumerate() {
            unsafe {
                out.add(index).write(description.id);
            }
        }
    }
}
macro_rules! query {
    ($name:ident, $ty:ty, $null:expr, $missing:expr, $field:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(image: *const Image, id: u32) -> $ty {
            let Some(image) = (unsafe { image.as_ref() }) else {
                return $null;
            };
            // Several upstream getters dereference unknown IDs. Return a safe
            // sentinel here; those undefined calls are not compatibility cases.
            image
                .component_ids
                .find(id)
                .map_or($missing, |d| d.$field as $ty)
        }
    };
}
query!(heif_image_get_component_channel, c_int, 0, 65535, channel);
query!(heif_image_get_component_width, u32, 0, 0, width);
query!(heif_image_get_component_height, u32, 0, 0, height);
query!(
    heif_image_get_component_bits_per_pixel,
    c_int,
    0,
    0,
    bit_depth
);
query!(heif_image_get_component_type, u16, 0, 32767, kind);
query!(heif_image_get_component_datatype, c_int, 255, 255, datatype);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_add_component(
    image: *mut Image,
    width: c_int,
    height: c_int,
    kind: u16,
    datatype: c_int,
    bit_depth: c_int,
    out: *mut u32,
) -> HeifError {
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    let budget = image.budget.take();
    let result = image.add_component(width as u32, height as u32, kind, datatype, bit_depth);
    image.budget = budget;
    match result {
        Ok(id) => {
            if !out.is_null() {
                unsafe {
                    out.write(id);
                }
            }
            SUCCESS
        }
        Err(error) => {
            let mut text = image
                .last_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *text = error.message.into_owned();
            HeifError {
                code: error.code,
                subcode: error.subcode,
                message: text.as_ptr(),
            }
        }
    }
}
macro_rules! access {
    ($read:ident, $write:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $read(
            image: *const Image,
            id: u32,
            stride: *mut usize,
        ) -> *const $ty {
            let plane = unsafe { image.as_ref() }.and_then(|i| i.component_plane(id));
            if !stride.is_null() {
                unsafe {
                    stride.write(plane.map_or(0, |p| p.stride / std::mem::size_of::<$ty>()));
                }
            }
            plane.map_or(ptr::null(), |p| p.data().as_ptr().cast())
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $write(
            image: *mut Image,
            id: u32,
            stride: *mut usize,
        ) -> *mut $ty {
            let plane = unsafe { image.as_mut() }.and_then(|i| i.component_plane_mut(id));
            if !stride.is_null() {
                unsafe {
                    stride.write(
                        plane
                            .as_ref()
                            .map_or(0, |p| p.stride / std::mem::size_of::<$ty>()),
                    );
                }
            }
            plane.map_or(ptr::null_mut(), |p| p.data_mut().as_mut_ptr().cast())
        }
    };
}
access!(
    heif_image_get_component_readonly,
    heif_image_get_component,
    u8
);
access!(
    heif_image_get_component_uint16_readonly,
    heif_image_get_component_uint16,
    u16
);
access!(
    heif_image_get_component_uint32_readonly,
    heif_image_get_component_uint32,
    u32
);
access!(
    heif_image_get_component_uint64_readonly,
    heif_image_get_component_uint64,
    u64
);
access!(
    heif_image_get_component_int8_readonly,
    heif_image_get_component_int8,
    i8
);
access!(
    heif_image_get_component_int16_readonly,
    heif_image_get_component_int16,
    i16
);
access!(
    heif_image_get_component_int32_readonly,
    heif_image_get_component_int32,
    i32
);
access!(
    heif_image_get_component_int64_readonly,
    heif_image_get_component_int64,
    i64
);
access!(
    heif_image_get_component_float32_readonly,
    heif_image_get_component_float32,
    f32
);
access!(
    heif_image_get_component_float64_readonly,
    heif_image_get_component_float64,
    f64
);
access!(
    heif_image_get_component_complex32_readonly,
    heif_image_get_component_complex32,
    Complex32
);
access!(
    heif_image_get_component_complex64_readonly,
    heif_image_get_component_complex64,
    Complex64
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_gimi_component_content_id(
    image: *mut Image,
    id: u32,
    content: *const c_char,
) -> HeifError {
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    if content.is_null() {
        return Error::NULL.into();
    }
    let Some(desc) = image.component_ids.find_mut(id) else {
        return Error::new(
            5,
            2006,
            c"No component with the requested component_id exists.",
        )
        .into();
    };
    let bytes = unsafe { CStr::from_ptr(content) }.to_bytes();
    let mut data = Vec::new();
    if data.try_reserve_exact(bytes.len()).is_err() {
        return Error::ALLOCATION.into();
    }
    data.extend_from_slice(bytes);
    desc.content_id = data;
    SUCCESS
}
