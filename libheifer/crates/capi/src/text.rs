// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, HeifHandle, SharedContext, lock, report},
};
use libheifer::{error::Error, text::TextItem};
use std::{
    ffi::{CStr, CString, c_char, c_int},
    sync::Arc,
};

pub struct HeifTextItem {
    shared: Arc<SharedContext>,
    value: Arc<TextItem>,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_text_item_release(text: *mut HeifTextItem) {
    if !text.is_null() {
        drop(unsafe { Box::from_raw(text) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_text_item_get_id(text: *mut HeifTextItem) -> u32 {
    unsafe { text.as_ref() }.map_or(0, |t| t.value.id)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_text_item_get_content(text: *mut HeifTextItem) -> *const c_char {
    let Some(text) = (unsafe { text.as_ref() }) else {
        return std::ptr::null();
    };
    let content = text
        .value
        .content
        .split(|b| *b == 0)
        .next()
        .unwrap_or_default();
    CString::new(content).unwrap().into_raw()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_text_item(
    ctx: *const HeifContext,
    id: u32,
    out: *mut *mut HeifTextItem,
) -> HeifError {
    if ctx.is_null() || out.is_null() {
        return Error::NULL.into();
    }
    let ctx = unsafe { &*ctx };
    let state = lock(&ctx.shared);
    let Some(value) = state.text_items.iter().find(|t| t.id == id) else {
        return Error::new(5, 2000, c"Text item does not exist").into();
    };
    unsafe {
        out.write(Box::into_raw(Box::new(HeifTextItem {
            shared: ctx.shared.clone(),
            value: value.clone(),
        })));
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_number_of_text_items(
    handle: *const HeifHandle,
) -> c_int {
    unsafe { handle.as_ref() }.map_or(0, |h| {
        h.image()
            .text_ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len() as c_int
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_list_of_text_item_ids(
    handle: *const HeifHandle,
    out: *mut u32,
    count: c_int,
) -> c_int {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    if out.is_null() || count <= 0 {
        return 0;
    }
    let ids = handle
        .image()
        .text_ids
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let n = ids.len().min(count as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(ids.as_ptr(), out, n);
    }
    n as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_add_text_item(
    handle: *mut HeifHandle,
    content_type: *const c_char,
    content: *const c_char,
    out: *mut *mut HeifTextItem,
) -> HeifError {
    if handle.is_null() || content_type.is_null() || content.is_null() {
        return Error::NULL.into();
    }
    let handle = unsafe { &*handle };
    let mut state = lock(&handle.shared);
    let content_type = unsafe { CStr::from_ptr(content_type) }.into();
    let content = unsafe { CStr::from_ptr(content) }.to_bytes().to_vec();
    let value = match state.add_text(content_type, content) {
        Ok(v) => v,
        Err(e) => return report(&mut state, e),
    };
    handle
        .image()
        .text_ids
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(value.id);
    if !out.is_null() {
        unsafe {
            out.write(Box::into_raw(Box::new(HeifTextItem {
                shared: handle.shared.clone(),
                value,
            })));
        }
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_text_item_get_property_extended_language(
    text: *const HeifTextItem,
    out: *mut *mut c_char,
) -> HeifError {
    if text.is_null() || out.is_null() {
        return Error::new(5, 2006, c"NULL passed").into();
    }
    let text = unsafe { &*text };
    let ctx = HeifContext {
        shared: text.shared.clone(),
    };
    unsafe { crate::items::heif_item_get_property_extended_language(&ctx, text.value.id, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_text_item_set_extended_language(
    text: *mut HeifTextItem,
    language: *const c_char,
    out: *mut u32,
) -> HeifError {
    if text.is_null() || language.is_null() {
        return Error::new(5, 2001, c"NULL passed").into();
    }
    let text = unsafe { &*text };
    let mut ctx = HeifContext {
        shared: text.shared.clone(),
    };
    unsafe {
        crate::items::heif_item_set_property_extended_language(
            &mut ctx,
            text.value.id,
            language,
            out,
        )
    }
}
