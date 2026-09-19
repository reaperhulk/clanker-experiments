// SPDX-License-Identifier: LGPL-3.0-or-later
//! Optional C ABI. Pointer contracts are the contracts in the pinned public headers.
#![allow(clippy::missing_safety_doc)] // Shared contract below applies to C entry points.
// SAFETY CONTRACT: Non-null input pointers reference the documented readable length;
// output pointers reference aligned writable objects; allocated results are released
// once with the corresponding function from this library. Borrowed buffers may not
// be concurrently mutated. Invalid non-null pointers are outside the C contract.

use libheifer::{brands, error::Error};
use std::alloc::{Layout, alloc, dealloc};
use std::ffi::{c_char, c_int};
use std::ptr;

mod auxiliary;
pub use auxiliary::DepthRepresentationInfo;
mod camera;
mod color;
mod components;
mod context;
mod decoding;
mod image;
mod items;
mod properties;
mod security;
mod sensor;
pub use decoding::DecodingOptions;
pub use properties::UserDescription;
pub use security::SecurityLimits;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HeifError {
    pub code: c_int,
    pub subcode: c_int,
    pub message: *const c_char,
}

// Object-bound errors use their owning context/image's buffer. This fallback
// retains diagnostics from operations which have no owning object.
thread_local! {
    static LAST_ERROR: std::cell::RefCell<std::ffi::CString> = std::cell::RefCell::new(std::ffi::CString::default());
}
impl From<Error> for HeifError {
    fn from(e: Error) -> Self {
        Self {
            code: e.code,
            subcode: e.subcode,
            message: match e.message {
                std::borrow::Cow::Borrowed(message) => message.as_ptr(),
                std::borrow::Cow::Owned(message) => LAST_ERROR.with(|slot| {
                    *slot.borrow_mut() = message;
                    slot.borrow().as_ptr()
                }),
            },
        }
    }
}
const SUCCESS: HeifError = HeifError {
    code: 0,
    subcode: 0,
    message: c"Success".as_ptr(),
};

#[repr(transparent)]
pub struct StaticError(HeifError);
// SAFETY: the only instance points to an immutable process-lifetime string.
unsafe impl Sync for StaticError {}
#[unsafe(no_mangle)]
pub static heif_error_success: StaticError = StaticError(SUCCESS);

// Negative lengths do not become enormous Rust slices. A null pointer produces an
// empty slice without calling from_raw_parts(null, 0), which is undefined in Rust.
unsafe fn input<'a>(data: *const u8, len: c_int) -> &'a [u8] {
    if data.is_null() || len <= 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, len as usize) }
    }
}

// Upstream checks bytes in sequence and stops at the first NUL. Do not read four
// bytes unconditionally: a pointer to a short NUL-terminated string is valid here.
unsafe fn read_fourcc(data: *const c_char) -> Option<[u8; 4]> {
    if data.is_null() {
        return None;
    }
    let mut value = [0; 4];
    for (i, byte) in value.iter_mut().enumerate() {
        *byte = unsafe { *data.add(i) as u8 };
        if *byte == 0 {
            return None;
        }
    }
    Some(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn heif_get_version() -> *const c_char {
    c"1.23.4".as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_get_version_number() -> u32 {
    (1 << 24) | (23 << 16) | (4 << 8)
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_get_version_number_major() -> c_int {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_get_version_number_minor() -> c_int {
    23
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_get_version_number_maintenance() -> c_int {
    4
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_read_main_brand(data: *const u8, len: c_int) -> u32 {
    brands::main_brand(unsafe { input(data, len) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_read_minor_version_brand(data: *const u8, len: c_int) -> u32 {
    brands::minor_version_brand(unsafe { input(data, len) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_fourcc_to_brand(data: *const c_char) -> u32 {
    unsafe { read_fourcc(data) }.map_or(0, brands::fourcc)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_brand_to_fourcc(brand: u32, output: *mut c_char) {
    if !output.is_null() {
        unsafe { ptr::copy_nonoverlapping(brand.to_be_bytes().as_ptr(), output.cast(), 4) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_main_brand(data: *const u8, len: c_int) -> c_int {
    brands::legacy_brand(unsafe { input(data, len) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_check_filetype(data: *const u8, len: c_int) -> c_int {
    brands::check_filetype(unsafe { input(data, len) }) as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_check_jpeg_filetype(data: *const u8, len: c_int) -> c_int {
    brands::check_jpeg(unsafe { input(data, len) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_get_file_mime_type(data: *const u8, len: c_int) -> *const c_char {
    brands::mime_type(unsafe { input(data, len) }).as_ptr()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_has_compatible_brand(
    data: *const u8,
    len: c_int,
    brand: *const c_char,
) -> c_int {
    let Some(brand) = (unsafe { read_fourcc(brand) }) else {
        return -1;
    };
    brands::has_compatible_brand(unsafe { input(data, len) }, brand)
}

// The allocation stores its Layout size before the public u32 array. This permits
// a matching Rust deallocation without C malloc/free or a global pointer registry.
fn brand_layout(count: usize) -> Layout {
    Layout::from_size_align(
        std::mem::size_of::<usize>() + count * 4,
        std::mem::align_of::<usize>(),
    )
    .unwrap()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_list_compatible_brands(
    data: *const u8,
    len: c_int,
    out: *mut *mut u32,
    size: *mut c_int,
) -> HeifError {
    if data.is_null() || out.is_null() || size.is_null() {
        return Error::NULL.into();
    }
    let brands = match brands::CompatibleBrands::parse(unsafe { input(data, len) }) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let count = brands.len();
    let layout = brand_layout(count);
    let allocation = unsafe { alloc(layout) };
    if allocation.is_null() {
        return Error::ALLOCATION.into();
    }
    let values = unsafe { allocation.add(std::mem::size_of::<usize>()).cast::<u32>() };
    unsafe { allocation.cast::<usize>().write(count) };
    for (i, brand) in brands.iter().enumerate() {
        unsafe { values.add(i).write(brand) };
    }
    unsafe {
        out.write(values);
        size.write(count as c_int);
    }
    SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_free_list_of_compatible_brands(brands: *mut u32) {
    if brands.is_null() {
        return;
    }
    let allocation = unsafe { brands.cast::<u8>().sub(std::mem::size_of::<usize>()) };
    let count = unsafe { allocation.cast::<usize>().read() };
    unsafe { dealloc(allocation, brand_layout(count)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_has_compatible_filetype(data: *const u8, len: c_int) -> HeifError {
    if data.is_null() {
        return Error::NULL.into();
    }
    match brands::has_compatible_filetype(unsafe { input(data, len) }) {
        Ok(()) => SUCCESS,
        Err(e) => e.into(),
    }
}
