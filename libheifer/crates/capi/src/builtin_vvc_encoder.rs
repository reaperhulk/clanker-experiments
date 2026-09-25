// SPDX-License-Identifier: LGPL-3.0-or-later
//! The built-in VVC encoder as a static encoder-plugin record following
//! libheif's vvenc plugin (encoder_vvenc.cc): its parameters (quality and a
//! stored, unused lossless flag), input checks, picture padding (multiples of
//! 8 by edge replication), QP mapping, SAR VUI and one packet per NAL unit.
//! The encoder is libheifer's all-intra VVC encoder
//! (`libheifer::vvc::encoder`), so the bitstreams themselves are not vvenc's.
use crate::encoding_options::SequenceEncodingOptions;
use crate::{HeifError, plugin_types::*};
use libheifer::image::Image;
use libheifer::vvc::encoder as vvc;
use std::collections::VecDeque;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

struct Packet {
    data: Vec<u8>,
    frame_nr: usize,
    more: bool,
}

/// A started encoding: the geometry and settings fixed by the first picture.
struct Session {
    width: u32,
    height: u32,
    qp: i32,
    sar: Option<(u16, u16)>,
    fps: f64,
}

struct State {
    quality: c_int,
    lossless: bool,
    session: Option<Session>,
    output: VecDeque<Packet>,
    active: Vec<u8>,
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
// SAFETY: the records hold only static strings.
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
static LOSSLESS: Parameter = Parameter(EncoderParameter {
    version: 2,
    name: c"lossless".as_ptr(),
    kind: 2,
    value: ParameterValue {
        boolean: BooleanParameter { default_value: 0 },
    },
    has_default: 1,
});
struct List([*const EncoderParameter; 3]);
// SAFETY: points only to the immutable static parameters.
unsafe impl Sync for List {}
static PARAMETERS: List = List([&QUALITY.0, &LOSSLESS.0, ptr::null()]);

fn named(name: *const c_char, expected: &CStr) -> bool {
    !name.is_null() && unsafe { CStr::from_ptr(name) } == expected
}
unsafe fn state<'a>(p: *mut c_void) -> &'a mut State {
    unsafe { &mut *p.cast::<State>() }
}

unsafe extern "C" fn name() -> *const c_char {
    c"libheifer VVC encoder".as_ptr()
}
unsafe extern "C" fn new_encoder(out: *mut *mut c_void) -> HeifError {
    // vvenc_set_default_parameters: quality 50, lossless off.
    let state = Box::new(State {
        quality: 50,
        lossless: false,
        session: None,
        output: VecDeque::new(),
        active: Vec::new(),
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
    // vvenc's plugin stores the flag but never applies it.
    unsafe { state(p) }.lossless = enable != 0;
    OK
}
unsafe extern "C" fn get_lossless(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(c_int::from(state(p).lossless)) };
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
/// Also the boolean setter, as in the vvenc plugin record.
unsafe extern "C" fn set_integer(p: *mut c_void, name: *const c_char, value: c_int) -> HeifError {
    if named(name, c"quality") {
        unsafe { set_quality(p, value) }
    } else if named(name, c"lossless") {
        unsafe { set_lossless(p, value) }
    } else {
        UNSUPPORTED
    }
}
/// Also the boolean getter, as in the vvenc plugin record.
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
        if *colorspace == 2 {
            chroma.write(0);
        } else {
            colorspace.write(0);
            chroma.write(1);
        }
    }
}
unsafe extern "C" fn query_input2(_: *mut c_void, colorspace: *mut c_int, chroma: *mut c_int) {
    unsafe {
        if *colorspace == 2 {
            chroma.write(0);
        } else {
            colorspace.write(0);
            if !matches!(*chroma, 1..=3) {
                chroma.write(1);
            }
        }
    }
}
unsafe extern "C" fn query_encoded_size(
    _: *mut c_void,
    width: u32,
    height: u32,
    out_width: *mut u32,
    out_height: *mut u32,
) {
    unsafe {
        out_width.write(width.wrapping_add(7) & !7);
        out_height.write(height.wrapping_add(7) & !7);
    }
}

/// `check_encoder_input_image(image, true, {8})`.
fn check_input(img: &Image) -> Result<(), HeifError> {
    let channels: &[c_int] = match img.colorspace {
        2 => &[0],
        0 => &[0, 1, 2],
        _ => {
            return Err(error(
                8,
                3001,
                c"Encoder can only encode YCbCr and monochrome images",
            ));
        }
    };
    for &channel in channels {
        if img.plane(channel).is_none() {
            return Err(error(
                8,
                3001,
                c"Input image is missing one of its color channels",
            ));
        }
    }
    let depth = |c: c_int| img.plane(c).map_or(-1, |p| c_int::from(p.bit_depth));
    let bpp = depth(0);
    if channels[1..].iter().any(|&c| depth(c) != bpp) {
        return Err(error(
            8,
            4000,
            c"Encoder cannot encode images in which the color channels have different bit depths",
        ));
    }
    if bpp == 8 {
        Ok(())
    } else {
        Err(error(
            8,
            4000,
            c"Encoder cannot encode images at this bit depth",
        ))
    }
}

/// `vvenc_start_sequence_encoding_intern`.
unsafe fn start(
    p: *mut c_void,
    image: *const Image,
    input_class: c_int,
    num: u32,
    den: u32,
) -> HeifError {
    let state = unsafe { state(p) };
    state.session = None;
    let img = unsafe { &*image };
    if img.plane(0).map_or(-1, |p| c_int::from(p.bit_depth)) != 8 {
        return error(8, 3001, c"Bit depth not supported by libheifer-vvc");
    }
    let (mut aspect_h, mut aspect_v) = (1u32, 1u32);
    unsafe { crate::image::heif_image_get_pixel_aspect_ratio(image, &mut aspect_h, &mut aspect_v) };
    let sar = (matches!(input_class, 1 | 4)
        && aspect_h != aspect_v
        && aspect_h > 0
        && aspect_v > 0
        && aspect_h <= 0xffff
        && aspect_v <= 0xffff)
        .then_some((aspect_h as u16, aspect_v as u16));
    let (mut width, mut height) = (0, 0);
    unsafe { query_encoded_size(p, img.width, img.height, &mut width, &mut height) };
    state.session = Some(Session {
        width,
        height,
        // "invert encoder quality range and scale to 0-63"
        qp: 63 - state.quality * 63 / 100,
        sar,
        // vvenc_init_default's rounded frame rate, then m_FrameRate/m_FrameScale.
        fps: f64::from(den) / f64::from(num.max(1)),
    });
    OK
}

unsafe extern "C" fn start_sequence(
    p: *mut c_void,
    image: *const Image,
    input_class: c_int,
    num: u32,
    den: u32,
    _options: *const SequenceEncodingOptions,
) -> HeifError {
    unsafe { start(p, image, input_class, num, den) }
}

/// `copy_plane`: an input plane padded to `pw` x `ph` by repeating its last
/// column and row.
fn padded_plane(img: &Image, channel: c_int, w: usize, h: usize, pw: usize, ph: usize) -> Vec<u8> {
    let plane = img.plane(channel).expect("checked input");
    let data = plane.data();
    let mut out = Vec::with_capacity(pw * ph);
    for y in 0..ph {
        let row = &data[y.min(h - 1) * plane.stride..][..w];
        out.extend_from_slice(row);
        out.resize(out.len() + pw - w, row[w - 1]);
    }
    out
}

unsafe extern "C" fn encode_frame(
    p: *mut c_void,
    image: *const Image,
    frame_nr: usize,
) -> HeifError {
    let img = unsafe { &*image };
    if let Err(e) = check_input(img) {
        return e;
    }
    let state = unsafe { state(p) };
    let Some(session) = state.session.as_ref() else {
        return error(
            5,
            0,
            c"called plugin encode_sequence_frame() without start_sequence_encoding()",
        );
    };
    let (w, h) = (img.width as usize, img.height as usize);
    let (ew, eh) = (session.width as usize, session.height as usize);
    let chroma = if img.colorspace == 2 { 0 } else { img.chroma };
    let (cw, ch, sx, sy) = match chroma {
        0 => (0, 0, 0, 0),
        1 => (w.div_ceil(2), h.div_ceil(2), 1, 1),
        2 => (w.div_ceil(2), h, 1, 0),
        3 => (w, h, 0, 0),
        _ => return error(8, 3001, c"Unsupported chroma type"),
    };
    let y = padded_plane(img, 0, w, h, ew, eh);
    // vvenc keeps its default internal 4:2:0 for every colour input
    // (copyPadToPelUnitBuf): 4:4:4 chroma is averaged over 2x2 samples
    // (downsampleYuv). For 4:2:2 vvenc copies all rows into the half-height
    // buffer (undefined behaviour); rows are averaged in pairs here.
    let (cb, cr) = if chroma == 0 {
        (Vec::new(), Vec::new())
    } else {
        let (pw, ph) = (ew >> sx, eh >> sy);
        let down = |p: Vec<u8>| -> Vec<u8> {
            match chroma {
                1 => p,
                2 => (0..ph / 2)
                    .flat_map(|y| {
                        let p = &p;
                        (0..pw).map(move |x| {
                            ((u16::from(p[2 * y * pw + x])
                                + u16::from(p[(2 * y + 1) * pw + x])
                                + 1)
                                >> 1) as u8
                        })
                    })
                    .collect(),
                _ => (0..ph / 2)
                    .flat_map(|y| {
                        let p = &p;
                        (0..pw / 2).map(move |x| {
                            let at =
                                |dx: usize, dy: usize| u16::from(p[(2 * y + dy) * pw + 2 * x + dx]);
                            ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8
                        })
                    })
                    .collect(),
            }
        };
        (
            down(padded_plane(img, 1, cw, ch, pw, ph)),
            down(padded_plane(img, 2, cw, ch, pw, ph)),
        )
    };
    let picture = vvc::Picture8 {
        width: ew as u32,
        height: eh as u32,
        planes: [&y, &cb, &cr],
    };
    let settings = vvc::Settings {
        qp: session.qp,
        chroma: u32::from(chroma != 0),
        sar: session.sar,
        effort: 1,
        deblocking: true,
        fps: session.fps,
    };
    let Ok((units, _)) = vvc::encode(&picture, &settings) else {
        return error(8, 3000, c"Unspecified encoder error");
    };
    let n = units.len();
    for (i, data) in units.into_iter().enumerate() {
        state.output.push_back(Packet {
            data,
            frame_nr,
            more: i + 1 < n,
        });
    }
    OK
}
unsafe extern "C" fn end_sequence(p: *mut c_void) -> HeifError {
    unsafe { state(p) }.session = None;
    OK
}
unsafe extern "C" fn compressed_data2(
    p: *mut c_void,
    data: *mut *mut u8,
    size: *mut c_int,
    frame: *mut usize,
    _keyframe: *mut c_int,
    more: *mut c_int,
) -> HeifError {
    let state = unsafe { state(p) };
    unsafe {
        match state.output.pop_front() {
            None => {
                data.write(ptr::null_mut());
                size.write(0);
            }
            Some(packet) => {
                if !frame.is_null() {
                    frame.write(packet.frame_nr);
                }
                if !more.is_null() {
                    more.write(c_int::from(packet.more));
                }
                state.active = packet.data;
                data.write(state.active.as_mut_ptr());
                size.write(state.active.len() as c_int);
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
    let err = unsafe { start(p, image, input_class, 1, 25) };
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
pub(crate) static VVC_ENCODER: Record = Record(EncoderPlugin {
    plugin_api_version: 4,
    compression_format: 5,
    id_name: c"libheifer-vvc".as_ptr(),
    priority: 100,
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
    set_parameter_boolean: Some(set_integer),
    get_parameter_boolean: Some(get_integer),
    set_parameter_string: Some(set_string),
    get_parameter_string: Some(get_string),
    query_input_colorspace: Some(query_input),
    encode_image: Some(encode_image),
    get_compressed_data: Some(compressed_data),
    query_input_colorspace2: Some(query_input2),
    query_encoded_size: Some(query_encoded_size),
    minimum_required_libheif_version: 0x0115_0000,
    start_sequence_encoding: Some(start_sequence),
    encode_sequence_frame: Some(encode_frame),
    end_sequence_encoding: Some(end_sequence),
    get_compressed_data2: Some(compressed_data2),
    does_indicate_keyframes: 0,
});
