// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, lock, report},
};
use libheifer::{
    error::Error,
    image::Image,
    tai::{ClockInfo, TaiProperty, Timestamp},
};
use std::ptr;

macro_rules! versioned {
    ($ty:ty, $alloc:ident, $copy:ident, $release:ident, $($field:ident),+) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $alloc() -> *mut $ty { Box::into_raw(Box::new(<$ty>::default())) }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $copy(dst: *mut $ty, src: *const $ty) {
            if dst.is_null() || src.is_null() { return; }
            // Version zero may be backed by only the one-byte struct prefix.
            if unsafe { dst.cast::<u8>().read() } == 0 || unsafe { src.cast::<u8>().read() } == 0 { return; }
            $(let $field = unsafe { ptr::addr_of!((*src).$field).read() };)+
            $(unsafe { ptr::addr_of_mut!((*dst).$field).write($field); })+
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $release(value: *mut $ty) {
            if !value.is_null() { drop(unsafe { Box::from_raw(value) }); }
        }
    };
}
versioned!(
    ClockInfo,
    heif_tai_clock_info_alloc,
    heif_tai_clock_info_copy,
    heif_tai_clock_info_release,
    time_uncertainty,
    clock_resolution,
    clock_drift_rate,
    clock_type
);
versioned!(
    Timestamp,
    heif_tai_timestamp_packet_alloc,
    heif_tai_timestamp_packet_copy,
    heif_tai_timestamp_packet_release,
    tai_timestamp,
    synchronization_state,
    timestamp_generation_failure,
    timestamp_is_modified
);

macro_rules! item_property {
    ($ty:ty, $variant:ident, $kind:expr, $get:ident, $set:ident, $copy:ident, $missing_get:expr, $missing_set:expr, $duplicate:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(
            ctx: *const HeifContext,
            id: u32,
            out: *mut *mut $ty,
        ) -> HeifError {
            if ctx.is_null() || out.is_null() {
                return Error::NULL.into();
            }
            unsafe {
                out.write(ptr::null_mut());
            }
            let state = lock(&unsafe { &*ctx }.shared);
            if !state.items.items.contains_key(&id) {
                return Error::new(1, 2006, $missing_get).into();
            }
            if let Ok(p) = state.properties.find(id, 0, Some($kind))
                && let Some(TaiProperty::$variant(value)) = p.tai
            {
                unsafe {
                    out.write(Box::into_raw(Box::new(value)));
                }
            }
            SUCCESS
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(
            ctx: *mut HeifContext,
            id: u32,
            value: *const $ty,
            out: *mut u32,
        ) -> HeifError {
            if ctx.is_null() {
                return Error::NULL.into();
            }
            // Clock-info explicitly checks NULL before item existence; the
            // timestamp setter checks item existence before touching its input.
            if $kind == *b"taic" && value.is_null() {
                return Error::NULL.into();
            }
            let mut state = lock(&unsafe { &*ctx }.shared);
            if !state.items.items.contains_key(&id) {
                return Error::new(1, 2006, $missing_set).into();
            }
            if state
                .document
                .as_ref()
                .is_some_and(|d| d.images.contains_key(&id))
                && state.properties.find(id, 0, Some($kind)).is_ok()
            {
                return Error::new(5, 2006, $duplicate).into();
            }
            if value.is_null() {
                return Error::NULL.into();
            }
            let mut owned = <$ty>::default();
            unsafe {
                $copy(&mut owned, value);
            }
            match state
                .properties
                .add(id, TaiProperty::$variant(owned).property(), false)
            {
                Ok(id) => {
                    if !out.is_null() {
                        unsafe {
                            out.write(id);
                        }
                    }
                    SUCCESS
                }
                Err(e) => report(&mut state, e),
            }
        }
    };
}
item_property!(
    ClockInfo,
    Clock,
    *b"taic",
    heif_item_get_property_tai_clock_info,
    heif_item_set_property_tai_clock_info,
    heif_tai_clock_info_copy,
    c"item ID does not exist",
    c"itemId does not exist",
    c"item already has an taic property"
);
item_property!(
    Timestamp,
    Timestamp,
    *b"itai",
    heif_item_get_property_tai_timestamp,
    heif_item_set_property_tai_timestamp,
    heif_tai_timestamp_packet_copy,
    c"item does not exist",
    c"item does not exist",
    c"item already has an itai property"
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_set_tai_timestamp(
    image: *mut Image,
    timestamp: *const Timestamp,
) -> HeifError {
    if image.is_null() || timestamp.is_null() {
        return Error::NULL.into();
    }
    let mut value = Timestamp::default();
    unsafe {
        heif_tai_timestamp_packet_copy(&mut value, timestamp);
        (*image).tai_timestamp = Some(value);
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_get_tai_timestamp(
    image: *const Image,
    out: *mut *mut Timestamp,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    unsafe {
        out.write(ptr::null_mut());
    }
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    if let Some(value) = image.tai_timestamp {
        unsafe {
            out.write(Box::into_raw(Box::new(value)));
        }
    }
    SUCCESS
}
