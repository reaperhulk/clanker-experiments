// SPDX-License-Identifier: LGPL-3.0-or-later
//! Public plugin ABI layouts. Foreign records are read by versioned raw-field
//! accessors so historical callers need not allocate the current full structure.
use crate::{HeifError, encoding_options::SequenceEncodingOptions, security::SecurityLimits};
use libheifer::image::Image;
use std::ffi::{c_char, c_int, c_void};
pub type Name = Option<unsafe extern "C" fn() -> *const c_char>;
pub type Initialize = Option<unsafe extern "C" fn()>;
pub type Allocate = Option<unsafe extern "C" fn(*mut *mut c_void) -> HeifError>;
pub type Release = Option<unsafe extern "C" fn(*mut c_void)>;
pub type SetInt = Option<unsafe extern "C" fn(*mut c_void, c_int) -> HeifError>;
pub type GetInt = Option<unsafe extern "C" fn(*mut c_void, *mut c_int) -> HeifError>;
pub type SetNamedInt = Option<unsafe extern "C" fn(*mut c_void, *const c_char, c_int) -> HeifError>;
pub type GetNamedInt =
    Option<unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_int) -> HeifError>;
pub type SetString =
    Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> HeifError>;
pub type GetString =
    Option<unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_char, c_int) -> HeifError>;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IntegerParameter {
    pub default_value: c_int,
    pub have_minimum_maximum: u8,
    pub minimum: c_int,
    pub maximum: c_int,
    pub valid_values: *mut c_int,
    pub num_valid_values: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StringParameter {
    pub default_value: *const c_char,
    pub valid_values: *const *const c_char,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BooleanParameter {
    pub default_value: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union ParameterValue {
    pub integer: IntegerParameter,
    pub string: StringParameter,
    pub boolean: BooleanParameter,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EncoderParameter {
    pub version: c_int,
    pub name: *const c_char,
    pub kind: c_int,
    pub value: ParameterValue,
    pub has_default: c_int,
}
#[repr(C)]
pub struct EncoderPlugin {
    pub plugin_api_version: c_int,
    pub compression_format: c_int,
    pub id_name: *const c_char,
    pub priority: c_int,
    pub supports_lossy_compression: c_int,
    pub supports_lossless_compression: c_int,
    pub get_plugin_name: Name,
    pub init_plugin: Initialize,
    pub cleanup_plugin: Initialize,
    pub new_encoder: Allocate,
    pub free_encoder: Release,
    pub set_parameter_quality: SetInt,
    pub get_parameter_quality: GetInt,
    pub set_parameter_lossless: SetInt,
    pub get_parameter_lossless: GetInt,
    pub set_parameter_logging_level: SetInt,
    pub get_parameter_logging_level: GetInt,
    pub list_parameters:
        Option<unsafe extern "C" fn(*mut c_void) -> *const *const EncoderParameter>,
    pub set_parameter_integer: SetNamedInt,
    pub get_parameter_integer: GetNamedInt,
    pub set_parameter_boolean: SetNamedInt,
    pub get_parameter_boolean: GetNamedInt,
    pub set_parameter_string: SetString,
    pub get_parameter_string: GetString,
    pub query_input_colorspace: Option<unsafe extern "C" fn(*mut c_int, *mut c_int)>,
    pub encode_image: Option<unsafe extern "C" fn(*mut c_void, *const Image, c_int) -> HeifError>,
    pub get_compressed_data: Option<
        unsafe extern "C" fn(*mut c_void, *mut *mut u8, *mut c_int, *mut c_int) -> HeifError,
    >,
    pub query_input_colorspace2: Option<unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int)>,
    pub query_encoded_size: Option<unsafe extern "C" fn(*mut c_void, u32, u32, *mut u32, *mut u32)>,
    pub minimum_required_libheif_version: u32,
    pub start_sequence_encoding: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const Image,
            c_int,
            u32,
            u32,
            *const SequenceEncodingOptions,
        ) -> HeifError,
    >,
    pub encode_sequence_frame:
        Option<unsafe extern "C" fn(*mut c_void, *const Image, usize) -> HeifError>,
    pub end_sequence_encoding: Option<unsafe extern "C" fn(*mut c_void) -> HeifError>,
    pub get_compressed_data2: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *mut *mut u8,
            *mut c_int,
            *mut usize,
            *mut c_int,
            *mut c_int,
        ) -> HeifError,
    >,
    pub does_indicate_keyframes: c_int,
}
#[repr(C)]
pub struct CompressedFormatDescription {
    pub format: c_int,
}
#[repr(C)]
pub struct DecoderPluginOptions {
    pub format: c_int,
    pub strict_decoding: c_int,
    pub num_threads: c_int,
    pub limits: *const SecurityLimits,
}
#[repr(C)]
pub struct DecoderPlugin {
    pub plugin_api_version: c_int,
    pub get_plugin_name: Name,
    pub init_plugin: Initialize,
    pub deinit_plugin: Initialize,
    pub does_support_format: Option<unsafe extern "C" fn(c_int) -> c_int>,
    pub new_decoder: Allocate,
    pub free_decoder: Release,
    pub push_data: Option<unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> HeifError>,
    pub decode_image: Option<unsafe extern "C" fn(*mut c_void, *mut *mut Image) -> HeifError>,
    pub set_strict_decoding: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub id_name: *const c_char,
    pub decode_next_image: Option<
        unsafe extern "C" fn(*mut c_void, *mut *mut Image, *const SecurityLimits) -> HeifError,
    >,
    pub minimum_required_libheif_version: u32,
    pub does_support_format2:
        Option<unsafe extern "C" fn(*const CompressedFormatDescription) -> c_int>,
    pub new_decoder2:
        Option<unsafe extern "C" fn(*mut *mut c_void, *const DecoderPluginOptions) -> HeifError>,
    pub push_data2:
        Option<unsafe extern "C" fn(*mut c_void, *const c_void, usize, usize) -> HeifError>,
    pub flush_data: Option<unsafe extern "C" fn(*mut c_void) -> HeifError>,
    pub decode_next_image2: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *mut *mut Image,
            *mut usize,
            *const SecurityLimits,
        ) -> HeifError,
    >,
}
#[repr(C)]
pub struct PluginInfo {
    pub version: c_int,
    pub kind: c_int,
    pub plugin: *const c_void,
    pub internal_handle: *mut c_void,
}
