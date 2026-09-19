// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::HeifContext,
    encoder_parameters::*,
    plugin_registry::{EncoderDescriptor, EncoderSource, encoders, field},
    plugin_types::*,
};
use libheifer::error::Error;
use std::{
    ffi::{CStr, c_char, c_int, c_void},
    ptr,
    sync::LazyLock,
};
pub struct Encoder {
    pub(super) source: EncoderSource,
    pub(super) state: *mut c_void,
}
impl Drop for Encoder {
    fn drop(&mut self) {
        if !self.state.is_null()
            && let EncoderSource::External(p) = self.source
            && let Some(f) = field!(p, free_encoder)
        {
            unsafe { f(self.state) }
        }
    }
}
struct BuiltinParameter(EncoderParameter);
// SAFETY: all pointers in this immutable static point to process-lifetime strings.
unsafe impl Sync for BuiltinParameter {}
unsafe impl Send for BuiltinParameter {}
const INTERLEAVE_VALUE: EncoderParameter = EncoderParameter {
    version: 2,
    name: c"interleave".as_ptr(),
    kind: 3,
    value: ParameterValue {
        string: StringParameter {
            default_value: c"planar".as_ptr(),
            valid_values: ptr::null(),
        },
    },
    has_default: 1,
};
static INTERLEAVE: [BuiltinParameter; 2] = [
    BuiltinParameter(INTERLEAVE_VALUE),
    BuiltinParameter(INTERLEAVE_VALUE),
];
struct BuiltinList([*const EncoderParameter; 2]);
// SAFETY: the array points only to the immutable static parameter and NULL.
unsafe impl Sync for BuiltinList {}
unsafe impl Send for BuiltinList {}
static PARAMETERS: LazyLock<[BuiltinList; 2]> = LazyLock::new(|| {
    [
        BuiltinList([&INTERLEAVE[0].0, ptr::null()]),
        BuiltinList([&INTERLEAVE[1].0, ptr::null()]),
    ]
});
unsafe fn allocate(source: EncoderSource, out: *mut *mut Encoder) -> HeifError {
    let enc = Box::into_raw(Box::new(Encoder {
        source,
        state: ptr::null_mut(),
    }));
    unsafe { out.write(enc) };
    match source {
        EncoderSource::Builtin(_) => SUCCESS,
        EncoderSource::External(p) => {
            if let Some(f) = field!(p, new_encoder) {
                unsafe { f(ptr::addr_of_mut!((*enc).state)) }
            } else {
                Error::NULL.into()
            }
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_encoder(
    _ctx: *mut HeifContext,
    d: *const EncoderDescriptor,
    out: *mut *mut Encoder,
) -> HeifError {
    if d.is_null() || out.is_null() {
        return Error::NULL.into();
    }
    unsafe { allocate((*d).source, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_encoder_for_format(
    ctx: *mut HeifContext,
    format: c_int,
    out: *mut *mut Encoder,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    let list = unsafe { encoders(format, ptr::null()) };
    if let Some(d) = list.first() {
        unsafe { allocate(d.source, out) }
    } else {
        unsafe { out.write(ptr::null_mut()) };
        if let Some(ctx) = unsafe { ctx.as_ref() } {
            crate::context::report(
                &mut crate::context::lock(&ctx.shared),
                libheifer::context::ContextError::new(3, 0, "Unsupported file-type: Unspecified"),
            )
        } else {
            Error::new(3, 0, c"Unknown error").into()
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_release(p: *mut Encoder) {
    if !p.is_null() {
        unsafe { drop(Box::from_raw(p)) }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_get_name(p: *const Encoder) -> *const c_char {
    unsafe { (*p).source.name() }
}
macro_rules! direct_set {
    ($name:ident,$f:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(e: *mut Encoder, value: c_int) -> HeifError {
            let Some(e) = (unsafe { e.as_ref() }) else {
                return Error::NULL.into();
            };
            match e.source {
                EncoderSource::Builtin(_) => SUCCESS,
                EncoderSource::External(p) => {
                    field!(p, $f).map_or(SUCCESS, |f| unsafe { f(e.state, value) })
                }
            }
        }
    };
}
direct_set!(heif_encoder_set_lossy_quality, set_parameter_quality);
direct_set!(heif_encoder_set_lossless, set_parameter_lossless);
direct_set!(heif_encoder_set_logging_level, set_parameter_logging_level);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_list_parameters(
    e: *mut Encoder,
) -> *const *const EncoderParameter {
    match unsafe { (*e).source } {
        EncoderSource::Builtin(format) => PARAMETERS[usize::from(format == 9)].0.as_ptr(),
        EncoderSource::External(p) => {
            field!(p, list_parameters).map_or(ptr::null(), |f| unsafe { f((*e).state) })
        }
    }
}
struct Matching {
    list: *const *const EncoderParameter,
    name: *const c_char,
}
impl Iterator for Matching {
    type Item = *const EncoderParameter;
    fn next(&mut self) -> Option<Self::Item> {
        while !self.list.is_null() {
            let p = unsafe { *self.list };
            if p.is_null() {
                return None;
            }
            self.list = unsafe { self.list.add(1) };
            let name = field!(p, name);
            if unsafe { CStr::from_ptr(name) == CStr::from_ptr(self.name) } {
                return Some(p);
            }
        }
        None
    }
}
unsafe fn matching(e: *mut Encoder, name: *const c_char) -> Matching {
    Matching {
        list: unsafe { heif_encoder_list_parameters(e) },
        name,
    }
}

fn invalid() -> HeifError {
    Error::new(5, 2006, c"Invalid parameter value").into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_set_parameter_integer(
    e: *mut Encoder,
    name: *const c_char,
    value: c_int,
) -> HeifError {
    for p in unsafe { matching(e, name) } {
        let (mut have_min, mut have_max, mut min, mut max, mut count) = (0, 0, 0, 0, 0);
        let mut values = ptr::null();
        let error = unsafe {
            heif_encoder_parameter_get_valid_integer_values(
                p,
                &mut have_min,
                &mut have_max,
                &mut min,
                &mut max,
                &mut count,
                &mut values,
            )
        };
        if error.code != 0 {
            return error;
        }
        if (have_min != 0 && value < min) || (have_max != 0 && value > max) {
            return invalid();
        }
        if count > 0
            && !unsafe { std::slice::from_raw_parts(values, count as usize) }.contains(&value)
        {
            return invalid();
        }
    }
    match unsafe { (*e).source } {
        EncoderSource::Builtin(_) => unsupported(),
        EncoderSource::External(p) => field!(p, set_parameter_integer)
            .map_or_else(unsupported, |f| unsafe { f((*e).state, name, value) }),
    }
}
macro_rules! named_get {
    ($name:ident,$f:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            e: *mut Encoder,
            name: *const c_char,
            out: *mut c_int,
        ) -> HeifError {
            match unsafe { (*e).source } {
                EncoderSource::Builtin(_) => unsupported(),
                EncoderSource::External(p) => {
                    field!(p, $f).map_or_else(unsupported, |f| unsafe { f((*e).state, name, out) })
                }
            }
        }
    };
}
named_get!(heif_encoder_get_parameter_integer, get_parameter_integer);
named_get!(heif_encoder_get_parameter_boolean, get_parameter_boolean);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_set_parameter_boolean(
    e: *mut Encoder,
    name: *const c_char,
    value: c_int,
) -> HeifError {
    match unsafe { (*e).source } {
        EncoderSource::Builtin(_) => unsupported(),
        EncoderSource::External(p) => field!(p, set_parameter_boolean)
            .map_or_else(unsupported, |f| unsafe { f((*e).state, name, value) }),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_set_parameter_string(
    e: *mut Encoder,
    name: *const c_char,
    value: *const c_char,
) -> HeifError {
    match unsafe { (*e).source } {
        EncoderSource::Builtin(_) => unsupported(),
        EncoderSource::External(p) => field!(p, set_parameter_string)
            .map_or_else(unsupported, |f| unsafe { f((*e).state, name, value) }),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_get_parameter_string(
    e: *mut Encoder,
    name: *const c_char,
    value: *mut c_char,
    size: c_int,
) -> HeifError {
    match unsafe { (*e).source } {
        EncoderSource::Builtin(_) => unsupported(),
        EncoderSource::External(p) => field!(p, get_parameter_string)
            .map_or_else(unsupported, |f| unsafe { f((*e).state, name, value, size) }),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_parameter_integer_valid_range(
    e: *mut Encoder,
    name: *const c_char,
    have: *mut c_int,
    min: *mut c_int,
    max: *mut c_int,
) -> HeifError {
    unsafe { matching(e, name) }
        .next()
        .map_or_else(unsupported, |p| unsafe {
            heif_encoder_parameter_get_valid_integer_range(p, have, min, max)
        })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_parameter_integer_valid_values(
    e: *mut Encoder,
    name: *const c_char,
    have_min: *mut c_int,
    have_max: *mut c_int,
    min: *mut c_int,
    max: *mut c_int,
    count: *mut c_int,
    array: *mut *const c_int,
) -> HeifError {
    unsafe { matching(e, name) }
        .next()
        .map_or_else(unsupported, |p| unsafe {
            heif_encoder_parameter_get_valid_integer_values(
                p, have_min, have_max, min, max, count, array,
            )
        })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_parameter_string_valid_values(
    e: *mut Encoder,
    name: *const c_char,
    array: *mut *const *const c_char,
) -> HeifError {
    unsafe { matching(e, name) }
        .next()
        .map_or_else(unsupported, |p| unsafe {
            heif_encoder_parameter_get_valid_string_values(p, array)
        })
}
fn atoi(bytes: &[u8]) -> c_int {
    let start = bytes
        .iter()
        .position(|b| !matches!(*b, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12))
        .unwrap_or(bytes.len());
    let mut tail = &bytes[start..];
    let negative = tail.first() == Some(&b'-');
    if matches!(tail.first(), Some(b'+' | b'-')) {
        tail = &tail[1..];
    }
    let mut n = 0i32;
    for &b in tail {
        if !b.is_ascii_digit() {
            break;
        }
        n = n.wrapping_mul(10).wrapping_add((b - b'0') as i32);
    }
    if negative { n.wrapping_neg() } else { n }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_set_parameter(
    e: *mut Encoder,
    name: *const c_char,
    value: *const c_char,
) -> HeifError {
    if let Some(p) = unsafe { matching(e, name) }.next() {
        match field!(p, kind) {
            1 => unsafe {
                heif_encoder_set_parameter_integer(e, name, atoi(CStr::from_ptr(value).to_bytes()))
            },
            2 => unsafe {
                heif_encoder_set_parameter_boolean(
                    e,
                    name,
                    i32::from(matches!(CStr::from_ptr(value).to_bytes(), b"true" | b"1")),
                )
            },
            3 => unsafe { heif_encoder_set_parameter_string(e, name, value) },
            _ => SUCCESS,
        }
    } else {
        unsafe { heif_encoder_set_parameter_string(e, name, value) }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_get_parameter(
    e: *mut Encoder,
    name: *const c_char,
    out: *mut c_char,
    size: c_int,
) -> HeifError {
    let mut params = unsafe { matching(e, name) };
    let Some(p) = params.next() else {
        return unsupported();
    };
    match field!(p, kind) {
        1 | 2 => {
            let mut value = 0;
            let error = if field!(p, kind) == 1 {
                unsafe { heif_encoder_get_parameter_integer(e, name, &mut value) }
            } else {
                unsafe { heif_encoder_get_parameter_boolean(e, name, &mut value) }
            };
            if error.code != 0 {
                return error;
            }
            if size > 0 && !out.is_null() {
                let text = value.to_string();
                let n = text.len().min(size as usize - 1);
                unsafe {
                    ptr::copy_nonoverlapping(text.as_ptr().cast(), out, n);
                    out.add(n).write(0)
                }
            }
        }
        3 => {
            let error = unsafe { heif_encoder_get_parameter_string(e, name, out, size) };
            if error.code != 0 {
                return error;
            }
        }
        _ => {}
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_has_default(e: *mut Encoder, name: *const c_char) -> c_int {
    unsafe { matching(e, name) }.next().map_or(0, |p| {
        if field!(p, version) < 2 {
            1
        } else {
            field!(p, has_default)
        }
    })
}
