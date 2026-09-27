// SPDX-License-Identifier: LGPL-3.0-or-later
//! The built-in AV1 encoder as a static encoder-plugin record, transliterated
//! from libheif's rav1e plugin (encoder_rav1e.cc). The encoder is the pure
//! Rust rav1e crate without assembly, driven through the same `rav1e::capi`
//! functions the native plugin calls, so configuration parsing, frame filling
//! and packet delivery follow the identical code.
use crate::color::{heif_image_get_nclx_color_profile, heif_nclx_color_profile_free};
use crate::encoding_options::SequenceEncodingOptions;
use crate::image::{
    heif_image_get_bits_per_pixel_range, heif_image_get_height, heif_image_get_plane_readonly2,
    heif_image_get_width, heif_image_has_channel,
};
use crate::{HeifError, plugin_types::*};
use libheifer::color::NclxProfile;
use libheifer::image::Image;
use rav1e::capi as ra;
use rav1e::prelude::{
    ChromaSamplePosition, ChromaSampling, ColorPrimaries, FrameType, MatrixCoefficients,
    PixelRange, Rational, TransferCharacteristics,
};
use std::collections::VecDeque;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

struct Packet {
    data: Vec<u8>,
    frame_nr: usize,
    is_keyframe: bool,
}

struct State {
    speed: c_int,
    /// Not in the parameter list, so never defaulted; `new T()` zeroes it.
    quality: c_int,
    min_q: c_int,
    threads: c_int,
    tile_rows: c_int,
    tile_cols: c_int,
    chroma: c_int,
    context: *mut ra::Context,
    y_shift: u8,
    bit_depth: c_int,
    output: VecDeque<Packet>,
    active: Vec<u8>,
}

impl Drop for State {
    fn drop(&mut self) {
        if !self.context.is_null() {
            unsafe { ra::rav1e_context_unref(self.context) };
        }
    }
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
const LIBRARY_ERROR: HeifError = error(8, 0, c"rav1e error");

struct Strings([*const c_char; 4]);
// SAFETY: static strings only.
unsafe impl Sync for Strings {}
static CHROMA_VALUES: Strings = Strings([
    c"420".as_ptr(),
    c"422".as_ptr(),
    c"444".as_ptr(),
    ptr::null(),
]);
struct TileValues([c_int; 7]);
// SAFETY: immutable table; libheif only reads it through the parameter record.
unsafe impl Sync for TileValues {}
static TILE_VALUES: TileValues = TileValues([1, 2, 4, 8, 16, 32, 64]);

struct Parameter(EncoderParameter);
// SAFETY: the records hold only static strings and tables.
unsafe impl Sync for Parameter {}
const fn integer(
    name: &'static CStr,
    default_value: c_int,
    minimum: c_int,
    maximum: c_int,
) -> Parameter {
    Parameter(EncoderParameter {
        version: 2,
        name: name.as_ptr(),
        kind: 1,
        value: ParameterValue {
            integer: IntegerParameter {
                default_value,
                have_minimum_maximum: 1,
                minimum,
                maximum,
                valid_values: ptr::null_mut(),
                num_valid_values: 0,
            },
        },
        has_default: 1,
    })
}
const fn tiles(name: &'static CStr) -> Parameter {
    Parameter(EncoderParameter {
        version: 2,
        name: name.as_ptr(),
        kind: 1,
        value: ParameterValue {
            integer: IntegerParameter {
                default_value: 4,
                have_minimum_maximum: 0,
                minimum: 0,
                maximum: 0,
                valid_values: TILE_VALUES.0.as_ptr().cast_mut(),
                num_valid_values: 7,
            },
        },
        has_default: 1,
    })
}
static SPEED: Parameter = integer(c"speed", 8, 0, 10);
static THREADS: Parameter = integer(c"threads", 4, 1, 16);
static TILE_ROWS: Parameter = tiles(c"tile-rows");
static TILE_COLS: Parameter = tiles(c"tile-cols");
static CHROMA: Parameter = Parameter(EncoderParameter {
    version: 2,
    name: c"chroma".as_ptr(),
    kind: 3,
    value: ParameterValue {
        string: StringParameter {
            default_value: c"420".as_ptr(),
            valid_values: CHROMA_VALUES.0.as_ptr(),
        },
    },
    has_default: 1,
});
static MIN_Q: Parameter = integer(c"min-q", 0, 0, 255);
struct List([*const EncoderParameter; 7]);
// SAFETY: points only to the immutable static parameters.
unsafe impl Sync for List {}
static PARAMETERS: List = List([
    &SPEED.0,
    &THREADS.0,
    &TILE_ROWS.0,
    &TILE_COLS.0,
    &CHROMA.0,
    &MIN_Q.0,
    ptr::null(),
]);

fn named(name: *const c_char, expected: &CStr) -> bool {
    !name.is_null() && unsafe { CStr::from_ptr(name) } == expected
}
unsafe fn state<'a>(p: *mut c_void) -> &'a mut State {
    unsafe { &mut *p.cast::<State>() }
}

unsafe extern "C" fn name() -> *const c_char {
    c"libheifer AV1 encoder (rav1e)".as_ptr()
}
unsafe extern "C" fn new_encoder(out: *mut *mut c_void) -> HeifError {
    let mut state = Box::new(State {
        speed: 0,
        quality: 0,
        min_q: 0,
        threads: 0,
        tile_rows: 1,
        tile_cols: 1,
        chroma: 0,
        context: ptr::null_mut(),
        y_shift: 0,
        bit_depth: 8,
        output: VecDeque::new(),
        active: Vec::new(),
    });
    // rav1e_set_default_parameters
    let p: *mut c_void = (&mut *state as *mut State).cast();
    unsafe {
        set_integer(p, c"speed".as_ptr(), 8);
        set_integer(p, c"threads".as_ptr(), 4);
        set_integer(p, c"tile-rows".as_ptr(), 4);
        set_integer(p, c"tile-cols".as_ptr(), 4);
        set_string(p, c"chroma".as_ptr(), c"420".as_ptr());
        set_integer(p, c"min-q".as_ptr(), 0);
        out.write(Box::into_raw(state).cast());
    }
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
    if enable != 0 {
        unsafe { state(p) }.min_q = 0;
    }
    OK
}
unsafe extern "C" fn get_lossless(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(c_int::from(state(p).min_q == 0)) };
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
        return unsafe { set_quality(p, value) };
    } else if named(name, c"lossless") {
        return unsafe { set_lossless(p, value) };
    }
    let state = unsafe { state(p) };
    let field = if named(name, c"min-q") {
        &mut state.min_q
    } else if named(name, c"threads") {
        &mut state.threads
    } else if named(name, c"speed") {
        &mut state.speed
    } else if named(name, c"tile-rows") {
        &mut state.tile_rows
    } else if named(name, c"tile-cols") {
        &mut state.tile_cols
    } else {
        return UNSUPPORTED;
    };
    *field = value;
    OK
}
unsafe extern "C" fn get_integer(
    p: *mut c_void,
    name: *const c_char,
    out: *mut c_int,
) -> HeifError {
    if named(name, c"quality") {
        return unsafe { get_quality(p, out) };
    } else if named(name, c"lossless") {
        return unsafe { get_lossless(p, out) };
    }
    let state = unsafe { state(p) };
    let value = if named(name, c"min-q") {
        state.min_q
    } else if named(name, c"threads") {
        state.threads
    } else if named(name, c"speed") {
        state.speed
    } else if named(name, c"tile-rows") {
        state.tile_rows
    } else if named(name, c"tile-cols") {
        state.tile_cols
    } else {
        return UNSUPPORTED;
    };
    unsafe { out.write(value) };
    OK
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
        _ => return INVALID_VALUE,
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
        colorspace.write(0);
        chroma.write(1);
    }
}
unsafe extern "C" fn query_input2(p: *mut c_void, colorspace: *mut c_int, chroma: *mut c_int) {
    unsafe {
        colorspace.write(0);
        chroma.write(state(p).chroma);
    }
}

/// Encoder input check (encoder_input_check.h): no monochrome, 8/10/12 bits.
fn check_input(image: *const Image) -> Result<(), HeifError> {
    let img = unsafe { &*image };
    match img.colorspace {
        2 => return Err(error(8, 3001, c"Encoder cannot encode monochrome images")),
        0 => {}
        _ => {
            return Err(error(
                8,
                3001,
                c"Encoder can only encode YCbCr and monochrome images",
            ));
        }
    }
    for channel in [0, 1, 2] {
        if unsafe { heif_image_has_channel(image, channel) } == 0 {
            return Err(error(
                8,
                3001,
                c"Input image is missing one of its color channels",
            ));
        }
    }
    let bpp = unsafe { heif_image_get_bits_per_pixel_range(image, 0) };
    for channel in [1, 2] {
        if unsafe { heif_image_get_bits_per_pixel_range(image, channel) } != bpp {
            return Err(error(
                8,
                4000,
                c"Encoder cannot encode images in which the color channels have different bit depths",
            ));
        }
    }
    if matches!(bpp, 8 | 10 | 12) {
        Ok(())
    } else {
        Err(error(
            8,
            4000,
            c"Encoder cannot encode images at this bit depth",
        ))
    }
}

fn parse(cfg: *mut ra::Config, key: &CStr, value: &CStr) -> bool {
    unsafe { ra::rav1e_config_parse(cfg, key.as_ptr(), value.as_ptr()) != -1 }
}
fn parse_int(cfg: *mut ra::Config, key: &CStr, value: c_int) -> bool {
    unsafe { ra::rav1e_config_parse_int(cfg, key.as_ptr(), value) != -1 }
}

/// The nclx code points as rav1e's enums. The native plugin casts the integers
/// directly; code points outside the enums are undefined behaviour there.
fn color_description(
    nclx: &NclxProfile,
) -> Option<(MatrixCoefficients, ColorPrimaries, TransferCharacteristics)> {
    use num_traits::FromPrimitive;
    Some((
        MatrixCoefficients::from_i32(nclx.matrix_coefficients)?,
        ColorPrimaries::from_i32(nclx.color_primaries)?,
        TransferCharacteristics::from_i32(nclx.transfer_characteristics)?,
    ))
}

struct Config(*mut ra::Config);
impl Drop for Config {
    fn drop(&mut self) {
        unsafe { ra::rav1e_config_unref(self.0) };
    }
}
struct Nclx(*mut NclxProfile);
impl Drop for Nclx {
    fn drop(&mut self) {
        unsafe { heif_nclx_color_profile_free(self.0) };
    }
}

unsafe fn start(
    p: *mut c_void,
    image: *const Image,
    input_class: c_int,
    framerate: (u32, u32),
    options: *const SequenceEncodingOptions,
    image_sequence: bool,
) -> HeifError {
    let encoder = unsafe { state(p) };
    if !encoder.context.is_null() {
        unsafe { ra::rav1e_context_unref(encoder.context) };
        encoder.context = ptr::null_mut();
    }
    let img = unsafe { &*image };
    let (sampling, position) = if input_class == 2 {
        encoder.y_shift = 1;
        (ChromaSampling::Cs420, ChromaSamplePosition::Unknown)
    } else {
        match img.chroma {
            3 => (ChromaSampling::Cs444, ChromaSamplePosition::Colocated),
            2 => (ChromaSampling::Cs422, ChromaSamplePosition::Colocated),
            1 => {
                encoder.y_shift = 1;
                (ChromaSampling::Cs420, ChromaSamplePosition::Unknown)
            }
            _ => return LIBRARY_ERROR,
        }
    };
    let mut raw: *mut NclxProfile = ptr::null_mut();
    if unsafe { heif_image_get_nclx_color_profile(image, &mut raw) }.code != 0 {
        raw = ptr::null_mut();
    }
    let nclx = Nclx(raw);
    let nclx_ref = unsafe { nclx.0.as_ref() };
    let range = match nclx_ref {
        Some(n) if n.full_range_flag == 0 => PixelRange::Limited,
        _ => PixelRange::Full,
    };
    let bit_depth = unsafe { heif_image_get_bits_per_pixel_range(image, 0) };
    let cfg = Config(unsafe { ra::rav1e_config_default() });
    if unsafe {
        ra::rav1e_config_set_pixel_format(cfg.0, bit_depth as u8, sampling, position, range)
    } < 0
    {
        return LIBRARY_ERROR;
    }
    unsafe {
        ra::rav1e_config_set_time_base(
            cfg.0,
            Rational {
                num: u64::from(framerate.0),
                den: u64::from(framerate.1),
            },
        )
    };
    if !image_sequence && !parse(cfg.0, c"still_picture", c"true") {
        return LIBRARY_ERROR;
    }
    if image_sequence {
        let options = unsafe { &*options };
        if options.gop_structure == 0 {
            if !parse(cfg.0, c"key_frame_interval", c"1") {
                return LIBRARY_ERROR;
            }
        } else if options.keyframe_distance_max != 0 {
            let value = std::ffi::CString::new(options.keyframe_distance_max.to_string()).unwrap();
            if !parse(cfg.0, c"key_frame_interval", &value) {
                return LIBRARY_ERROR;
            }
        }
        if options.keyframe_distance_min != 0 {
            let value = std::ffi::CString::new(options.keyframe_distance_min.to_string()).unwrap();
            if !parse(cfg.0, c"min_key_frame_interval", &value) {
                return LIBRARY_ERROR;
            }
        }
    }
    if !parse_int(cfg.0, c"width", unsafe { heif_image_get_width(image, 0) })
        || !parse_int(cfg.0, c"height", unsafe { heif_image_get_height(image, 0) })
        || !parse_int(cfg.0, c"threads", encoder.threads)
    {
        return LIBRARY_ERROR;
    }
    let description = nclx_ref.map(color_description);
    if let Some(n) = description
        && (input_class == 1 || input_class == 4)
    {
        let Some((matrix, primaries, transfer)) = n else {
            return LIBRARY_ERROR;
        };
        if unsafe { ra::rav1e_config_set_color_description(cfg.0, matrix, primaries, transfer) }
            == -1
        {
            return LIBRARY_ERROR;
        }
    }
    if !parse_int(cfg.0, c"min_quantizer", encoder.min_q) {
        return LIBRARY_ERROR;
    }
    let base_quantizer = ((100 - encoder.quality) * 255 + 50) / 100;
    if !parse_int(cfg.0, c"quantizer", base_quantizer) {
        return LIBRARY_ERROR;
    }
    if encoder.tile_rows != 1 && !parse_int(cfg.0, c"tile_rows", encoder.tile_rows) {
        return LIBRARY_ERROR;
    }
    if encoder.tile_cols != 1 && !parse_int(cfg.0, c"tile_cols", encoder.tile_cols) {
        return LIBRARY_ERROR;
    }
    if !parse_int(cfg.0, c"speed", encoder.speed) {
        return LIBRARY_ERROR;
    }
    if let Some(n) = description {
        let Some((matrix, primaries, transfer)) = n else {
            return LIBRARY_ERROR;
        };
        unsafe { ra::rav1e_config_set_color_description(cfg.0, matrix, primaries, transfer) };
    }
    encoder.context = unsafe { ra::rav1e_context_new(cfg.0) };
    if encoder.context.is_null() {
        return LIBRARY_ERROR;
    }
    encoder.bit_depth = bit_depth;
    OK
}

unsafe fn collect_packets(encoder: &mut State) -> HeifError {
    loop {
        let mut pkt: *mut ra::Packet = ptr::null_mut();
        let status = unsafe { ra::rav1e_receive_packet(encoder.context, &mut pkt) };
        match status {
            ra::EncoderStatus::NeedMoreData => return OK,
            ra::EncoderStatus::Encoded => continue,
            ra::EncoderStatus::Success => {}
            _ => return LIBRARY_ERROR,
        }
        if let Some(packet) = unsafe { pkt.as_ref() } {
            if !packet.data.is_null() && packet.len > 0 {
                encoder.output.push_back(Packet {
                    data: unsafe { std::slice::from_raw_parts(packet.data, packet.len) }.to_vec(),
                    frame_nr: packet.input_frameno as usize,
                    is_keyframe: packet.frame_type == FrameType::KEY,
                });
            }
            unsafe { ra::rav1e_packet_unref(pkt) };
        } else {
            break;
        }
    }
    OK
}

unsafe extern "C" fn start_sequence(
    p: *mut c_void,
    image: *const Image,
    input_class: c_int,
    num: u32,
    den: u32,
    options: *const SequenceEncodingOptions,
) -> HeifError {
    unsafe { start(p, image, input_class, (num, den), options, true) }
}
unsafe extern "C" fn encode_frame(p: *mut c_void, image: *const Image, _: usize) -> HeifError {
    if let Err(e) = check_input(image) {
        return e;
    }
    let encoder = unsafe { state(p) };
    let bit_depth = unsafe { heif_image_get_bits_per_pixel_range(image, 0) };
    if bit_depth != encoder.bit_depth {
        return error(
            8,
            4000,
            c"All frames of a sequence must have the bit depth of the first frame",
        );
    }
    let frame = unsafe { ra::rav1e_frame_new(encoder.context) };
    let byte_width = if bit_depth > 8 { 2 } else { 1 };
    let height = unsafe { heif_image_get_height(image, 0) } as usize;
    let uv_height = (height + usize::from(encoder.y_shift)) >> encoder.y_shift;
    for (plane, rows) in [(0, height), (1, uv_height), (2, uv_height)] {
        let mut stride = 0usize;
        let mut data = unsafe { heif_image_get_plane_readonly2(image, plane, &mut stride) };
        if data.is_null() {
            // A zero-length slice needs a non-null pointer on the Rust side.
            data = ptr::NonNull::<u8>::dangling().as_ptr();
        }
        unsafe {
            ra::rav1e_frame_fill_plane(
                frame,
                plane,
                data,
                stride * rows,
                stride as isize,
                byte_width,
            )
        };
    }
    let status = unsafe { ra::rav1e_send_frame(encoder.context, frame) };
    unsafe { ra::rav1e_frame_unref(frame) };
    if status != ra::EncoderStatus::Success {
        return LIBRARY_ERROR;
    }
    unsafe { collect_packets(encoder) }
}
unsafe extern "C" fn end_sequence(p: *mut c_void) -> HeifError {
    let encoder = unsafe { state(p) };
    if unsafe { ra::rav1e_send_frame(encoder.context, ptr::null_mut()) }
        != ra::EncoderStatus::Success
    {
        return LIBRARY_ERROR;
    }
    unsafe { collect_packets(encoder) }
}
unsafe extern "C" fn compressed_data2(
    p: *mut c_void,
    data: *mut *mut u8,
    size: *mut c_int,
    frame: *mut usize,
    keyframe: *mut c_int,
    _more: *mut c_int,
) -> HeifError {
    let encoder = unsafe { state(p) };
    encoder.active.clear();
    unsafe {
        match encoder.output.pop_front() {
            None => {
                size.write(0);
                data.write(ptr::null_mut());
            }
            Some(packet) => {
                if !frame.is_null() {
                    frame.write(packet.frame_nr);
                }
                if !keyframe.is_null() {
                    keyframe.write(c_int::from(packet.is_keyframe));
                }
                encoder.active = packet.data;
                size.write(encoder.active.len() as c_int);
                data.write(encoder.active.as_mut_ptr());
            }
        }
    }
    OK
}
unsafe extern "C" fn compressed_data(
    p: *mut c_void,
    data: *mut *mut u8,
    size: *mut c_int,
    _: *mut c_int,
) -> HeifError {
    unsafe {
        compressed_data2(
            p,
            data,
            size,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    }
}
unsafe extern "C" fn encode_image(
    p: *mut c_void,
    image: *const Image,
    input_class: c_int,
) -> HeifError {
    let err = unsafe { start(p, image, input_class, (1, 25), ptr::null(), false) };
    if err.code != 0 {
        return err;
    }
    let err = unsafe { encode_frame(p, image, 0) };
    if err.code != 0 {
        return err;
    }
    unsafe { end_sequence(p) };
    OK
}

pub(crate) struct Record(pub EncoderPlugin);
// SAFETY: immutable record of static strings and function pointers.
unsafe impl Sync for Record {}
pub(crate) static AV1_ENCODER: Record = Record(EncoderPlugin {
    plugin_api_version: 4,
    compression_format: 4,
    id_name: c"libheifer-rav1e".as_ptr(),
    priority: 20,
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
    query_encoded_size: None,
    minimum_required_libheif_version: 0x0115_0000,
    start_sequence_encoding: Some(start_sequence),
    encode_sequence_frame: Some(encode_frame),
    end_sequence_encoding: Some(end_sequence),
    get_compressed_data2: Some(compressed_data2),
    does_indicate_keyframes: 1,
});
