// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, lock, report},
};
use libheifer::{
    error::Error,
    items::{Item, Reference},
    properties::Property,
};
use std::{
    alloc::{Layout, alloc, dealloc},
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_number_of_items(ctx: *const HeifContext) -> c_int {
    unsafe { ctx.as_ref() }.map_or(0, |c| lock(&c.shared).items.items.len() as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_list_of_item_IDs(
    ctx: *const HeifContext,
    ids: *mut u32,
    count: c_int,
) -> c_int {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return 0;
    };
    if ids.is_null() {
        return 0;
    }
    let state = lock(&ctx.shared);
    let mut written = 0;
    for id in state.items.items.keys() {
        if written == count {
            break;
        }
        unsafe {
            ids.add(written as usize).write(*id);
        }
        written += 1;
    }
    written
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_item_type(ctx: *const HeifContext, id: u32) -> u32 {
    unsafe { ctx.as_ref() }
        .and_then(|c| lock(&c.shared).items.items.get(&id).map(|i| i.kind))
        .unwrap_or(0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_is_item_hidden(ctx: *const HeifContext, id: u32) -> c_int {
    c_int::from(unsafe { ctx.as_ref() }.is_none_or(|c| {
        lock(&c.shared)
            .items
            .items
            .get(&id)
            .is_none_or(|i| i.hidden)
    }))
}
macro_rules! string_query {
    ($name:ident, $field:ident, $kind:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(ctx: *const HeifContext, id: u32) -> *const c_char {
            let Some(ctx) = (unsafe { ctx.as_ref() }) else {
                return ptr::null();
            };
            let state = lock(&ctx.shared);
            state
                .items
                .items
                .get(&id)
                .filter(|i| $kind.is_none_or(|kind| i.kind == u32::from_be_bytes(kind)))
                .map_or(ptr::null(), |i| i.$field.as_ptr())
        }
    };
}
string_query!(heif_item_get_item_name, name, None::<[u8; 4]>);
string_query!(
    heif_item_get_mime_item_content_type,
    content_type,
    Some(*b"mime")
);
string_query!(
    heif_item_get_mime_item_content_encoding,
    content_encoding,
    Some(*b"mime")
);
string_query!(heif_item_get_uri_item_uri_type, uri_type, Some(*b"uri "));

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_set_item_name(
    ctx: *mut HeifContext,
    id: u32,
    name: *const c_char,
) -> HeifError {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&ctx.shared);
    let Some(item) = state.items.items.get_mut(&id) else {
        return Error::new(1, 2000, c"Item does not exist").into();
    };
    if name.is_null() {
        return Error::NULL.into();
    }
    item.name = unsafe { CStr::from_ptr(name) }.into();
    SUCCESS
}

// The private usize header stores the byte count. Public arrays only contain
// u8/u32 values, whose alignment is at most usize alignment on supported ABIs.
fn copy_array<T: Copy>(values: &[T]) -> Option<*mut T> {
    let bytes = std::mem::size_of_val(values);
    let size = bytes.checked_add(std::mem::size_of::<usize>())?;
    let layout = Layout::from_size_align(size, std::mem::align_of::<usize>()).ok()?;
    let allocation = unsafe { alloc(layout) };
    if allocation.is_null() {
        return None;
    }
    unsafe {
        allocation.cast::<usize>().write(bytes);
        let data = allocation.add(std::mem::size_of::<usize>()).cast::<T>();
        ptr::copy_nonoverlapping(values.as_ptr(), data, values.len());
        Some(data)
    }
}
unsafe fn release_array<T>(data: *mut *mut T) {
    if data.is_null() {
        return;
    }
    let values = unsafe { data.read() };
    if !values.is_null() {
        let allocation = unsafe { values.cast::<u8>().sub(std::mem::size_of::<usize>()) };
        let bytes = unsafe { allocation.cast::<usize>().read() };
        let layout = Layout::from_size_align(
            bytes + std::mem::size_of::<usize>(),
            std::mem::align_of::<usize>(),
        )
        .unwrap();
        unsafe {
            dealloc(allocation, layout);
        }
    }
    unsafe {
        data.write(ptr::null_mut());
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_item_data(
    ctx: *const HeifContext,
    id: u32,
    compression: *mut c_int,
    out: *mut *mut u8,
    size: *mut usize,
) -> HeifError {
    if !out.is_null() && size.is_null() {
        return Error::new(5, 2001, c"cannot return data with out_data_size==NULL").into();
    }
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&ctx.shared);
    let limits = *state
        .limits
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let method = state.items.items.get(&id).map_or(0, Item::compression);
    if state.items.has_iloc
        && state.items.items.contains_key(&id)
        && method == 0
        && !compression.is_null()
    {
        unsafe {
            compression.write(0);
        }
    }
    let result = state.items.item_data(id, limits).and_then(|data| {
        if compression.is_null() {
            libheifer::compression::decompress(data, method, &state.budget)
        } else {
            unsafe {
                compression.write(method);
            }
            Ok(data)
        }
    });
    match result {
        Err(error) => {
            unsafe {
                if !out.is_null() {
                    out.write(ptr::null_mut());
                }
                if !size.is_null() {
                    size.write(0);
                }
            }
            report(&mut state, error)
        }
        Ok(data) => {
            if !size.is_null() {
                unsafe {
                    size.write(data.len());
                }
            }
            if !out.is_null() {
                let Some(copy) = copy_array(&data) else {
                    return Error::ALLOCATION.into();
                };
                unsafe {
                    out.write(copy);
                }
            }
            SUCCESS
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_release_item_data(_ctx: *const HeifContext, data: *mut *mut u8) {
    unsafe {
        release_array(data);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_item_references(
    ctx: *const HeifContext,
    from: u32,
    index: c_int,
    kind: *mut u32,
    out: *mut *mut u32,
) -> usize {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return 0;
    };
    if index < 0 {
        return 0;
    }
    let state = lock(&ctx.shared);
    let Some(reference) = state
        .items
        .references
        .iter()
        .filter(|r| r.from == from)
        .nth(index as usize)
    else {
        return 0;
    };
    if !kind.is_null() {
        unsafe {
            kind.write(reference.kind);
        }
    }
    if !out.is_null() {
        let Some(copy) = copy_array(&reference.to) else {
            return 0;
        };
        unsafe {
            out.write(copy);
        }
    }
    reference.to.len()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_release_item_references(
    _ctx: *const HeifContext,
    refs: *mut *mut u32,
) {
    unsafe {
        release_array(refs);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_item_references(
    ctx: *mut HeifContext,
    kind: u32,
    from: u32,
    to: *const u32,
    count: c_int,
) -> HeifError {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    if count < 0 {
        return Error::new(5, 2006, c"Negative reference count").into();
    }
    if count > 0 && to.is_null() {
        return Error::NULL.into();
    }
    let to = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(to, count as usize) }
    };
    lock(&ctx.shared).items.add_reference(Reference {
        from,
        kind,
        to: to.to_vec(),
    });
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_item_reference(
    ctx: *mut HeifContext,
    kind: u32,
    from: u32,
    to: u32,
) -> HeifError {
    unsafe { heif_context_add_item_references(ctx, kind, from, &to, 1) }
}

unsafe fn add(
    ctx: *mut HeifContext,
    item: Item,
    compression: c_int,
    data: *const c_void,
    size: c_int,
    out: *mut u32,
) -> HeifError {
    if size < 0 {
        return Error::new(5, 2006, c"item data size must not be negative").into();
    }
    if size > 0 && data.is_null() {
        return Error::NULL.into();
    }
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    let data = if size == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) }
    };
    let mut state = lock(&ctx.shared);
    match state.items.add_compressed(item, data, compression) {
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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_item(
    ctx: *mut HeifContext,
    kind: *const c_char,
    data: *const c_void,
    size: c_int,
    out: *mut u32,
) -> HeifError {
    let kind = if kind.is_null() {
        &[][..]
    } else {
        unsafe { CStr::from_ptr(kind) }.to_bytes()
    };
    let Ok(kind) = <[u8; 4]>::try_from(kind) else {
        return Error::new(
            5,
            2006,
            c"called heif_context_add_item() with invalid 'item_type'.",
        )
        .into();
    };
    unsafe { add(ctx, Item::new(kind), 0, data, size, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_mime_item(
    ctx: *mut HeifContext,
    kind: *const c_char,
    compression: c_int,
    data: *const c_void,
    size: c_int,
    out: *mut u32,
) -> HeifError {
    if kind.is_null() {
        return Error::NULL.into();
    }
    let mut item = Item::new(*b"mime");
    item.content_type = unsafe { CStr::from_ptr(kind) }.into();
    unsafe { add(ctx, item, compression, data, size, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_precompressed_mime_item(
    ctx: *mut HeifContext,
    kind: *const c_char,
    encoding: *const c_char,
    data: *const c_void,
    size: c_int,
    out: *mut u32,
) -> HeifError {
    if kind.is_null() || encoding.is_null() {
        return Error::NULL.into();
    }
    let mut item = Item::new(*b"mime");
    item.content_type = unsafe { CStr::from_ptr(kind) }.into();
    item.content_encoding = unsafe { CStr::from_ptr(encoding) }.into();
    unsafe { add(ctx, item, 0, data, size, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_uri_item(
    ctx: *mut HeifContext,
    uri: *const c_char,
    data: *const c_void,
    size: c_int,
    out: *mut u32,
) -> HeifError {
    if uri.is_null() {
        return Error::NULL.into();
    }
    let mut item = Item::new(*b"uri ");
    item.uri_type = unsafe { CStr::from_ptr(uri) }.into();
    unsafe { add(ctx, item, 0, data, size, out) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_get_property_extended_language(
    ctx: *const HeifContext,
    id: u32,
    out: *mut *mut c_char,
) -> HeifError {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::new(5, 2006, c"NULL passed").into();
    };
    if out.is_null() {
        return Error::new(5, 2006, c"NULL passed").into();
    }
    let mut state = lock(&ctx.shared);
    match state.properties.find(id, 0, Some(*b"elng")) {
        Err(error) => report(&mut state, error),
        Ok(property) => {
            let bytes = property.data.get(4..).unwrap_or_default();
            let end = bytes
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(bytes.len().saturating_sub(1));
            unsafe {
                out.write(CString::new(&bytes[..end]).unwrap().into_raw());
            }
            SUCCESS
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_item_set_property_extended_language(
    ctx: *mut HeifContext,
    id: u32,
    language: *const c_char,
    out: *mut u32,
) -> HeifError {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::new(5, 2001, c"NULL passed").into();
    };
    if language.is_null() {
        return Error::new(5, 2001, c"NULL passed").into();
    }
    let mut state = lock(&ctx.shared);
    if state.properties.find(id, 0, Some(*b"elng")).is_ok() {
        return report(
            &mut state,
            libheifer::context::ContextError::new(
                5,
                0,
                "Usage error: Unspecified: Item already has an 'elng' language property.",
            ),
        );
    }
    let mut data = vec![0; 4];
    data.extend_from_slice(unsafe { CStr::from_ptr(language) }.to_bytes_with_nul());
    match state.properties.add(
        id,
        Property {
            kind: *b"elng",
            uuid: None,
            data,
            raw: false,
            tai: None,
            write_error: None,
            gimi_components: None,
        },
        false,
    ) {
        Ok(id) => {
            if !out.is_null() {
                unsafe {
                    out.write(id);
                }
            }
            SUCCESS
        }
        Err(error) => report(&mut state, error),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_string_release(value: *const c_char) {
    if !value.is_null() {
        unsafe {
            drop(CString::from_raw(value.cast_mut()));
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_metadata_compression_method_supported(method: c_int) -> c_int {
    c_int::from(matches!(method, 0 | 1 | 3 | 4))
}
