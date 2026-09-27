// SPDX-License-Identifier: LGPL-3.0-or-later
//! The built-in HTJ2K encoder as a static encoder-plugin record, with the
//! callback semantics of libheif's OpenJPH plugin (encoder_openjph.cc).
//! The codestream comes from the pure Rust `libheifer::htj2k_encoder`.
//!
//! The plugin guards its `tlm_marker` and `tilepart_division` parameters and
//! its COM-marker `write_headers` call with `OPENJPH_MAJOR_VERSION` /
//! `OPENJPH_MINOR_VERSION`, which OpenJPH does not define (its macros are
//! `OPENJPH_VERSION_MAJOR` / `_MINOR`). Those parameters are therefore absent
//! and the stored `codestream_comment` is never written.
use crate::{HeifError, plugin_types::*};
use libheifer::htj2k_encoder::{self, Component, Settings};
use libheifer::image::Image;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

struct State {
    /// Stored and reported, but unused by the plugin.
    quality: c_int,
    /// The `chroma` parameter (heif_chroma, -1 when undefined).
    chroma: c_int,
    reversible: bool,
    num_decompositions: u32,
    progression: u8,
    /// Stored and reported, but never written.
    comment: Vec<u8>,
    tile_size: (u32, u32),
    log_block: (u32, u32),
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

struct Strings<const N: usize>([*const c_char; N]);
// SAFETY: static strings only.
unsafe impl<const N: usize> Sync for Strings<N> {}
static CHROMA_VALUES: Strings<4> = Strings([
    c"420".as_ptr(),
    c"422".as_ptr(),
    c"444".as_ptr(),
    ptr::null(),
]);
const PROGRESSIONS: [&CStr; 5] = [c"LRCP", c"RLCP", c"RPCL", c"PCRL", c"CPRL"];
static PROGRESSION_VALUES: Strings<6> = Strings([
    c"LRCP".as_ptr(),
    c"RLCP".as_ptr(),
    c"RPCL".as_ptr(),
    c"PCRL".as_ptr(),
    c"CPRL".as_ptr(),
    ptr::null(),
]);

struct Parameter(EncoderParameter);
// SAFETY: the records hold only static strings.
unsafe impl Sync for Parameter {}
const fn boolean(name: &'static CStr) -> Parameter {
    Parameter(EncoderParameter {
        version: 2,
        name: name.as_ptr(),
        kind: 2,
        value: ParameterValue {
            boolean: BooleanParameter { default_value: 0 },
        },
        has_default: 1,
    })
}
const fn string(
    name: &'static CStr,
    default_value: *const c_char,
    valid_values: *const *const c_char,
) -> Parameter {
    Parameter(EncoderParameter {
        version: 2,
        name: name.as_ptr(),
        kind: 3,
        value: ParameterValue {
            string: StringParameter {
                default_value,
                valid_values,
            },
        },
        has_default: !default_value.is_null() as c_int,
    })
}
static LOSSLESS: Parameter = boolean(c"lossless");
static CHROMA: Parameter = string(c"chroma", c"444".as_ptr(), CHROMA_VALUES.0.as_ptr());
static DECOMPOSITIONS: Parameter = Parameter(EncoderParameter {
    version: 2,
    name: c"num_decompositions".as_ptr(),
    kind: 1,
    value: ParameterValue {
        integer: IntegerParameter {
            default_value: 5,
            have_minimum_maximum: 1,
            minimum: 0,
            maximum: 32,
            valid_values: ptr::null_mut(),
            num_valid_values: 0,
        },
    },
    has_default: 1,
});
static PROGRESSION: Parameter = string(
    c"progression_order",
    c"RPCL".as_ptr(),
    PROGRESSION_VALUES.0.as_ptr(),
);
static COMMENT: Parameter = string(c"codestream_comment", ptr::null(), ptr::null());
static TILE_SIZE: Parameter = string(c"tile_size", c"0,0".as_ptr(), ptr::null());
static BLOCK: Parameter = string(c"block_dimensions", c"64,64".as_ptr(), ptr::null());
struct List([*const EncoderParameter; 8]);
// SAFETY: points only to the immutable static parameters.
unsafe impl Sync for List {}
static PARAMETERS: List = List([
    &LOSSLESS.0,
    &CHROMA.0,
    &DECOMPOSITIONS.0,
    &PROGRESSION.0,
    &COMMENT.0,
    &TILE_SIZE.0,
    &BLOCK.0,
    ptr::null(),
]);

fn named(name: *const c_char, expected: &CStr) -> bool {
    !name.is_null() && unsafe { CStr::from_ptr(name) } == expected
}
unsafe fn state<'a>(p: *mut c_void) -> &'a mut State {
    unsafe { &mut *p.cast::<State>() }
}

/// std::stoul: leading whitespace, an optional sign, then decimal digits;
/// None where it would throw (no digits or overflow).
fn stoul(text: &[u8]) -> Option<u64> {
    let mut i = 0;
    while i < text.len() && matches!(text[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let negative = match text.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut value: u64 = 0;
    while i < text.len() && text[i].is_ascii_digit() {
        value = value
            .checked_mul(10)?
            .checked_add(u64::from(text[i] - b'0'))?;
        i += 1;
    }
    if i == start {
        return None;
    }
    Some(if negative {
        value.wrapping_neg()
    } else {
        value
    })
}

fn split_pair(value: &[u8]) -> Option<(u64, u64)> {
    let comma = value.iter().position(|&b| b == b',')?;
    Some((stoul(&value[..comma])?, stoul(&value[comma + 1..])?))
}

unsafe extern "C" fn name() -> *const c_char {
    c"libheifer HTJ2K encoder".as_ptr()
}
unsafe extern "C" fn new_encoder(out: *mut *mut c_void) -> HeifError {
    // The plugin's defaults (ojph_set_default_parameters); the tile size
    // default "0,0" is rejected, leaving OpenJPH's own (0, 0).
    let state = Box::new(State {
        quality: 70,
        chroma: 3,
        reversible: false,
        num_decompositions: 5,
        progression: htj2k_encoder::RPCL,
        comment: Vec::new(),
        tile_size: (0, 0),
        log_block: (6, 6),
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
    unsafe { state(p) }.quality = quality;
    OK
}
unsafe extern "C" fn get_quality(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(state(p).quality) };
    OK
}
unsafe extern "C" fn set_lossless(p: *mut c_void, enable: c_int) -> HeifError {
    unsafe { state(p) }.reversible = enable != 0;
    OK
}
unsafe extern "C" fn get_lossless(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(c_int::from(state(p).reversible)) };
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
    } else if named(name, c"num_decompositions") {
        if !(0..=32).contains(&value) {
            return INVALID_VALUE;
        }
        unsafe { state(p) }.num_decompositions = value as u32;
        OK
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
    } else if named(name, c"num_decompositions") {
        unsafe { out.write(state(p).num_decompositions as c_int) };
        OK
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
unsafe extern "C" fn set_string(
    p: *mut c_void,
    name: *const c_char,
    value: *const c_char,
) -> HeifError {
    let state = unsafe { state(p) };
    if named(name, c"codestream_comment") {
        if !value.is_null() {
            state.comment = unsafe { CStr::from_ptr(value) }.to_bytes().to_vec();
        }
        return OK;
    }
    if value.is_null() {
        return INVALID_VALUE;
    }
    let text = unsafe { CStr::from_ptr(value) }.to_bytes();
    if named(name, c"chroma") {
        state.chroma = match text {
            b"420" => 1,
            b"422" => 2,
            b"444" => 3,
            _ => return INVALID_VALUE,
        };
        OK
    } else if named(name, c"progression_order") {
        match PROGRESSIONS.iter().position(|p| p.to_bytes() == text) {
            Some(i) => {
                state.progression = i as u8;
                OK
            }
            None => INVALID_VALUE,
        }
    } else if named(name, c"tile_size") {
        // std::stoul throws in the plugin where this returns None.
        match split_pair(text) {
            Some((w, h))
                if (1..=u64::from(u32::MAX)).contains(&w)
                    && (1..=u64::from(u32::MAX)).contains(&h) =>
            {
                state.tile_size = (w as u32, h as u32);
                OK
            }
            _ => INVALID_VALUE,
        }
    } else if named(name, c"block_dimensions") {
        let log = |v: u64| (2..=10).find(|&l| v == 1u64 << l);
        match split_pair(text) {
            Some((w, h)) => match (log(w), log(h)) {
                // OpenJPH itself throws when the block has more than 4096 samples.
                (Some(lw), Some(lh)) if lw + lh <= 12 => {
                    state.log_block = (lw, lh);
                    OK
                }
                _ => INVALID_VALUE,
            },
            None => INVALID_VALUE,
        }
    } else {
        UNSUPPORTED
    }
}
unsafe extern "C" fn get_string(
    p: *mut c_void,
    name: *const c_char,
    out: *mut c_char,
    size: c_int,
) -> HeifError {
    let state = unsafe { state(p) };
    let owned;
    let text: &[u8] = if named(name, c"chroma") {
        match state.chroma {
            1 => b"420",
            2 => b"422",
            3 => b"444",
            _ => b"undefined",
        }
    } else if named(name, c"progression_order") {
        PROGRESSIONS[usize::from(state.progression)].to_bytes()
    } else if named(name, c"codestream_comment") {
        let end = state
            .comment
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(state.comment.len());
        &state.comment[..end]
    } else if named(name, c"tile_size") {
        owned = format!("{},{}", state.tile_size.0, state.tile_size.1);
        owned.as_bytes()
    } else if named(name, c"block_dimensions") {
        owned = format!(
            "{},{}",
            1u32 << state.log_block.0,
            1u32 << state.log_block.1
        );
        owned.as_bytes()
    } else {
        return UNSUPPORTED;
    };
    // safe_strcpy: strncpy of size - 1 bytes, then a terminating NUL.
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

unsafe extern "C" fn encode_image(
    p: *mut c_void,
    image: *const Image,
    _input_class: c_int,
) -> HeifError {
    let image = unsafe { &*image };
    let channels: &[i32] = match image.colorspace {
        0 if image.chroma == 0 => &[0],
        0 => &[0, 1, 2],
        2 => &[0],
        _ => {
            return error(
                9,
                0,
                c"OpenJPH encoder plugin received image with invalid colorspace.",
            );
        }
    };
    let (sub_dx, sub_dy) = match image.chroma {
        3 => (1, 1),
        2 => (2, 1),
        _ => (2, 2),
    };
    let width = image.width;
    let height = image.height;
    let mut planes = Vec::new();
    let mut dims = Vec::new();
    for (comp, &channel) in channels.iter().enumerate() {
        let Some(plane) = image.plane(channel) else {
            return error(9, 0, c"OpenJPH encoder error");
        };
        let (dx, dy) = if comp == 0 { (1, 1) } else { (sub_dx, sub_dy) };
        let cw = width.div_ceil(dx);
        let ch = height.div_ceil(dy);
        let data = plane.data();
        let mut samples = Vec::with_capacity((cw * ch) as usize);
        for y in 0..ch as usize {
            for x in 0..cw as usize {
                samples.push(if plane.bit_depth <= 8 {
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
        reversible: state.reversible,
        num_decompositions: state.num_decompositions,
        progression: state.progression,
        log_block: state.log_block,
        tile_size: state.tile_size,
        tilepart_resolutions: false,
        tilepart_components: false,
        tlm: false,
        comment: &[],
    };
    match htj2k_encoder::encode(&components, width, height, &settings) {
        Ok(codestream) => {
            state.codestream = codestream;
            OK
        }
        Err(_) => error(9, 0, c"OpenJPH encoder error"),
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
    unsafe { encode_image(p, image, 0) }
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
pub(crate) static HTJ2K_ENCODER: Record = Record(EncoderPlugin {
    plugin_api_version: 4,
    compression_format: 10,
    id_name: c"libheifer-htj2k".as_ptr(),
    priority: 80,
    supports_lossy_compression: 1,
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
    query_encoded_size: None,
    minimum_required_libheif_version: 0x0115_0000,
    start_sequence_encoding: Some(start_sequence),
    encode_sequence_frame: Some(encode_frame),
    end_sequence_encoding: Some(end_sequence),
    get_compressed_data2: Some(compressed_data2),
    does_indicate_keyframes: 1,
});
