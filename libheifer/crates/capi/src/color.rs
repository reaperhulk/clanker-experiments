// SPDX-License-Identifier: LGPL-3.0-or-later
use super::{HeifError, SUCCESS};
use libheifer::{color::*, error::Error, image::Image};
use std::{
    alloc::{Layout, alloc},
    ffi::{CStr, c_char, c_int, c_void},
    ptr,
};

const PROFILE_MISSING: Error = Error::new(10, 0, c"Color profile does not exist: Unspecified");
const ANONYMOUS_SUCCESS: HeifError = HeifError {
    code: 0,
    subcode: 0,
    message: c"Unknown error".as_ptr(),
};

pub(super) fn allocate<T>(value: T) -> *mut T {
    // All callers use sized, nonzero structs. Matching releases use Box<T>.
    let allocation = unsafe { alloc(Layout::new::<T>()) }.cast::<T>();
    if !allocation.is_null() {
        unsafe { allocation.write(value) };
    }
    allocation
}

#[unsafe(no_mangle)]
pub extern "C" fn heif_nclx_color_profile_alloc() -> *mut NclxProfile {
    allocate(NclxProfile::default())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_nclx_color_profile_free(profile: *mut NclxProfile) {
    if !profile.is_null() {
        unsafe { drop(Box::from_raw(profile)) };
    }
}

macro_rules! nclx_setter {
    ($name:ident, $field:ident, $validate:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(profile: *mut NclxProfile, value: u16) -> HeifError {
            if profile.is_null() {
                return Error::NULL.into();
            }
            // A C-allocated profile may have uninitialized decoded coordinates.
            // Access only this initialized scalar, without borrowing the struct.
            match $validate(value) {
                Ok(()) => {
                    unsafe { ptr::addr_of_mut!((*profile).$field).write(value.into()) };
                    ANONYMOUS_SUCCESS
                }
                Err(e) => {
                    unsafe { ptr::addr_of_mut!((*profile).$field).write(2) };
                    e.into()
                }
            }
        }
    };
}
nclx_setter!(
    heif_nclx_color_profile_set_color_primaries,
    color_primaries,
    validate_primaries
);
nclx_setter!(
    heif_nclx_color_profile_set_transfer_characteristics,
    transfer_characteristics,
    validate_transfer
);
nclx_setter!(
    heif_nclx_color_profile_set_matrix_coefficients,
    matrix_coefficients,
    validate_matrix
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_color_profile_type(image: *const Image) -> u32 {
    unsafe { image.as_ref() }.map_or(0, |i| i.color.profile_type())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_raw_color_profile_size(image: *const Image) -> usize {
    unsafe { image.as_ref() }
        .and_then(|i| i.color.raw.as_ref())
        .map_or(0, |p| p.data.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_raw_color_profile(
    image: *const Image,
    output: *mut c_void,
) -> HeifError {
    if output.is_null() {
        return Error::NULL.into();
    }
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(profile) = &image.color.raw else {
        return PROFILE_MISSING.into();
    };
    unsafe { ptr::copy_nonoverlapping(profile.data.as_ptr(), output.cast(), profile.data.len()) };
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_raw_color_profile(
    image: *mut Image,
    profile_type: *const c_char,
    data: *const c_void,
    len: usize,
) -> HeifError {
    if profile_type.is_null() {
        return Error::NULL.into();
    }
    let name = unsafe { CStr::from_ptr(profile_type) }.to_bytes();
    if name.len() != 4 {
        return Error::new(5, 0, c"Invalid color_profile_type (must be 4 characters)").into();
    }
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    if data.is_null() && len != 0 {
        return Error::NULL.into();
    }
    if len > isize::MAX as usize {
        return Error::ALLOCATION.into();
    }
    let data = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }
    };
    match image
        .color
        .set_raw(u32::from_be_bytes(name.try_into().unwrap()), data)
    {
        Ok(()) => SUCCESS,
        Err(e) => e.into(),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_nclx_color_profile(
    image: *mut Image,
    profile: *const NclxProfile,
) -> HeifError {
    if profile.is_null() {
        return Error::NULL.into();
    }
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    // Do not copy decoded floats: upstream's allocator leaves them unspecified.
    image.color.nclx = Some(Nclx {
        primaries: unsafe { ptr::addr_of!((*profile).color_primaries).read() } as u16,
        transfer: unsafe { ptr::addr_of!((*profile).transfer_characteristics).read() } as u16,
        matrix: unsafe { ptr::addr_of!((*profile).matrix_coefficients).read() } as u16,
        full_range: unsafe { ptr::addr_of!((*profile).full_range_flag).read() } != 0,
    });
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_nclx_color_profile(
    image: *const Image,
    output: *mut *mut NclxProfile,
) -> HeifError {
    if output.is_null() {
        return Error::NULL.into();
    }
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(profile) = image.color.nclx.filter(|p| p.is_defined()) else {
        return PROFILE_MISSING.into();
    };
    unsafe { output.write(ptr::null_mut()) };
    match profile.decode() {
        Ok(decoded) => {
            let allocated = allocate(decoded);
            if allocated.is_null() {
                return Error::ALLOCATION.into();
            }
            unsafe { output.write(allocated) };
            SUCCESS
        }
        Err(e) => e.into(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_has_content_light_level(image: *const Image) -> c_int {
    unsafe { image.as_ref() }
        .is_some_and(|i| i.color.has_content_light())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_content_light_level(
    image: *const Image,
    output: *mut ContentLightLevel,
) {
    if let (Some(image), Some(output)) = (unsafe { image.as_ref() }, unsafe { output.as_mut() }) {
        *output = image.color.content_light;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_content_light_level(
    image: *const Image,
    input: *const ContentLightLevel,
) {
    if let (Some(image), Some(input)) = (unsafe { image.cast_mut().as_mut() }, unsafe {
        input.as_ref()
    }) {
        image.color.content_light = *input;
    }
}

macro_rules! hdr_property {
    ($has:ident, $get:ident, $set:ident, $field:ident, $type:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $has(image: *const Image) -> c_int {
            unsafe { image.as_ref() }
                .is_some_and(|i| i.color.$field.is_some())
                .into()
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(image: *const Image, output: *mut $type) -> c_int {
            let Some(value) = (unsafe { image.as_ref() }).and_then(|i| i.color.$field) else {
                return 0;
            };
            if let Some(output) = unsafe { output.as_mut() } {
                *output = value;
            }
            1
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(image: *const Image, input: *const $type) {
            if let (Some(image), Some(input)) = (unsafe { image.cast_mut().as_mut() }, unsafe {
                input.as_ref()
            }) {
                image.color.$field = Some(*input);
            }
        }
    };
}
hdr_property!(
    heif_image_has_ambient_viewing_environment,
    heif_image_get_ambient_viewing_environment,
    heif_image_set_ambient_viewing_environment,
    ambient,
    AmbientViewingEnvironment
);
// Mastering-display getter has a void ABI, unlike the ambient getter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_has_mastering_display_colour_volume(
    image: *const Image,
) -> c_int {
    unsafe { image.as_ref() }
        .is_some_and(|i| i.color.mastering.is_some())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_mastering_display_colour_volume(
    image: *const Image,
    output: *mut MasteringDisplayColourVolume,
) {
    if let (Some(value), Some(output)) = (
        (unsafe { image.as_ref() }).and_then(|i| i.color.mastering),
        unsafe { output.as_mut() },
    ) {
        *output = value;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_mastering_display_colour_volume(
    image: *const Image,
    input: *const MasteringDisplayColourVolume,
) {
    if let (Some(image), Some(input)) = (unsafe { image.cast_mut().as_mut() }, unsafe {
        input.as_ref()
    }) {
        image.color.mastering = Some(*input);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_has_nominal_diffuse_white_luminance(
    image: *const Image,
) -> c_int {
    unsafe { image.as_ref() }
        .is_some_and(|i| i.color.diffuse_white.is_some())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_nominal_diffuse_white_luminance(
    image: *const Image,
) -> u32 {
    unsafe { image.as_ref() }
        .and_then(|i| i.color.diffuse_white)
        .unwrap_or(0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_nominal_diffuse_white_luminance(
    image: *const Image,
    value: u32,
) {
    if let Some(image) = unsafe { image.cast_mut().as_mut() } {
        image.color.diffuse_white = Some(value);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_mastering_display_colour_volume_decode(
    input: *const MasteringDisplayColourVolume,
    output: *mut DecodedMasteringDisplayColourVolume,
) -> HeifError {
    let (Some(input), Some(output)) = (unsafe { input.as_ref() }, unsafe { output.as_mut() })
    else {
        return Error::NULL.into();
    };
    *output = input.decode();
    SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_color_conversion_options_set_defaults(
    options: *mut ColorConversionOptions,
) {
    if let Some(options) = unsafe { options.as_mut() } {
        *options = ColorConversionOptions::default();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_color_conversion_options_ext_alloc() -> *mut ColorConversionOptionsExt {
    allocate(ColorConversionOptionsExt::default())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_color_conversion_options_ext_copy(
    dst: *mut ColorConversionOptionsExt,
    src: *const ColorConversionOptionsExt,
) {
    if src.is_null() || dst.is_null() {
        return;
    }
    // Only the version byte may exist in an older layout. Do not copy fields
    // before determining that both structures have the version-1 payload.
    let src_version = unsafe { ptr::addr_of!((*src).version).read() };
    let dst_version = unsafe { ptr::addr_of!((*dst).version).read() };
    if src_version.min(dst_version) != 1 {
        return;
    }
    // Read before borrowing the destination, including the valid src == dst case.
    let Some(src) = (unsafe { src.as_ref() }).copied() else {
        return;
    };
    if let Some(dst) = unsafe { dst.as_mut() } {
        dst.copy_from_versioned(src);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_color_conversion_options_ext_free(
    options: *mut ColorConversionOptionsExt,
) {
    if !options.is_null() {
        unsafe { drop(Box::from_raw(options)) };
    }
}
