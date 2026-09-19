// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{HeifError, SUCCESS, plugin_types::EncoderParameter};
use libheifer::error::Error;
use std::{
    ffi::{c_char, c_int},
    ptr,
};
pub(super) fn unsupported() -> HeifError {
    Error::new(5, 2005, c"Unsupported encoder parameter").into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_parameter_get_name(
    p: *const EncoderParameter,
) -> *const c_char {
    if p.is_null() {
        return ptr::null();
    }
    unsafe { ptr::addr_of!((*p).name).read() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_parameter_get_type(p: *const EncoderParameter) -> c_int {
    if p.is_null() {
        return 0;
    }
    unsafe { ptr::addr_of!((*p).kind).read() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_parameter_get_valid_integer_range(
    p: *const EncoderParameter,
    have: *mut c_int,
    minimum: *mut c_int,
    maximum: *mut c_int,
) -> HeifError {
    if p.is_null() {
        return Error::NULL.into();
    }
    if unsafe { ptr::addr_of!((*p).kind).read() } != 1 {
        return unsupported();
    }
    let have_range = unsafe { ptr::addr_of!((*p).value.integer.have_minimum_maximum).read() };
    if have_range != 0 {
        if !minimum.is_null() {
            unsafe { minimum.write(ptr::addr_of!((*p).value.integer.minimum).read()) }
        }
        if !maximum.is_null() {
            unsafe { maximum.write(ptr::addr_of!((*p).value.integer.maximum).read()) }
        }
    }
    if !have.is_null() {
        unsafe { have.write(have_range.into()) }
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_parameter_get_valid_integer_values(
    p: *const EncoderParameter,
    have_min: *mut c_int,
    have_max: *mut c_int,
    minimum: *mut c_int,
    maximum: *mut c_int,
    count: *mut c_int,
    array: *mut *const c_int,
) -> HeifError {
    if p.is_null() {
        return Error::NULL.into();
    }
    if unsafe { ptr::addr_of!((*p).kind).read() } != 1 {
        return unsupported();
    }
    let have_range = unsafe { ptr::addr_of!((*p).value.integer.have_minimum_maximum).read() };
    if have_range != 0 {
        if !minimum.is_null() {
            unsafe { minimum.write(ptr::addr_of!((*p).value.integer.minimum).read()) }
        }
        if !maximum.is_null() {
            unsafe { maximum.write(ptr::addr_of!((*p).value.integer.maximum).read()) }
        }
    }
    let num_values = unsafe { ptr::addr_of!((*p).value.integer.num_valid_values).read() };
    if !have_min.is_null() {
        unsafe { have_min.write(have_range.into()) }
    }
    if !have_max.is_null() {
        unsafe { have_max.write(have_range.into()) }
    }
    if num_values > 0 && !array.is_null() {
        unsafe { array.write(ptr::addr_of!((*p).value.integer.valid_values).read()) }
    }
    if !count.is_null() {
        unsafe { count.write(num_values) }
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_parameter_get_valid_string_values(
    p: *const EncoderParameter,
    array: *mut *const *const c_char,
) -> HeifError {
    if p.is_null() {
        return Error::NULL.into();
    }
    if unsafe { ptr::addr_of!((*p).kind).read() } != 3 {
        return unsupported();
    }
    if !array.is_null() {
        unsafe { array.write(ptr::addr_of!((*p).value.string.valid_values).read()) }
    }
    SUCCESS
}
