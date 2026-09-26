// SPDX-License-Identifier: LGPL-3.0-or-later
use super::{HeifError, SUCCESS};
use libheifer::{error::Error, image::Image, sensor::*};
use std::{ffi::c_int, ptr};

unsafe fn owned<T: Copy>(data: *const T, count: usize) -> Result<Vec<T>, Error> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if data.is_null() {
        return Err(Error::NULL);
    }
    let mut out = Vec::new();
    out.try_reserve_exact(count)
        .map_err(|_| Error::ALLOCATION)?;
    out.extend_from_slice(unsafe { std::slice::from_raw_parts(data, count) });
    Ok(out)
}
unsafe fn write<T: Copy>(out: *mut T, value: T) {
    if !out.is_null() {
        unsafe {
            out.write(value);
        }
    }
}
unsafe fn copy<T: Copy>(out: *mut T, data: &[T]) {
    if !out.is_null() && !data.is_empty() {
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), out, data.len());
        }
    }
}
macro_rules! attempt {
    ($value:expr) => {
        match $value {
            Ok(v) => v,
            Err(e) => return e.into(),
        }
    };
}
fn push<T>(out: &mut Vec<T>, value: T) -> HeifError {
    if out.try_reserve(1).is_err() {
        return Error::ALLOCATION.into();
    }
    out.push(value);
    SUCCESS
}
const POLARIZATION_INDEX: Error = Error::new(5, 2006, c"Polarization pattern index out of range.");
const BAD_PIXEL_INDEX: Error = Error::new(5, 2006, c"Sensor bad pixels map index out of range.");
const NUC_INDEX: Error = Error::new(5, 2006, c"Sensor NUC index out of range.");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_bayer_pattern(
    image: *mut Image,
    _component: u32,
    width: u16,
    height: u16,
    pixels: *const BayerPixel,
) -> HeifError {
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    if pixels.is_null() {
        return Error::NULL.into();
    }
    if width == 0 || height == 0 {
        return Error::new(5, 2006, c"Bayer pattern dimensions must be non-zero.").into();
    }
    let pixels = attempt!(unsafe { owned(pixels, usize::from(width) * usize::from(height)) });
    image.sensor.bayer = Some(BayerPattern {
        width,
        height,
        pixels,
    });
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_add_bayer_component(
    image: *mut Image,
    kind: u16,
    out: *mut u32,
) -> HeifError {
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let id = attempt!(image.component_ids.add_reference(kind));
    unsafe {
        out.write(id);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_bayer_pattern_size(
    image: *const Image,
    _component: u32,
    width: *mut u16,
    height: *mut u16,
) -> c_int {
    let pattern = unsafe { image.as_ref() }.and_then(|i| i.sensor.bayer.as_ref());
    unsafe {
        write(width, pattern.map_or(0, |p| p.width));
        write(height, pattern.map_or(0, |p| p.height));
    }
    pattern.is_some().into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_bayer_pattern(
    image: *const Image,
    _component: u32,
    out: *mut BayerPixel,
) -> HeifError {
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let Some(pattern) = &image.sensor.bayer else {
        return Error::new(5, 2006, c"Image does not have a Bayer pattern.").into();
    };
    unsafe {
        copy(out, &pattern.pixels);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_polarization_angle_no_filter() -> f32 {
    f32::from_bits(u32::MAX)
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_polarization_angle_is_no_filter(value: f32) -> c_int {
    (value.to_bits() == u32::MAX).into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_add_polarization_pattern(
    image: *mut Image,
    count: u32,
    components: *const u32,
    width: u16,
    height: u16,
    angles: *const f32,
) -> HeifError {
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    if angles.is_null() || (count != 0 && components.is_null()) {
        return Error::NULL.into();
    }
    if width == 0 || height == 0 {
        return Error::new(
            5,
            2006,
            c"Polarization pattern dimensions must be non-zero.",
        )
        .into();
    }
    let components = attempt!(unsafe { owned(components, count as usize) });
    let angles = attempt!(unsafe { owned(angles, usize::from(width) * usize::from(height)) });
    push(
        &mut image.sensor.polarization,
        PolarizationPattern {
            components,
            width,
            height,
            angles,
        },
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_number_of_polarization_patterns(
    image: *const Image,
) -> c_int {
    unsafe { image.as_ref() }.map_or(0, |i| i.sensor.polarization.len() as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_polarization_pattern_info(
    image: *const Image,
    index: c_int,
    count: *mut u32,
    width: *mut u16,
    height: *mut u16,
) -> HeifError {
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(p) = image.sensor.polarization.get(index as usize) else {
        return POLARIZATION_INDEX.into();
    };
    unsafe {
        write(count, p.components.len() as u32);
        write(width, p.width);
        write(height, p.height);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_polarization_pattern_data(
    image: *const Image,
    index: c_int,
    components: *mut u32,
    angles: *mut f32,
) -> HeifError {
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    if angles.is_null() {
        return Error::NULL.into();
    }
    let Some(p) = image.sensor.polarization.get(index as usize) else {
        return POLARIZATION_INDEX.into();
    };
    unsafe {
        copy(components, &p.components);
        copy(angles, &p.angles);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_polarization_pattern_index_for_component(
    image: *const Image,
    component: u32,
) -> c_int {
    unsafe { image.as_ref() }
        .and_then(|i| i.sensor.polarization_for(component))
        .map_or(-1, |i| i as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_add_sensor_bad_pixels_map(
    image: *mut Image,
    count: u32,
    components: *const u32,
    applied: c_int,
    row_count: u32,
    rows: *const u32,
    column_count: u32,
    columns: *const u32,
    pixel_count: u32,
    pixels: *const BadPixel,
) -> HeifError {
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    if (count != 0 && components.is_null())
        || (row_count != 0 && rows.is_null())
        || (column_count != 0 && columns.is_null())
        || (pixel_count != 0 && pixels.is_null())
    {
        return Error::NULL.into();
    }
    let components = attempt!(unsafe { owned(components, count as usize) });
    let rows = attempt!(unsafe { owned(rows, row_count as usize) });
    let columns = attempt!(unsafe { owned(columns, column_count as usize) });
    let pixels = attempt!(unsafe { owned(pixels, pixel_count as usize) });
    push(
        &mut image.sensor.bad_pixels,
        BadPixelsMap {
            components,
            applied: applied != 0,
            rows,
            columns,
            pixels,
        },
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_number_of_sensor_bad_pixels_maps(
    image: *const Image,
) -> c_int {
    unsafe { image.as_ref() }.map_or(0, |i| i.sensor.bad_pixels.len() as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_sensor_bad_pixels_map_info(
    image: *const Image,
    index: c_int,
    count: *mut u32,
    applied: *mut c_int,
    rows: *mut u32,
    columns: *mut u32,
    pixels: *mut u32,
) -> HeifError {
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(p) = image.sensor.bad_pixels.get(index as usize) else {
        return BAD_PIXEL_INDEX.into();
    };
    unsafe {
        write(count, p.components.len() as u32);
        write(applied, p.applied.into());
        write(rows, p.rows.len() as u32);
        write(columns, p.columns.len() as u32);
        write(pixels, p.pixels.len() as u32);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_sensor_bad_pixels_map_data(
    image: *const Image,
    index: c_int,
    components: *mut u32,
    rows: *mut u32,
    columns: *mut u32,
    pixels: *mut BadPixel,
) -> HeifError {
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(p) = image.sensor.bad_pixels.get(index as usize) else {
        return BAD_PIXEL_INDEX.into();
    };
    unsafe {
        copy(components, &p.components);
        copy(rows, &p.rows);
        copy(columns, &p.columns);
        copy(pixels, &p.pixels);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_add_sensor_nuc(
    image: *mut Image,
    count: u32,
    components: *const u32,
    applied: c_int,
    width: u32,
    height: u32,
    gains: *const f32,
    offsets: *const f32,
) -> HeifError {
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    if gains.is_null() || offsets.is_null() || (count != 0 && components.is_null()) {
        return Error::NULL.into();
    }
    if width == 0 || height == 0 {
        return Error::new(5, 2006, c"NUC image dimensions must be non-zero.").into();
    }
    let Some(pixels) = (width as usize).checked_mul(height as usize) else {
        return Error::ALLOCATION.into();
    };
    let components = attempt!(unsafe { owned(components, count as usize) });
    let gains = attempt!(unsafe { owned(gains, pixels) });
    let offsets = attempt!(unsafe { owned(offsets, pixels) });
    push(
        &mut image.sensor.nuc,
        NonUniformityCorrection {
            components,
            applied: applied != 0,
            width,
            height,
            gains,
            offsets,
        },
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_number_of_sensor_nucs(image: *const Image) -> c_int {
    unsafe { image.as_ref() }.map_or(0, |i| i.sensor.nuc.len() as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_sensor_nuc_info(
    image: *const Image,
    index: c_int,
    count: *mut u32,
    applied: *mut c_int,
    width: *mut u32,
    height: *mut u32,
) -> HeifError {
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(p) = image.sensor.nuc.get(index as usize) else {
        return NUC_INDEX.into();
    };
    unsafe {
        write(count, p.components.len() as u32);
        write(applied, p.applied.into());
        write(width, p.width);
        write(height, p.height);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_sensor_nuc_data(
    image: *const Image,
    index: c_int,
    components: *mut u32,
    gains: *mut f32,
    offsets: *mut f32,
) -> HeifError {
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(p) = image.sensor.nuc.get(index as usize) else {
        return NUC_INDEX.into();
    };
    unsafe {
        copy(components, &p.components);
        copy(gains, &p.gains);
        copy(offsets, &p.offsets);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_chroma_location(
    image: *mut Image,
    location: u8,
) -> HeifError {
    let Some(image) = (unsafe { image.as_mut() }) else {
        return Error::NULL.into();
    };
    if location > 6 {
        return Error::new(5, 2006, c"Chroma location must be in the range 0-6.").into();
    }
    image.sensor.chroma_location = Some(location);
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_has_chroma_location(image: *const Image) -> c_int {
    unsafe { image.as_ref() }
        .is_some_and(|i| i.sensor.chroma_location.is_some())
        .into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_chroma_location(image: *const Image) -> u8 {
    unsafe { image.as_ref() }
        .and_then(|i| i.sensor.chroma_location)
        .unwrap_or(0)
}
