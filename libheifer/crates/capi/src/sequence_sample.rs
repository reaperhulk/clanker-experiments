// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{HeifError, SUCCESS};
use libheifer::{error::Error, image::Image, sequence_sample::RawSample, tai::Timestamp};
use std::{
    ffi::{CStr, c_char, c_int},
    ptr,
};

#[unsafe(no_mangle)]
pub extern "C" fn heif_raw_sequence_sample_alloc() -> *mut RawSample {
    super::color::allocate(RawSample::default())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_raw_sequence_sample_release(sample: *mut RawSample) {
    if !sample.is_null() {
        drop(unsafe { Box::from_raw(sample) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_raw_sequence_sample_get_data(
    sample: *const RawSample,
    size: *mut usize,
) -> *const u8 {
    let Some(sample) = (unsafe { sample.as_ref() }) else {
        return ptr::null();
    };
    if !size.is_null() {
        unsafe {
            size.write(sample.data().len());
        }
    }
    if sample.has_storage() {
        sample.data().as_ptr()
    } else {
        ptr::null()
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_raw_sequence_sample_get_data_size(sample: *const RawSample) -> usize {
    unsafe { sample.as_ref() }.map_or(0, |s| s.data().len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_raw_sequence_sample_set_data(
    sample: *mut RawSample,
    data: *const u8,
    size: usize,
) -> HeifError {
    let Some(sample) = (unsafe { sample.as_mut() }) else {
        return Error::NULL.into();
    };
    if size > isize::MAX as usize {
        return Error::ALLOCATION.into();
    }
    let bytes = if size == 0 {
        &[]
    } else if data.is_null() {
        return Error::NULL.into();
    } else {
        unsafe { std::slice::from_raw_parts(data, size) }
    };
    match sample.set_data(bytes) {
        Ok(()) => SUCCESS,
        Err(e) => e.into(),
    }
}
macro_rules! metadata {
    ($ty:ty, $duration_get:ident, $duration_set:ident, $id_get:ident, $id_set:ident, $field:ident, $empty_null:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $duration_get(value: *const $ty) -> u32 {
            unsafe { value.as_ref() }.map_or(0, |v| v.$field.duration)
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $duration_set(value: *mut $ty, duration: u32) {
            if let Some(v) = unsafe { value.as_mut() } {
                v.$field.duration = duration;
            }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $id_get(value: *const $ty) -> *const c_char {
            let Some(v) = (unsafe { value.as_ref() }) else {
                return ptr::null();
            };
            if $empty_null && v.$field.content_id.is_empty() {
                return ptr::null();
            }
            libheifer::gimi::c_string(&v.$field.content_id).into_raw()
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $id_set(value: *mut $ty, id: *const c_char) {
            if let Some(v) = unsafe { value.as_mut() } {
                v.$field.content_id = if id.is_null() {
                    Vec::new()
                } else {
                    unsafe { CStr::from_ptr(id) }.to_bytes().to_vec()
                };
            }
        }
    };
}
metadata!(
    RawSample,
    heif_raw_sequence_sample_get_duration,
    heif_raw_sequence_sample_set_duration,
    heif_raw_sequence_sample_get_gimi_sample_content_id,
    heif_raw_sequence_sample_set_gimi_sample_content_id,
    metadata,
    false
);
metadata!(
    Image,
    heif_image_get_duration,
    heif_image_set_duration,
    heif_image_get_gimi_sample_content_id,
    heif_image_set_gimi_sample_content_id,
    sample,
    true
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_raw_sequence_sample_has_tai_timestamp(
    sample: *const RawSample,
) -> c_int {
    unsafe { sample.as_ref() }
        .is_some_and(|s| s.timestamp.is_some())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_raw_sequence_sample_get_tai_timestamp(
    sample: *const RawSample,
) -> *const Timestamp {
    unsafe { sample.as_ref() }
        .and_then(|s| s.timestamp.as_deref())
        .map_or(ptr::null(), ptr::from_ref)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_raw_sequence_sample_set_tai_timestamp(
    sample: *mut RawSample,
    timestamp: *const Timestamp,
) {
    let Some(sample) = (unsafe { sample.as_mut() }) else {
        return;
    };
    let mut copy = Box::new(Timestamp::default());
    // Copy only the fields present in the caller's version. NULL input is
    // undefined upstream; this implementation safely creates a default packet.
    unsafe {
        super::tai::heif_tai_timestamp_packet_copy(&mut *copy, timestamp);
    }
    sample.timestamp = Some(copy);
}
