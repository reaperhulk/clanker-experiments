// SPDX-License-Identifier: LGPL-3.0-or-later
//! The built-in JPEG encoder as a static encoder-plugin record, with the
//! callback semantics of libheif's libjpeg plugin (encoder_jpeg.cc). The
//! compressed data comes from the pure Rust `libheifer::jpeg_encoder`.
use crate::{HeifError, plugin_types::*};
use libheifer::image::Image;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

struct State {
    quality: c_int,
    compressed: Vec<u8>,
    read: bool,
}

const fn error(code: c_int, subcode: c_int, message: &'static CStr) -> HeifError {
    HeifError {
        code,
        subcode,
        message: message.as_ptr(),
    }
}
const OK: HeifError = error(0, 0, c"Success");
const INVALID_VALUE: HeifError = error(5, 2006, c"Invalid parameter value");
const UNSUPPORTED: HeifError = error(5, 2005, c"Unsupported encoder parameter");

struct Parameter(EncoderParameter);
// SAFETY: the record holds only static strings.
unsafe impl Sync for Parameter {}
static QUALITY: Parameter = Parameter(EncoderParameter {
    version: 2,
    name: c"quality".as_ptr(),
    kind: 1,
    value: ParameterValue {
        integer: IntegerParameter {
            default_value: 50,
            have_minimum_maximum: 1,
            minimum: 0,
            maximum: 100,
            valid_values: ptr::null_mut(),
            num_valid_values: 0,
        },
    },
    has_default: 1,
});
struct List([*const EncoderParameter; 2]);
// SAFETY: points only to the immutable static parameter.
unsafe impl Sync for List {}
static PARAMETERS: List = List([&QUALITY.0, ptr::null()]);

fn named(name: *const c_char, expected: &CStr) -> bool {
    !name.is_null() && unsafe { CStr::from_ptr(name) } == expected
}
unsafe fn state<'a>(p: *mut c_void) -> &'a mut State {
    unsafe { &mut *p.cast::<State>() }
}

unsafe extern "C" fn name() -> *const c_char {
    c"libheifer JPEG encoder".as_ptr()
}
unsafe extern "C" fn new_encoder(out: *mut *mut c_void) -> HeifError {
    let state = Box::new(State {
        quality: 50,
        compressed: Vec::new(),
        read: false,
    });
    unsafe { out.write(Box::into_raw(state).cast()) };
    OK
}
unsafe extern "C" fn free_encoder(p: *mut c_void) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(p.cast::<State>()) });
    }
}
unsafe extern "C" fn set_quality(p: *mut c_void, quality: c_int) -> HeifError {
    if !(0..=100).contains(&quality) {
        return INVALID_VALUE;
    }
    unsafe { state(p) }.quality = quality;
    OK
}
unsafe extern "C" fn get_quality(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(state(p).quality) };
    OK
}
unsafe extern "C" fn set_lossless(p: *mut c_void, enable: c_int) -> HeifError {
    // Not lossless: libheif's plugin maps it to quality 100.
    if enable != 0 {
        unsafe { state(p) }.quality = 100;
    }
    OK
}
unsafe extern "C" fn get_lossless(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(c_int::from(state(p).quality == 100)) };
    OK
}
unsafe extern "C" fn set_logging(_: *mut c_void, _: c_int) -> HeifError {
    OK
}
unsafe extern "C" fn get_logging(_: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(0) };
    OK
}
unsafe extern "C" fn list_parameters(_: *mut c_void) -> *const *const EncoderParameter {
    PARAMETERS.0.as_ptr()
}
unsafe extern "C" fn set_integer(p: *mut c_void, name: *const c_char, value: c_int) -> HeifError {
    if named(name, c"quality") {
        unsafe { set_quality(p, value) }
    } else if named(name, c"lossless") {
        unsafe { set_lossless(p, value) }
    } else {
        UNSUPPORTED
    }
}
unsafe extern "C" fn get_integer(
    p: *mut c_void,
    name: *const c_char,
    out: *mut c_int,
) -> HeifError {
    if named(name, c"quality") {
        unsafe { get_quality(p, out) }
    } else if named(name, c"lossless") {
        unsafe { get_lossless(p, out) }
    } else {
        UNSUPPORTED
    }
}
unsafe extern "C" fn set_boolean(p: *mut c_void, name: *const c_char, value: c_int) -> HeifError {
    if named(name, c"lossless") {
        unsafe { set_lossless(p, value) }
    } else {
        UNSUPPORTED
    }
}
unsafe extern "C" fn get_boolean(
    p: *mut c_void,
    name: *const c_char,
    out: *mut c_int,
) -> HeifError {
    if named(name, c"lossless") {
        unsafe { get_lossless(p, out) }
    } else {
        UNSUPPORTED
    }
}
unsafe extern "C" fn set_string(_: *mut c_void, _: *const c_char, _: *const c_char) -> HeifError {
    UNSUPPORTED
}
unsafe extern "C" fn get_string(
    _: *mut c_void,
    _: *const c_char,
    _: *mut c_char,
    _: c_int,
) -> HeifError {
    UNSUPPORTED
}
unsafe extern "C" fn query_input(colorspace: *mut c_int, chroma: *mut c_int) {
    unsafe {
        colorspace.write(0);
        chroma.write(1);
    }
}
unsafe extern "C" fn query_input2(_: *mut c_void, colorspace: *mut c_int, chroma: *mut c_int) {
    unsafe { query_input(colorspace, chroma) }
}
unsafe extern "C" fn query_size(_: *mut c_void, w: u32, h: u32, ow: *mut u32, oh: *mut u32) {
    unsafe {
        ow.write(w);
        oh.write(h);
    }
}

/// `check_encoder_input_image` (no monochrome, 8 bits) and the 8-bit storage check.
fn check_input(image: &Image) -> Result<(), HeifError> {
    if image.colorspace == 2 {
        return Err(error(8, 3001, c"Encoder cannot encode monochrome images"));
    }
    if image.colorspace != 0 {
        return Err(error(
            8,
            3001,
            c"Encoder can only encode YCbCr and monochrome images",
        ));
    }
    let planes: Vec<_> = (0..3).map(|c| image.plane(c)).collect();
    if planes.iter().any(Option::is_none) {
        return Err(error(
            8,
            3001,
            c"Input image is missing one of its color channels",
        ));
    }
    let depth = planes[0].unwrap().bit_depth;
    if planes.iter().any(|p| p.unwrap().bit_depth != depth) {
        return Err(error(
            8,
            4000,
            c"Encoder cannot encode images in which the color channels have different bit depths",
        ));
    }
    if depth != 8 {
        return Err(error(
            8,
            4000,
            c"Encoder cannot encode images at this bit depth",
        ));
    }
    if planes[0].unwrap().storage_bits() != 8 {
        return Err(error(9, 5002, c"Cannot write JPEG image with >8 bpp."));
    }
    Ok(())
}

unsafe extern "C" fn encode_image(
    p: *mut c_void,
    image: *const Image,
    input_class: c_int,
) -> HeifError {
    let image = unsafe { &*image };
    if let Err(e) = check_input(image) {
        return e;
    }
    let state = unsafe { state(p) };
    let plane = |c| {
        let p = image.plane(c).unwrap();
        libheifer::jpeg_encoder::PlaneRef {
            data: p.data(),
            stride: p.stride,
            width: p.width as usize,
            height: p.height as usize,
        }
    };
    let (h, v) = image.pixel_aspect_ratio;
    // The pixel aspect ratio goes to the JFIF density of normal and thumbnail images.
    let density = if matches!(input_class, 1 | 4)
        && h != v
        && (1..=0xFFFF).contains(&h)
        && (1..=0xFFFF).contains(&v)
    {
        (h as u16, v as u16)
    } else {
        (1, 1)
    };
    state.compressed =
        libheifer::jpeg_encoder::encode(&plane(0), &plane(1), &plane(2), state.quality, density);
    state.read = false;
    OK
}
unsafe extern "C" fn compressed_data(
    p: *mut c_void,
    data: *mut *mut u8,
    size: *mut c_int,
    _: *mut c_int,
) -> HeifError {
    let state = unsafe { state(p) };
    unsafe {
        if state.read {
            data.write(ptr::null_mut());
            size.write(0);
        } else {
            data.write(state.compressed.as_mut_ptr());
            size.write(state.compressed.len() as c_int);
            state.read = true;
        }
    }
    OK
}
unsafe extern "C" fn start_sequence(
    _: *mut c_void,
    _: *const Image,
    _: c_int,
    _: u32,
    _: u32,
    _: *const crate::encoding_options::SequenceEncodingOptions,
) -> HeifError {
    OK
}
unsafe extern "C" fn encode_frame(p: *mut c_void, image: *const Image, _: usize) -> HeifError {
    unsafe { encode_image(p, image, 1) }
}
unsafe extern "C" fn end_sequence(_: *mut c_void) -> HeifError {
    OK
}
unsafe extern "C" fn compressed_data2(
    p: *mut c_void,
    data: *mut *mut u8,
    size: *mut c_int,
    _frame: *mut usize,
    keyframe: *mut c_int,
    more: *mut c_int,
) -> HeifError {
    let result = unsafe { compressed_data(p, data, size, ptr::null_mut()) };
    unsafe {
        if !keyframe.is_null() {
            keyframe.write(1);
        }
        if !more.is_null() {
            more.write(1);
        }
    }
    result
}

pub(crate) struct Record(pub EncoderPlugin);
// SAFETY: immutable record of static strings and function pointers.
unsafe impl Sync for Record {}
pub(crate) static JPEG_ENCODER: Record = Record(EncoderPlugin {
    plugin_api_version: 4,
    compression_format: 3,
    id_name: c"libheifer-jpeg".as_ptr(),
    priority: 100,
    supports_lossy_compression: 1,
    supports_lossless_compression: 0,
    get_plugin_name: Some(name),
    init_plugin: None,
    cleanup_plugin: None,
    new_encoder: Some(new_encoder),
    free_encoder: Some(free_encoder),
    set_parameter_quality: Some(set_quality),
    get_parameter_quality: Some(get_quality),
    set_parameter_lossless: Some(set_lossless),
    get_parameter_lossless: Some(get_lossless),
    set_parameter_logging_level: Some(set_logging),
    get_parameter_logging_level: Some(get_logging),
    list_parameters: Some(list_parameters),
    set_parameter_integer: Some(set_integer),
    get_parameter_integer: Some(get_integer),
    set_parameter_boolean: Some(set_boolean),
    get_parameter_boolean: Some(get_boolean),
    set_parameter_string: Some(set_string),
    get_parameter_string: Some(get_string),
    query_input_colorspace: Some(query_input),
    encode_image: Some(encode_image),
    get_compressed_data: Some(compressed_data),
    query_input_colorspace2: Some(query_input2),
    query_encoded_size: Some(query_size),
    minimum_required_libheif_version: 0x0115_0000,
    start_sequence_encoding: Some(start_sequence),
    encode_sequence_frame: Some(encode_frame),
    end_sequence_encoding: Some(end_sequence),
    get_compressed_data2: Some(compressed_data2),
    does_indicate_keyframes: 1,
});
