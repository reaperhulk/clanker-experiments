// SPDX-License-Identifier: LGPL-3.0-or-later
use super::{
    HeifError, SUCCESS,
    context::{HeifContext, lock, report},
};
use libheifer::{error::Error, properties::Property};
use std::{
    ffi::{CStr, CString, c_char, c_int},
    ptr,
};

#[repr(C)]
pub struct UserDescription {
    pub version: c_int,
    pub lang: *const c_char,
    pub name: *const c_char,
    pub description: *const c_char,
    pub tags: *const c_char,
}
#[repr(C)]
struct OwnedDescription {
    public: UserDescription,
    strings: [CString; 4],
}

unsafe fn list(
    context: *const HeifContext,
    id: u32,
    filter: impl Fn([u8; 4]) -> bool,
    out: *mut u32,
    count: c_int,
) -> c_int {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return 0;
    };
    let state = lock(&context.shared);
    let Ok(properties) = state.properties.get(id) else {
        return 0;
    };
    let mut n = 0;
    for (index, property) in properties.iter().enumerate() {
        if filter(property.kind) {
            if out.is_null() {
                n += 1;
            } else if n < count {
                unsafe { out.add(n as usize).write(index as u32 + 1) };
                n += 1;
            }
        }
    }
    n
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_properties_of_type(
    context: *const HeifContext,
    id: u32,
    kind: u32,
    out: *mut u32,
    count: c_int,
) -> c_int {
    unsafe {
        list(
            context,
            id,
            |k| kind == 0 || kind == u32::from_be_bytes(k),
            out,
            count,
        )
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_transformation_properties(
    context: *const HeifContext,
    id: u32,
    out: *mut u32,
    count: c_int,
) -> c_int {
    unsafe {
        list(
            context,
            id,
            |k| matches!(&k, b"irot" | b"imir" | b"clap"),
            out,
            count,
        )
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_type(
    context: *const HeifContext,
    id: u32,
    property: u32,
) -> u32 {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return 0;
    };
    lock(&context.shared)
        .properties
        .get(id)
        .ok()
        .and_then(|p| {
            property
                .checked_sub(1)
                .and_then(|i| p.get(i as usize))
                .map(|p| u32::from_be_bytes(p.kind))
        })
        .unwrap_or(0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_user_description(
    context: *const HeifContext,
    id: u32,
    property: u32,
    out: *mut *mut UserDescription,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&context.shared);
    let strings = match state.properties.find(id, property, Some(*b"udes")) {
        Ok(p) => p.description(),
        Err(e) => return report(&mut state, e),
    };
    let value = OwnedDescription {
        public: UserDescription {
            version: 1,
            lang: strings[0].as_ptr(),
            name: strings[1].as_ptr(),
            description: strings[2].as_ptr(),
            tags: strings[3].as_ptr(),
        },
        strings,
    };
    let value = super::color::allocate(value);
    if value.is_null() {
        return Error::ALLOCATION.into();
    }
    unsafe { out.write(value.cast()) };
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_property_user_description_release(value: *mut UserDescription) {
    if !value.is_null() {
        unsafe { drop(Box::from_raw(value.cast::<OwnedDescription>())) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_add_property_user_description(
    context: *const HeifContext,
    id: u32,
    description: *const UserDescription,
    out: *mut u32,
) -> HeifError {
    let (Some(context), Some(description)) =
        (unsafe { context.as_ref() }, unsafe { description.as_ref() })
    else {
        return Error::NULL.into();
    };
    let strings = [
        description.lang,
        description.name,
        description.description,
        description.tags,
    ]
    .map(|s| {
        if s.is_null() {
            &[][..]
        } else {
            unsafe { CStr::from_ptr(s) }.to_bytes()
        }
    });
    let mut state = lock(&context.shared);
    match state
        .properties
        .add(id, Property::user_description(strings), false)
    {
        Ok(id) => {
            if !out.is_null() {
                unsafe { out.write(id) };
            }
            SUCCESS
        }
        Err(e) => report(&mut state, e),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_add_raw_property(
    context: *const HeifContext,
    id: u32,
    kind: u32,
    uuid: *const u8,
    data: *const u8,
    size: usize,
    essential: c_int,
    out: *mut u32,
) -> HeifError {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    if data.is_null() || (kind == u32::from_be_bytes(*b"uuid") && uuid.is_null()) {
        return Error::NULL.into();
    }
    if size > isize::MAX as usize {
        return Error::ALLOCATION.into();
    }
    let uuid = if kind == u32::from_be_bytes(*b"uuid") {
        Some(unsafe { ptr::read_unaligned(uuid.cast::<[u8; 16]>()) })
    } else {
        None
    };
    let mut owned = Vec::new();
    if owned.try_reserve_exact(size).is_err() {
        return Error::ALLOCATION.into();
    }
    owned.extend_from_slice(unsafe { std::slice::from_raw_parts(data, size) });
    let mut state = lock(&context.shared);
    match state.properties.add(
        id,
        Property {
            kind: kind.to_be_bytes(),
            uuid,
            data: owned,
            raw: true,
            tai: None,
            write_error: None,
            gimi_components: None,
        },
        essential != 0,
    ) {
        Ok(id) => {
            if !out.is_null() {
                unsafe { out.write(id) };
            }
            SUCCESS
        }
        Err(e) => report(&mut state, e),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_raw_size(
    context: *const HeifContext,
    id: u32,
    property: u32,
    out: *mut usize,
) -> HeifError {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let mut state = lock(&context.shared);
    match state.properties.find(id, property, None) {
        Ok(p) => {
            unsafe { out.write(p.data.len()) };
            SUCCESS
        }
        Err(e) => report(&mut state, e),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_raw_data(
    context: *const HeifContext,
    id: u32,
    property: u32,
    out: *mut u8,
) -> HeifError {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let mut state = lock(&context.shared);
    match state.properties.find(id, property, None) {
        Ok(p) => {
            unsafe { ptr::copy_nonoverlapping(p.data.as_ptr(), out, p.data.len()) };
            SUCCESS
        }
        Err(e) => report(&mut state, e),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_uuid_type(
    context: *const HeifContext,
    id: u32,
    property: u32,
    out: *mut u8,
) -> HeifError {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    let mut state = lock(&context.shared);
    match state.properties.find(id, property, None) {
        Ok(p) => {
            if let Some(uuid) = p.uuid {
                unsafe { ptr::copy_nonoverlapping(uuid.as_ptr(), out, 16) };
            }
            SUCCESS
        }
        Err(e) => report(&mut state, e),
    }
}
unsafe fn transform(
    context: *const HeifContext,
    id: u32,
    property: u32,
    kind: [u8; 4],
    mask: u8,
    scale: c_int,
) -> c_int {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return -1;
    };
    lock(&context.shared)
        .properties
        .find(id, property, Some(kind))
        .ok()
        .and_then(|p| p.data.first())
        .map_or(-1, |v| c_int::from(v & mask) * scale)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_transform_mirror(
    context: *const HeifContext,
    id: u32,
    property: u32,
) -> c_int {
    unsafe { transform(context, id, property, *b"imir", 1, 1) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_transform_rotation_ccw(
    context: *const HeifContext,
    id: u32,
    property: u32,
) -> c_int {
    unsafe { transform(context, id, property, *b"irot", 3, 90) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_transform_crop_borders(
    context: *const HeifContext,
    id: u32,
    property: u32,
    width: c_int,
    height: c_int,
    left: *mut c_int,
    top: *mut c_int,
    right: *mut c_int,
    bottom: *mut c_int,
) {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return;
    };
    let state = lock(&context.shared);
    let Ok(p) = state.properties.find(id, property, Some(*b"clap")) else {
        return;
    };
    let borders = libheifer::geometry::CleanAperture::parse(&p.data)
        .and_then(|c| c.unclamped_crop(width as u32, height as u32))
        .map_or([0; 4], |(l, r, t, b)| {
            [
                l as c_int,
                t as c_int,
                (i64::from(width) - 1 - r) as c_int,
                (i64::from(height) - 1 - b) as c_int,
            ]
        });
    for (out, value) in [left, top, right, bottom].into_iter().zip(borders) {
        if !out.is_null() {
            unsafe { out.write(value) };
        }
    }
}
