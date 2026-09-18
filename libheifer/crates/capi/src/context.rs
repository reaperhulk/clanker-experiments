// SPDX-License-Identifier: LGPL-3.0-or-later
use super::{HeifError, SUCCESS};
use libheifer::{
    context::{Context, ContextError, Document, ImageInfo, Input, Metadata},
    error::Error,
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr,
    sync::{Arc, Mutex, MutexGuard},
};

pub struct HeifContext {
    pub(super) shared: Arc<Mutex<Context>>,
}
pub struct HeifHandle {
    pub(super) shared: Arc<Mutex<Context>>,
    pub(super) document: Arc<Document>,
    pub(super) id: u32,
}
impl HeifHandle {
    fn image(&self) -> &ImageInfo {
        &self.document.images[&self.id]
    }
    fn metadata(&self, id: u32) -> Option<&Metadata> {
        self.image()
            .metadata
            .iter()
            .find(|m| m.id == id)
            .map(AsRef::as_ref)
    }
}
pub(super) fn lock(shared: &Mutex<Context>) -> MutexGuard<'_, Context> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
pub(super) fn report(context: &mut Context, error: ContextError) -> HeifError {
    context.last_error =
        CString::new(error.message).unwrap_or_else(|_| CString::new("Invalid error text").unwrap());
    HeifError {
        code: error.code,
        subcode: error.subcode,
        message: context.last_error.as_ptr(),
    }
}
fn invalid_id(shared: &Mutex<Context>) -> HeifError {
    report(
        &mut lock(shared),
        ContextError::new(5, 2000, "Usage error: Non-existing item ID referenced"),
    )
}
fn no_primary(context: &mut Context) -> HeifError {
    report(
        context,
        ContextError::invalid(124, "No or invalid primary item"),
    )
}
fn context_null(context: &mut Context) -> HeifError {
    report(
        context,
        ContextError::new(5, 2001, "Usage error: NULL argument received"),
    )
}

struct BorrowedInput {
    data: *const u8,
    len: usize,
}
// SAFETY: the C no-copy contract requires the buffer to remain alive and
// immutable until all contexts/handles that refer to it have been released.
// The adapter never writes the buffer. Sharing only creates immutable slices.
unsafe impl Send for BorrowedInput {}
unsafe impl Sync for BorrowedInput {}
impl Input for BorrowedInput {
    fn bytes(&self) -> &[u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.data, self.len) }
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_context_alloc() -> *mut HeifContext {
    super::color::allocate(HeifContext {
        shared: Arc::new(Mutex::new(Context::default())),
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_free(context: *mut HeifContext) {
    if !context.is_null() {
        unsafe { drop(Box::from_raw(context)) };
    }
}
unsafe fn read_memory(
    context: *mut HeifContext,
    data: *const c_void,
    size: usize,
    copy: bool,
) -> HeifError {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    if data.is_null() && size != 0 {
        return Error::NULL.into();
    }
    if size > isize::MAX as usize {
        return Error::ALLOCATION.into();
    }
    let input: Arc<dyn Input> = if copy {
        let slice = if size == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size) }
        };
        let mut owned = Vec::new();
        if owned.try_reserve_exact(size).is_err() {
            return Error::ALLOCATION.into();
        }
        owned.extend_from_slice(slice);
        Arc::new(owned)
    } else {
        Arc::new(BorrowedInput {
            data: data.cast(),
            len: size,
        })
    };
    let mut state = lock(&context.shared);
    match state.read(input) {
        Ok(()) => SUCCESS,
        Err(e) => report(&mut state, e),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_read_from_memory(
    context: *mut HeifContext,
    data: *const c_void,
    size: usize,
    _options: *const c_void,
) -> HeifError {
    unsafe { read_memory(context, data, size, true) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_read_from_memory_without_copy(
    context: *mut HeifContext,
    data: *const c_void,
    size: usize,
    _options: *const c_void,
) -> HeifError {
    unsafe { read_memory(context, data, size, false) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_number_of_top_level_images(
    context: *const HeifContext,
) -> c_int {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return 0;
    };
    lock(&context.shared)
        .document
        .as_ref()
        .map_or(0, |d| d.top_level.len() as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_is_top_level_image_ID(
    context: *const HeifContext,
    id: u32,
) -> c_int {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return 0;
    };
    lock(&context.shared)
        .document
        .as_ref()
        .is_some_and(|d| d.top_level.contains(&id))
        .into()
}
unsafe fn ids(values: &[u32], out: *mut u32, count: c_int) -> c_int {
    if out.is_null() || count == 0 {
        return 0;
    }
    let n = count.min(values.len() as c_int);
    if n > 0 {
        unsafe { ptr::copy_nonoverlapping(values.as_ptr(), out, n as usize) };
    }
    n
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_list_of_top_level_image_IDs(
    context: *const HeifContext,
    out: *mut u32,
    count: c_int,
) -> c_int {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return 0;
    };
    let state = lock(&context.shared);
    unsafe {
        ids(
            state
                .document
                .as_ref()
                .map_or(&[], |d| d.top_level.as_slice()),
            out,
            count,
        )
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_primary_image_ID(
    context: *const HeifContext,
    out: *mut u32,
) -> HeifError {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&context.shared);
    if out.is_null() {
        return context_null(&mut state);
    }
    let Some(doc) = state
        .document
        .as_ref()
        .filter(|d| d.images.get(&d.primary).is_some_and(|i| i.primary))
    else {
        return no_primary(&mut state);
    };
    unsafe { out.write(doc.primary) };
    SUCCESS
}
unsafe fn create_handle(
    shared: &Arc<Mutex<Context>>,
    doc: Arc<Document>,
    id: u32,
    out: *mut *mut HeifHandle,
) -> HeifError {
    if let Some(e) = &doc.images[&id].error {
        return report(&mut lock(shared), e.clone());
    }
    let handle = super::color::allocate(HeifHandle {
        shared: shared.clone(),
        document: doc,
        id,
    });
    if handle.is_null() {
        return Error::ALLOCATION.into();
    }
    unsafe { out.write(handle) };
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_primary_image_handle(
    context: *const HeifContext,
    out: *mut *mut HeifHandle,
) -> HeifError {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&context.shared);
    if out.is_null() {
        return context_null(&mut state);
    }
    let Some(doc) = state
        .document
        .clone()
        .filter(|d| d.images.get(&d.primary).is_some_and(|i| i.primary))
    else {
        return no_primary(&mut state);
    };
    let id = doc.primary;
    drop(state);
    unsafe { create_handle(&context.shared, doc, id, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_image_handle(
    context: *const HeifContext,
    id: u32,
    out: *mut *mut HeifHandle,
) -> HeifError {
    if out.is_null() {
        return Error::new(5, 2001, c"").into();
    }
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    let doc = lock(&context.shared).document.clone();
    if let Some(doc) = doc.filter(|d| d.images.contains_key(&id)) {
        unsafe { create_handle(&context.shared, doc, id, out) }
    } else {
        unsafe { out.write(ptr::null_mut()) };
        Error::new(5, 2000, c"").into()
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_release(handle: *const HeifHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle.cast_mut())) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_context(
    handle: *const HeifHandle,
) -> *mut HeifContext {
    unsafe { handle.as_ref() }.map_or(ptr::null_mut(), |h| {
        super::color::allocate(HeifContext {
            shared: h.shared.clone(),
        })
    })
}
fn dimension(n: u32) -> c_int {
    c_int::try_from(n).unwrap_or(0)
}
macro_rules! query {
    ($name:ident,$default:expr,$body:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(handle: *const HeifHandle) -> c_int {
            unsafe { handle.as_ref() }.map_or($default, |h| ($body)(h.image()))
        }
    };
}
query!(heif_image_handle_is_primary_image, 0, |i: &ImageInfo| {
    i32::from(i.primary)
});
query!(heif_image_handle_get_width, 0, |i: &ImageInfo| dimension(
    i.width
));
query!(heif_image_handle_get_height, 0, |i: &ImageInfo| dimension(
    i.height
));
query!(
    heif_image_handle_get_ispe_width,
    0,
    |i: &ImageInfo| i.ispe.0 as c_int
);
query!(
    heif_image_handle_get_ispe_height,
    0,
    |i: &ImageInfo| i.ispe.1 as c_int
);
query!(
    heif_image_handle_get_luma_bits_per_pixel,
    -1,
    |i: &ImageInfo| i.luma_bits
);
query!(
    heif_image_handle_get_chroma_bits_per_pixel,
    -1,
    |i: &ImageInfo| i.chroma_bits
);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_has_alpha_channel(handle: *const HeifHandle) -> c_int {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    // Upstream resolves this query through the context, unlike the other
    // description queries. A reload can therefore change an old handle's answer.
    lock(&handle.shared)
        .document
        .as_ref()
        .and_then(|d| d.images.get(&handle.id))
        .is_some_and(|i| i.has_alpha)
        .into()
}
query!(
    heif_image_handle_is_premultiplied_alpha,
    0,
    |i: &ImageInfo| i32::from(i.premultiplied_alpha)
);
query!(
    heif_image_handle_get_number_of_thumbnails,
    0,
    |i: &ImageInfo| i.thumbnails.len() as c_int
);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_item_id(handle: *const HeifHandle) -> u32 {
    unsafe { handle.as_ref() }.map_or(0, |h| h.id)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_preferred_decoding_colorspace(
    handle: *const HeifHandle,
    colorspace: *mut c_int,
    chroma: *mut c_int,
) -> HeifError {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    let image = handle.image();
    let (preferred_colorspace, preferred_chroma) =
        if image.colorspace == 0 && image.color.nclx.is_some_and(|p| p.matrix == 0) {
            (1, 3)
        } else {
            (image.colorspace, image.chroma)
        };
    if !colorspace.is_null() {
        unsafe { colorspace.write(preferred_colorspace) };
    }
    if !chroma.is_null() {
        unsafe { chroma.write(preferred_chroma) };
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_pixel_aspect_ratio(
    handle: *const HeifHandle,
    h: *mut u32,
    v: *mut u32,
) -> c_int {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    let ratio = handle.image().pixel_aspect;
    if !h.is_null() {
        unsafe { h.write(ratio.unwrap_or((1, 1)).0) };
    }
    if !v.is_null() {
        unsafe { v.write(ratio.unwrap_or((1, 1)).1) };
    }
    ratio.is_some().into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_list_of_thumbnail_IDs(
    handle: *const HeifHandle,
    out: *mut u32,
    count: c_int,
) -> c_int {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    unsafe { ids(&handle.image().thumbnails, out, count) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_thumbnail(
    handle: *const HeifHandle,
    id: u32,
    out: *mut *mut HeifHandle,
) -> HeifError {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    if out.is_null() {
        return Error::NULL.into();
    }
    if !handle.image().thumbnails.contains(&id) {
        return invalid_id(&handle.shared);
    }
    if handle.document.images[&id].error.is_some() {
        unsafe { out.write(ptr::null_mut()) };
    }
    unsafe { create_handle(&handle.shared, handle.document.clone(), id, out) }
}
unsafe fn filter<'a>(value: *const c_char) -> Option<&'a CStr> {
    if value.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(value) })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_number_of_metadata_blocks(
    handle: *const HeifHandle,
    kind: *const c_char,
) -> c_int {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    let kind = unsafe { filter(kind) };
    handle
        .image()
        .metadata
        .iter()
        .filter(|m| kind.is_none_or(|k| k == m.kind.as_c_str()))
        .count() as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_list_of_metadata_block_IDs(
    handle: *const HeifHandle,
    kind: *const c_char,
    out: *mut u32,
    count: c_int,
) -> c_int {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    if out.is_null() || count <= 0 {
        return 0;
    }
    let kind = unsafe { filter(kind) };
    let mut n = 0;
    for m in handle
        .image()
        .metadata
        .iter()
        .filter(|m| kind.is_none_or(|k| k == m.kind.as_c_str()))
        .take(count as usize)
    {
        unsafe { out.add(n).write(m.id) };
        n += 1;
    }
    n as c_int
}
macro_rules! metadata_string {
    ($name:ident,$field:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(handle: *const HeifHandle, id: u32) -> *const c_char {
            unsafe { handle.as_ref() }
                .and_then(|h| h.metadata(id))
                .map_or(ptr::null(), |m| m.$field.as_ptr())
        }
    };
}
metadata_string!(heif_image_handle_get_metadata_type, kind);
metadata_string!(heif_image_handle_get_metadata_content_type, content_type);
metadata_string!(heif_image_handle_get_metadata_item_uri_type, uri_type);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_metadata_size(
    handle: *const HeifHandle,
    id: u32,
) -> usize {
    unsafe { handle.as_ref() }
        .and_then(|h| h.metadata(id))
        .map_or(0, |m| m.data.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_metadata(
    handle: *const HeifHandle,
    id: u32,
    out: *mut c_void,
) -> HeifError {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(m) = handle.metadata(id) else {
        return invalid_id(&handle.shared);
    };
    if !m.data.is_empty() {
        if out.is_null() {
            return Error::NULL.into();
        }
        unsafe { ptr::copy_nonoverlapping(m.data.as_ptr(), out.cast(), m.data.len()) };
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_color_profile_type(
    handle: *const HeifHandle,
) -> u32 {
    unsafe { handle.as_ref() }.map_or(0, |h| h.image().color.profile_type())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_raw_color_profile_size(
    handle: *const HeifHandle,
) -> usize {
    unsafe { handle.as_ref() }
        .and_then(|h| h.image().color.raw.as_ref())
        .map_or(0, |p| p.data.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_raw_color_profile(
    handle: *const HeifHandle,
    out: *mut c_void,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(raw) = &handle.image().color.raw else {
        return Error::new(10, 0, c"Color profile does not exist: Unspecified").into();
    };
    unsafe { ptr::copy_nonoverlapping(raw.data.as_ptr(), out.cast(), raw.data.len()) };
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_nclx_color_profile(
    handle: *const HeifHandle,
    out: *mut *mut libheifer::color::NclxProfile,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(p) = handle.image().color.nclx.filter(|p| p.is_defined()) else {
        return Error::new(10, 0, c"Color profile does not exist: Unspecified").into();
    };
    unsafe { out.write(ptr::null_mut()) };
    match p.decode() {
        Ok(p) => {
            let p = super::color::allocate(p);
            if p.is_null() {
                return Error::ALLOCATION.into();
            }
            unsafe { out.write(p) };
            SUCCESS
        }
        Err(e) => e.into(),
    }
}
