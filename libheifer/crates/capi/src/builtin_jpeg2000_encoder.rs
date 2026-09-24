// SPDX-License-Identifier: LGPL-3.0-or-later
//! The built-in JPEG2000 encoder as a static encoder-plugin record, with the
//! callback semantics of libheif's OpenJPEG plugin (encoder_openjpeg.cc).
//! The codestream comes from the pure Rust `libheifer::jpeg2000_encoder`.
use crate::{HeifError, plugin_types::*};
use libheifer::image::Image;
use libheifer::jpeg2000_encoder::{Component, EncodeError, Settings};
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

struct State {
    quality: c_int,
    /// `parameters.irreversible`; OpenJPEG's defaults leave it lossless.
    irreversible: bool,
    /// The `chroma` parameter (heif_chroma, -1 when undefined).
    chroma: c_int,
    codestream: Vec<u8>,
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

struct Strings([*const c_char; 4]);
// SAFETY: static strings only.
unsafe impl Sync for Strings {}
static CHROMA_VALUES: Strings = Strings([
    c"420".as_ptr(),
    c"422".as_ptr(),
    c"444".as_ptr(),
    ptr::null(),
]);

struct Parameter(EncoderParameter);
// SAFETY: the record holds only static strings.
unsafe impl Sync for Parameter {}
static CHROMA: Parameter = Parameter(EncoderParameter {
    version: 2,
    name: c"chroma".as_ptr(),
    kind: 2,
    value: ParameterValue {
        string: StringParameter {
            default_value: ptr::null(),
            valid_values: CHROMA_VALUES.0.as_ptr(),
        },
    },
    has_default: 0,
});
struct List([*const EncoderParameter; 2]);
// SAFETY: points only to the immutable static parameter.
unsafe impl Sync for List {}
static PARAMETERS: List = List([&CHROMA.0, ptr::null()]);

fn named(name: *const c_char, expected: &CStr) -> bool {
    !name.is_null() && unsafe { CStr::from_ptr(name) } == expected
}
unsafe fn state<'a>(p: *mut c_void) -> &'a mut State {
    unsafe { &mut *p.cast::<State>() }
}

unsafe extern "C" fn name() -> *const c_char {
    c"libheifer JPEG2000 encoder".as_ptr()
}
unsafe extern "C" fn new_encoder(out: *mut *mut c_void) -> HeifError {
    let state = Box::new(State {
        quality: 70,
        irreversible: false,
        chroma: -1,
        codestream: Vec::new(),
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
    unsafe { state(p) }.irreversible = enable == 0;
    OK
}
unsafe extern "C" fn get_lossless(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(c_int::from(!state(p).irreversible)) };
    OK
}
unsafe extern "C" fn set_logging(_: *mut c_void, _: c_int) -> HeifError {
    OK
}
/// Like the plugin, succeeds without writing the level.
unsafe extern "C" fn get_logging(_: *mut c_void, _: *mut c_int) -> HeifError {
    OK
}
unsafe extern "C" fn list_parameters(_: *mut c_void) -> *const *const EncoderParameter {
    PARAMETERS.0.as_ptr()
}
unsafe extern "C" fn set_integer(p: *mut c_void, name: *const c_char, value: c_int) -> HeifError {
    if named(name, c"quality") {
        unsafe { set_quality(p, value) }
    } else {
        UNSUPPORTED
    }
}
/// Unknown names succeed without writing, as in the plugin.
unsafe extern "C" fn get_integer(
    p: *mut c_void,
    name: *const c_char,
    out: *mut c_int,
) -> HeifError {
    if named(name, c"quality") {
        unsafe { get_quality(p, out) }
    } else {
        OK
    }
}
unsafe extern "C" fn set_boolean(_: *mut c_void, _: *const c_char, _: c_int) -> HeifError {
    OK
}
unsafe extern "C" fn get_boolean(_: *mut c_void, _: *const c_char, _: *mut c_int) -> HeifError {
    OK
}
unsafe extern "C" fn set_string(
    p: *mut c_void,
    name: *const c_char,
    value: *const c_char,
) -> HeifError {
    if !named(name, c"chroma") {
        return UNSUPPORTED;
    }
    let chroma = if named(value, c"420") {
        1
    } else if named(value, c"422") {
        2
    } else if named(value, c"444") {
        3
    } else {
        return INVALID_VALUE;
    };
    unsafe { state(p) }.chroma = chroma;
    OK
}
unsafe extern "C" fn get_string(
    p: *mut c_void,
    name: *const c_char,
    out: *mut c_char,
    size: c_int,
) -> HeifError {
    if !named(name, c"chroma") {
        return UNSUPPORTED;
    }
    let text: &[u8] = match unsafe { state(p) }.chroma {
        1 => b"420",
        2 => b"422",
        3 => b"444",
        _ => b"undefined",
    };
    // save_strcpy: strncpy of size - 1 bytes, then a terminating NUL.
    if size > 0 && !out.is_null() {
        let room = (size - 1) as usize;
        let n = text.len().min(room);
        unsafe {
            ptr::copy_nonoverlapping(text.as_ptr().cast::<c_char>(), out, n);
            for i in n..room {
                out.add(i).write(0);
            }
            out.add(room).write(0);
        }
    }
    OK
}
unsafe extern "C" fn query_input(colorspace: *mut c_int, chroma: *mut c_int) {
    unsafe {
        if colorspace.read() == 2 {
            chroma.write(0);
        } else {
            colorspace.write(0);
            chroma.write(3);
        }
    }
}
unsafe extern "C" fn query_input2(p: *mut c_void, colorspace: *mut c_int, chroma: *mut c_int) {
    unsafe {
        if colorspace.read() == 2 {
            chroma.write(0);
        } else {
            colorspace.write(0);
            let wanted = state(p).chroma;
            chroma.write(if wanted != -1 { wanted } else { 3 });
        }
    }
}
unsafe extern "C" fn query_size(_: *mut c_void, w: u32, h: u32, ow: *mut u32, oh: *mut u32) {
    unsafe {
        ow.write(w);
        oh.write(h);
    }
}

unsafe extern "C" fn encode_image(
    p: *mut c_void,
    image: *const Image,
    _input_class: c_int,
) -> HeifError {
    let image = unsafe { &*image };
    let channels: &[i32] = match image.colorspace {
        0 => &[0, 1, 2],
        1 => &[3, 4, 5],
        2 => &[0],
        _ => {
            return error(
                9,
                0,
                c"OpenJPEG encoder plugin received image with invalid colorspace.",
            );
        }
    };
    let (sub_dx, sub_dy) = match image.chroma {
        1 => (2, 2),
        2 => (2, 1),
        _ => (1, 1),
    };
    let width = image.width;
    let height = image.height;
    let mut planes = Vec::new();
    let mut dims = Vec::new();
    for (comp, &channel) in channels.iter().enumerate() {
        let Some(plane) = image.plane(channel) else {
            return error(9, 0, c"Failed create OpenJPEG image");
        };
        let (dx, dy) = if comp == 0 { (1, 1) } else { (sub_dx, sub_dy) };
        let cw = if comp == 0 {
            width
        } else {
            (width + dx / 2) / dx
        };
        let ch = if comp == 0 {
            height
        } else {
            (height + dy / 2) / dy
        };
        let data = plane.data();
        let mut samples = Vec::with_capacity((cw * ch) as usize);
        for y in 0..ch as usize {
            for x in 0..cw as usize {
                samples.push(if plane.storage_bits() <= 8 {
                    data.get(y * plane.stride + x).copied().unwrap_or(0) as u16
                } else {
                    let at = y * plane.stride + 2 * x;
                    data.get(at..at + 2)
                        .map(|b| u16::from_ne_bytes([b[0], b[1]]))
                        .unwrap_or(0)
                });
            }
        }
        planes.push(samples);
        dims.push((cw, ch, dx, dy, u32::from(plane.bit_depth)));
    }
    let components: Vec<Component<'_>> = planes
        .iter()
        .zip(&dims)
        .map(|(samples, &(width, height, dx, dy, precision))| Component {
            samples,
            width,
            height,
            dx,
            dy,
            precision,
        })
        .collect();
    let state = unsafe { state(p) };
    state.read = false;
    state.codestream.clear();
    let settings = Settings {
        irreversible: state.irreversible,
        quality: state.quality,
    };
    match libheifer::jpeg2000_encoder::encode(&components, width, height, settings) {
        Ok(codestream) => {
            state.codestream = codestream;
            OK
        }
        Err(EncodeError::StartCompress) => error(9, 0, c"Failed opj_start_compress()"),
        Err(EncodeError::Encode) => error(9, 0, c"Failed opj_encode()"),
    }
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
            data.write(state.codestream.as_mut_ptr());
            size.write(state.codestream.len() as c_int);
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
pub(crate) static JPEG2000_ENCODER: Record = Record(EncoderPlugin {
    plugin_api_version: 4,
    compression_format: 7,
    id_name: c"libheifer-jpeg2000".as_ptr(),
    priority: 80,
    supports_lossy_compression: 0,
    supports_lossless_compression: 1,
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
