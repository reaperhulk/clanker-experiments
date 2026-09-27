// SPDX-License-Identifier: LGPL-3.0-or-later
//! The built-in HEVC encoder as a static encoder-plugin record following
//! libheif's x265 plugin (encoder_x265.cc): its parameter list, defaults and
//! setter semantics, input checks, picture padding (even sizes of at least
//! 64 samples) and one packet per NAL unit. The encoder is the pure Rust
//! hpvca crate (all-intra; see `libheifer::hevc_encoder`), so the bitstreams
//! themselves are not x265's.
use crate::color::{heif_image_get_nclx_color_profile, heif_nclx_color_profile_free};
use crate::encoding_options::SequenceEncodingOptions;
use crate::{HeifError, plugin_types::*};
use libheifer::color::NclxProfile;
use libheifer::hevc_encoder as hevc;
use libheifer::image::Image;
use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

struct Packet {
    data: Vec<u8>,
    frame_nr: usize,
}

/// One entry of the x265 plugin's ordered parameter list.
#[derive(Clone)]
enum Value {
    Int(c_int),
    /// An `x265:` option (its value is never applied).
    Text,
}

/// A started encoding: the geometry and settings fixed by the first picture.
struct Session {
    width: u32,
    height: u32,
    bit_depth: c_int,
    settings: hevc::Settings,
    /// The parameter sets delivered last; a sequence frame repeats them only
    /// when they change.
    headers: Vec<Vec<u8>>,
}

struct State {
    parameters: Vec<(CString, Value)>,
    preset: CString,
    tune: CString,
    chroma: c_int,
    log_level: c_int,
    session: Option<Session>,
    output: VecDeque<Packet>,
    active: Vec<u8>,
    last_error: CString,
}

impl State {
    /// `encoder_struct_x265::add_param`: replace an entry, moving it to the end.
    fn add(&mut self, name: &CStr, value: Value) {
        self.parameters.retain(|(n, _)| n.as_c_str() != name);
        self.parameters.push((name.to_owned(), value));
    }
    /// `get_param`: the stored integer, or 0 when unset.
    fn int(&self, name: &CStr) -> c_int {
        self.parameters
            .iter()
            .find(|(n, _)| n.as_c_str() == name)
            .map_or(0, |(_, v)| match v {
                Value::Int(i) => *i,
                Value::Text => 0,
            })
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

struct Strings<const N: usize>([*const c_char; N]);
// SAFETY: static strings only.
unsafe impl<const N: usize> Sync for Strings<N> {}
static PRESETS: Strings<11> = Strings([
    c"ultrafast".as_ptr(),
    c"superfast".as_ptr(),
    c"veryfast".as_ptr(),
    c"faster".as_ptr(),
    c"fast".as_ptr(),
    c"medium".as_ptr(),
    c"slow".as_ptr(),
    c"slower".as_ptr(),
    c"veryslow".as_ptr(),
    c"placebo".as_ptr(),
    ptr::null(),
]);
static TUNES: Strings<5> = Strings([
    c"psnr".as_ptr(),
    c"ssim".as_ptr(),
    c"grain".as_ptr(),
    c"fastdecode".as_ptr(),
    ptr::null(),
]);
static CHROMAS: Strings<4> = Strings([
    c"420".as_ptr(),
    c"422".as_ptr(),
    c"444".as_ptr(),
    ptr::null(),
]);

struct Parameter(EncoderParameter);
// SAFETY: the records hold only static strings and tables.
unsafe impl Sync for Parameter {}
const fn integer(
    name: &'static CStr,
    default_value: c_int,
    has_default: bool,
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
        has_default: has_default as c_int,
    })
}
const fn string(
    name: &'static CStr,
    default_value: &'static CStr,
    valid: *const *const c_char,
) -> Parameter {
    Parameter(EncoderParameter {
        version: 2,
        name: name.as_ptr(),
        kind: 3,
        value: ParameterValue {
            string: StringParameter {
                default_value: default_value.as_ptr(),
                valid_values: valid,
            },
        },
        has_default: 1,
    })
}
static QUALITY: Parameter = integer(c"quality", 50, true, 0, 100);
static LOSSLESS: Parameter = Parameter(EncoderParameter {
    version: 2,
    name: c"lossless".as_ptr(),
    kind: 2,
    value: ParameterValue {
        boolean: BooleanParameter { default_value: 0 },
    },
    has_default: 1,
});
static PRESET: Parameter = string(c"preset", c"slow", PRESETS.0.as_ptr());
static TUNE: Parameter = string(c"tune", c"ssim", TUNES.0.as_ptr());
static TU_INTRA_DEPTH: Parameter = integer(c"tu-intra-depth", 2, true, 1, 4);
static COMPLEXITY: Parameter = integer(c"complexity", 50, false, 0, 100);
static CHROMA: Parameter = string(c"chroma", c"420", CHROMAS.0.as_ptr());
struct List([*const EncoderParameter; 8]);
// SAFETY: points only to the immutable static parameters.
unsafe impl Sync for List {}
static PARAMETERS: List = List([
    &QUALITY.0,
    &LOSSLESS.0,
    &PRESET.0,
    &TUNE.0,
    &TU_INTRA_DEPTH.0,
    &COMPLEXITY.0,
    &CHROMA.0,
    ptr::null(),
]);

fn named(name: *const c_char, expected: &CStr) -> bool {
    !name.is_null() && unsafe { CStr::from_ptr(name) } == expected
}
fn in_list(values: *const *const c_char, value: *const c_char) -> bool {
    let value = unsafe { CStr::from_ptr(value) };
    let mut i = 0;
    loop {
        let entry = unsafe { *values.add(i) };
        if entry.is_null() {
            return false;
        }
        if unsafe { CStr::from_ptr(entry) } == value {
            return true;
        }
        i += 1;
    }
}
unsafe fn state<'a>(p: *mut c_void) -> &'a mut State {
    unsafe { &mut *p.cast::<State>() }
}

unsafe extern "C" fn name() -> *const c_char {
    c"libheifer HEVC encoder (hpvca)".as_ptr()
}
unsafe extern "C" fn new_encoder(out: *mut *mut c_void) -> HeifError {
    let state = Box::new(State {
        parameters: Vec::new(),
        preset: CString::default(),
        tune: CString::default(),
        chroma: 1,
        log_level: 0,
        session: None,
        output: VecDeque::new(),
        active: Vec::new(),
        last_error: CString::default(),
    });
    let p: *mut c_void = Box::into_raw(state).cast();
    // x265_set_default_parameters: every parameter with a default.
    unsafe {
        set_integer(p, c"quality".as_ptr(), 50);
        set_integer(p, c"lossless".as_ptr(), 0);
        set_string(p, c"preset".as_ptr(), c"slow".as_ptr());
        set_string(p, c"tune".as_ptr(), c"ssim".as_ptr());
        set_integer(p, c"tu-intra-depth".as_ptr(), 2);
        set_string(p, c"chroma".as_ptr(), c"420".as_ptr());
        out.write(p);
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
    unsafe { state(p) }.add(c"quality", Value::Int(quality));
    OK
}
unsafe extern "C" fn get_quality(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(state(p).int(c"quality")) };
    OK
}
unsafe extern "C" fn set_lossless(p: *mut c_void, enable: c_int) -> HeifError {
    unsafe { state(p) }.add(c"lossless", Value::Int(c_int::from(enable != 0)));
    OK
}
unsafe extern "C" fn get_lossless(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(state(p).int(c"lossless")) };
    OK
}
unsafe extern "C" fn set_logging(p: *mut c_void, level: c_int) -> HeifError {
    if !(0..=4).contains(&level) {
        return INVALID_VALUE;
    }
    unsafe { state(p) }.log_level = level;
    OK
}
unsafe extern "C" fn get_logging(p: *mut c_void, out: *mut c_int) -> HeifError {
    unsafe { out.write(state(p).log_level) };
    OK
}
unsafe extern "C" fn list_parameters(_: *mut c_void) -> *const *const EncoderParameter {
    PARAMETERS.0.as_ptr()
}
/// Also the boolean setter, as in the x265 plugin record.
unsafe extern "C" fn set_integer(p: *mut c_void, name: *const c_char, value: c_int) -> HeifError {
    if named(name, c"quality") {
        return unsafe { set_quality(p, value) };
    } else if named(name, c"lossless") {
        return unsafe { set_lossless(p, value) };
    } else if named(name, c"tu-intra-depth") {
        if !(1..=4).contains(&value) {
            return INVALID_VALUE;
        }
        unsafe { state(p) }.add(c"tu-intra-depth", Value::Int(value));
        return OK;
    } else if named(name, c"complexity") {
        if !(0..=100).contains(&value) {
            return INVALID_VALUE;
        }
        unsafe { state(p) }.add(c"complexity", Value::Int(value));
        return OK;
    }
    UNSUPPORTED
}
/// Also the boolean getter, as in the x265 plugin record.
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
    for key in [c"tu-intra-depth", c"complexity"] {
        if named(name, key) {
            unsafe { out.write(state(p).int(key)) };
            return OK;
        }
    }
    UNSUPPORTED
}
unsafe extern "C" fn set_string(
    p: *mut c_void,
    name: *const c_char,
    value: *const c_char,
) -> HeifError {
    let state = unsafe { state(p) };
    if named(name, c"preset") {
        if !in_list(PRESETS.0.as_ptr(), value) {
            return INVALID_VALUE;
        }
        state.preset = unsafe { CStr::from_ptr(value) }.to_owned();
        return OK;
    } else if named(name, c"tune") {
        if !in_list(TUNES.0.as_ptr(), value) {
            return INVALID_VALUE;
        }
        state.tune = unsafe { CStr::from_ptr(value) }.to_owned();
        return OK;
    } else if !name.is_null()
        && unsafe { CStr::from_ptr(name) }
            .to_bytes()
            .starts_with(b"x265:")
    {
        let name = unsafe { CStr::from_ptr(name) };
        state.add(name, Value::Text);
        return OK;
    } else if named(name, c"chroma") {
        state.chroma = if named(value, c"420") {
            1
        } else if named(value, c"422") {
            2
        } else if named(value, c"444") {
            3
        } else {
            return INVALID_VALUE;
        };
        return OK;
    }
    UNSUPPORTED
}
fn save_strcpy(text: &[u8], out: *mut c_char, size: c_int) {
    // strncpy of size - 1 bytes, then a terminating NUL.
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
}
unsafe extern "C" fn get_string(
    p: *mut c_void,
    name: *const c_char,
    out: *mut c_char,
    size: c_int,
) -> HeifError {
    let state = unsafe { state(p) };
    if named(name, c"preset") {
        save_strcpy(state.preset.to_bytes(), out, size);
    } else if named(name, c"tune") {
        save_strcpy(state.tune.to_bytes(), out, size);
    } else if named(name, c"chroma") {
        let text: &[u8] = match state.chroma {
            1 => b"420",
            2 => b"422",
            3 => b"444",
            _ => return INVALID_VALUE,
        };
        save_strcpy(text, out, size);
    } else {
        return UNSUPPORTED;
    }
    OK
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
unsafe extern "C" fn query_input2(p: *mut c_void, colorspace: *mut c_int, chroma: *mut c_int) {
    unsafe {
        if *colorspace == 2 {
            chroma.write(0);
        } else {
            colorspace.write(0);
            chroma.write(state(p).chroma);
        }
    }
}

/// `check_encoder_input_image(image, true, {8, 10, 12})`.
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

/// `rounded_size`: even, and at least 64.
fn rounded_size(s: i64) -> i64 {
    ((s + 1) & !1).max(64)
}

struct Nclx(*mut NclxProfile);
impl Drop for Nclx {
    fn drop(&mut self) {
        unsafe { heif_nclx_color_profile_free(self.0) };
    }
}

fn plugin_error(state: &mut State, message: String) -> HeifError {
    state.last_error = CString::new(message).unwrap_or_default();
    HeifError {
        code: 8,
        subcode: 0,
        message: state.last_error.as_ptr(),
    }
}

/// `x265_start_sequence_encoding_intern`.
unsafe fn start(p: *mut c_void, image: *const Image, input_class: c_int) -> HeifError {
    let state = unsafe { state(p) };
    state.session = None;
    let img = unsafe { &*image };
    let bit_depth = img.plane(0).map_or(-1, |p| c_int::from(p.bit_depth));
    if !matches!(bit_depth, 8 | 10 | 12) {
        return error(8, 4000, c"Bit depth not supported by hpvca");
    }
    let mut raw: *mut NclxProfile = ptr::null_mut();
    if unsafe { heif_image_get_nclx_color_profile(image, &mut raw) }.code != 0 {
        raw = ptr::null_mut();
    }
    let nclx = Nclx(raw);
    let nclx = unsafe { nclx.0.as_ref() };
    let mut vui = hevc::VuiSignal {
        full_range: nclx.is_none_or(|n| n.full_range_flag != 0),
        description: None,
    };
    if let Some(n) = nclx
        && matches!(input_class, 1 | 4)
    {
        // x265 parses the code points as integers into u8 VUI fields.
        vui.description = Some((
            n.color_primaries as u8,
            n.transfer_characteristics as u8,
            n.matrix_coefficients as u8,
        ));
    }
    let mut quality = 50;
    let mut lossless = false;
    for (name, value) in &state.parameters {
        match (name.to_bytes(), value) {
            (b"quality", Value::Int(q)) => quality = *q,
            (b"lossless", Value::Int(l)) => lossless = *l != 0,
            (n, _) if n.starts_with(b"x265:") => {
                // hpvca is not x265: no x265 option can be applied.
                let key = String::from_utf8_lossy(&n[5..]).into_owned();
                state.last_error =
                    CString::new(format!("Unsupported x265 encoder parameter: {key}"))
                        .unwrap_or_default();
                return HeifError {
                    code: 5,
                    subcode: 2005,
                    message: state.last_error.as_ptr(),
                };
            }
            _ => {}
        }
    }
    let width = rounded_size(i64::from(img.width));
    let height = rounded_size(i64::from(img.height));
    if width * height > 142_606_336 {
        return error(
            9,
            5002,
            c"Image too large for x265: at most 142,606,336 luma samples (HEVC Level 7.2) are supported",
        );
    }
    let slow = matches!(
        state.preset.to_bytes(),
        b"slow" | b"slower" | b"veryslow" | b"placebo"
    );
    state.session = Some(Session {
        width: width as u32,
        height: height as u32,
        bit_depth,
        settings: hevc::Settings {
            quality,
            lossless,
            slow,
            psnr: state.tune.to_bytes() == b"psnr",
            vui,
        },
        headers: Vec::new(),
    });
    OK
}

unsafe extern "C" fn start_sequence(
    p: *mut c_void,
    image: *const Image,
    input_class: c_int,
    _num: u32,
    _den: u32,
    _options: *const SequenceEncodingOptions,
) -> HeifError {
    unsafe { start(p, image, input_class) }
}

/// Samples of one padded plane as u16.
fn plane_samples(img: &Image, channel: c_int, w: usize, h: usize) -> Vec<u16> {
    let plane = img.plane(channel).expect("checked input");
    let data = plane.data();
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = &data[y * plane.stride..];
        if plane.bytes_per_pixel == 1 {
            out.extend(row[..w].iter().map(|&v| u16::from(v)));
        } else {
            out.extend(
                row[..2 * w]
                    .chunks_exact(2)
                    .map(|b| u16::from_ne_bytes([b[0], b[1]])),
            );
        }
    }
    out
}

unsafe extern "C" fn encode_frame(
    p: *mut c_void,
    image: *const Image,
    frame_nr: usize,
) -> HeifError {
    let state = unsafe { state(p) };
    let Some(session) = state.session.as_ref() else {
        return error(
            5,
            0,
            c"called plugin encode_sequence_frame() without start_sequence_encoding()",
        );
    };
    let img = unsafe { &*image };
    if let Err(e) = check_input(img) {
        return e;
    }
    if img.plane(0).map_or(-1, |p| c_int::from(p.bit_depth)) != session.bit_depth {
        return error(
            8,
            4000,
            c"All frames of a sequence must have the bit depth of the first frame",
        );
    }
    let (width, height) = (session.width, session.height);
    // The image content is unchanged; only its padding is extended.
    let err = unsafe {
        crate::image::heif_image_extend_padding_to_size(
            image.cast_mut(),
            width as c_int,
            height as c_int,
        )
    };
    if err.code != 0 {
        return err;
    }
    let img = unsafe { &*image };
    let chroma = if img.colorspace == 2 { 0 } else { img.chroma };
    let (cw, ch) = match chroma {
        1 => (width.div_ceil(2), height.div_ceil(2)),
        2 => (width.div_ceil(2), height),
        _ => (width, height),
    };
    let y = plane_samples(img, 0, width as usize, height as usize);
    let (cb, cr) = if chroma == 0 {
        (Vec::new(), Vec::new())
    } else {
        (
            plane_samples(img, 1, cw as usize, ch as usize),
            plane_samples(img, 2, cw as usize, ch as usize),
        )
    };
    let picture = hevc::Picture {
        width,
        height,
        bit_depth: session.bit_depth as u8,
        chroma,
        planes: [&y, &cb, &cr],
    };
    let settings = state.session.as_ref().unwrap().settings;
    let units = match hevc::encode(&picture, &settings) {
        Ok(units) => units,
        Err(e) => return plugin_error(state, e),
    };
    let session = state.session.as_mut().unwrap();
    let headers: Vec<Vec<u8>> = units
        .iter()
        .filter(|u| matches!((u[0] >> 1) & 0x3f, 32..=34))
        .cloned()
        .collect();
    let send_headers = headers != session.headers;
    session.headers = headers;
    for data in units {
        if matches!((data[0] >> 1) & 0x3f, 32..=34) && !send_headers {
            continue;
        }
        state.output.push_back(Packet { data, frame_nr });
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
    _more: *mut c_int,
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
    let err = unsafe { start(p, image, input_class) };
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
pub(crate) static HEVC_ENCODER: Record = Record(EncoderPlugin {
    plugin_api_version: 4,
    compression_format: 1,
    id_name: c"libheifer-hevc".as_ptr(),
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
    query_encoded_size: None,
    minimum_required_libheif_version: 0x0115_0000,
    start_sequence_encoding: Some(start_sequence),
    encode_sequence_frame: Some(encode_frame),
    end_sequence_encoding: Some(end_sequence),
    get_compressed_data2: Some(compressed_data2),
    does_indicate_keyframes: 0,
});
