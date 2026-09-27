// SPDX-License-Identifier: LGPL-3.0-or-later
// Compatibility semantics adapted from libheif, Copyright Dirk Farin and contributors.
use libheifer::color::{ColorConversionOptions, NclxProfile};
use std::{ffi::c_int, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnciParameters {
    pub version: c_int,
    pub image_width: u32,
    pub image_height: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub compression: c_int,
}
impl Default for UnciParameters {
    fn default() -> Self {
        Self {
            version: 1,
            image_width: 0,
            image_height: 0,
            tile_width: 0,
            tile_height: 0,
            compression: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
pub struct EncodingOptions {
    pub version: u8,
    pub save_alpha_channel: u8,
    pub macOS_compatibility_workaround: u8,
    pub save_two_colr_boxes_when_ICC_and_nclx_available: u8,
    pub output_nclx_profile: *mut NclxProfile,
    pub macOS_compatibility_workaround_no_nclx_profile: u8,
    pub image_orientation: c_int,
    pub color_conversion_options: ColorConversionOptions,
    pub prefer_uncC_short_form: u8,
    pub unci_parameters: *const UnciParameters,
}
fn conversion() -> ColorConversionOptions {
    ColorConversionOptions {
        version: 1,
        preferred_chroma_downsampling_algorithm: 2,
        preferred_chroma_upsampling_algorithm: 2,
        only_use_preferred_chroma_algorithm: 0,
    }
}
impl Default for EncodingOptions {
    fn default() -> Self {
        Self {
            version: 8,
            save_alpha_channel: 1,
            macOS_compatibility_workaround: 0,
            save_two_colr_boxes_when_ICC_and_nclx_available: 0,
            output_nclx_profile: ptr::null_mut(),
            macOS_compatibility_workaround_no_nclx_profile: 0,
            image_orientation: 1,
            color_conversion_options: conversion(),
            prefer_uncC_short_form: 1,
            unci_parameters: ptr::null(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SequenceEncodingOptions {
    pub version: u8,
    pub output_nclx_profile: *const NclxProfile,
    pub color_conversion_options: ColorConversionOptions,
    pub gop_structure: c_int,
    pub keyframe_distance_min: c_int,
    pub keyframe_distance_max: c_int,
    pub save_alpha_channel: c_int,
    pub content_kind: c_int,
}
impl Default for SequenceEncodingOptions {
    fn default() -> Self {
        Self {
            version: 3,
            output_nclx_profile: ptr::null(),
            color_conversion_options: conversion(),
            gop_structure: 1,
            keyframe_distance_min: 0,
            keyframe_distance_max: 0,
            save_alpha_channel: 1,
            content_kind: 0,
        }
    }
}
macro_rules! versioned_options {
    ($ty:ty,$alloc:ident,$copy:ident,$free:ident,$max:expr,$($version:literal=>[$($field:ident),+]),+) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $alloc()->*mut $ty {super::color::allocate(<$ty>::default())}
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $free(value:*mut $ty) {if !value.is_null() {drop(unsafe {Box::from_raw(value)});}}
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $copy(dst:*mut $ty,src:*const $ty) {
            if dst.is_null() || src.is_null() {return;}
            // Read only the shared version prefix; old callers may allocate
            // only their historical fields. Unknown minimum versions are no-op.
            let version=unsafe {ptr::addr_of!((*dst).version).read().min(ptr::addr_of!((*src).version).read())};
            if !(1..=$max).contains(&version) {return;}
            $(if version >= $version {$(let value=unsafe {ptr::addr_of!((*src).$field).read()};unsafe {ptr::addr_of_mut!((*dst).$field).write(value);})+})+
        }
    };
}
versioned_options!(EncodingOptions,heif_encoding_options_alloc,heif_encoding_options_copy,heif_encoding_options_free,8,
    1=>[save_alpha_channel],2=>[macOS_compatibility_workaround],3=>[save_two_colr_boxes_when_ICC_and_nclx_available],4=>[output_nclx_profile,macOS_compatibility_workaround_no_nclx_profile],5=>[image_orientation],6=>[color_conversion_options],7=>[prefer_uncC_short_form],8=>[unci_parameters]);
versioned_options!(SequenceEncodingOptions,heif_sequence_encoding_options_alloc,heif_sequence_encoding_options_copy,heif_sequence_encoding_options_release,3,
    1=>[output_nclx_profile,color_conversion_options],2=>[gop_structure,keyframe_distance_min,keyframe_distance_max,save_alpha_channel],3=>[content_kind]);
versioned_options!(UnciParameters,heif_unci_image_parameters_alloc,heif_unci_image_parameters_copy,heif_unci_image_parameters_release,1,
    1=>[image_width,image_height,tile_width,tile_height,compression]);
#[unsafe(no_mangle)]
pub extern "C" fn heif_orientation_concat(first: c_int, second: c_int) -> c_int {
    libheifer::geometry::orientation_concat(first, second)
}
