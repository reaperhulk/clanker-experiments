// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, lock},
};
use libheifer::{error::Error, security::Limits};
use std::{cell::UnsafeCell, ptr};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SecurityLimits {
    pub version: u8,
    pub max_image_size_pixels: u64,
    pub max_number_of_tiles: u64,
    pub max_bayer_pattern_pixels: u32,
    pub max_items: u32,
    pub max_color_profile_size: u32,
    pub max_memory_block_size: u64,
    pub max_components: u32,
    pub max_iloc_extents_per_item: u32,
    pub max_size_entity_group: u32,
    pub max_children_per_box: u32,
    pub max_total_memory: u64,
    pub max_sample_description_box_entries: u32,
    pub max_sample_group_description_box_entries: u32,
    pub max_sequence_frames: u32,
    pub max_number_of_file_brands: u32,
    pub max_bad_pixels: u32,
    pub max_iso23001_17_pixel_size_bytes: u32,
    pub parent: *const SecurityLimits,
}
const GLOBAL: SecurityLimits = SecurityLimits {
    version: 4,
    max_image_size_pixels: 1_073_741_824,
    max_number_of_tiles: 16_777_216,
    max_bayer_pattern_pixels: 256,
    max_items: 1000,
    max_color_profile_size: 104_857_600,
    max_memory_block_size: 4_294_967_296,
    max_components: 256,
    max_iloc_extents_per_item: 32,
    max_size_entity_group: 64,
    max_children_per_box: 100,
    max_total_memory: 4_294_967_296,
    max_sample_description_box_entries: 1024,
    max_sample_group_description_box_entries: 1024,
    max_sequence_frames: 18_000_000,
    max_number_of_file_brands: 1000,
    max_bad_pixels: 1000,
    max_iso23001_17_pixel_size_bytes: 256,
    parent: ptr::null(),
};
struct StaticLimits(SecurityLimits);
// SAFETY: these process-lifetime objects are immutable and contain only a null pointer.
unsafe impl Sync for StaticLimits {}
static DEFAULT_LIMITS: StaticLimits = StaticLimits(GLOBAL);
static DISABLED_LIMITS: StaticLimits = StaticLimits(SecurityLimits {
    version: 4,
    max_image_size_pixels: 0,
    max_number_of_tiles: 0,
    max_bayer_pattern_pixels: 0,
    max_items: 0,
    max_color_profile_size: 0,
    max_memory_block_size: 0,
    max_components: 0,
    max_iloc_extents_per_item: 0,
    max_size_entity_group: 0,
    max_children_per_box: 0,
    max_total_memory: 0,
    max_sample_description_box_entries: 0,
    max_sample_group_description_box_entries: 0,
    max_sequence_frames: 0,
    max_number_of_file_brands: 0,
    max_bad_pixels: 0,
    max_iso23001_17_pixel_size_bytes: 0,
    parent: ptr::null(),
});
pub(super) struct ForeignLimits(UnsafeCell<SecurityLimits>);
// SAFETY: the public C API permits direct mutation of the limits. Its caller must
// synchronize such mutation with all operations on this context and its aliases.
// Immutable operations only copy scalar fields; the parent pointer is never dereferenced.
unsafe impl Send for ForeignLimits {}
unsafe impl Sync for ForeignLimits {}
impl Default for ForeignLimits {
    fn default() -> Self {
        Self(UnsafeCell::new(GLOBAL))
    }
}
impl ForeignLimits {
    pub fn pointer(&self) -> *mut SecurityLimits {
        self.0.get()
    }
    pub fn snapshot(&self) -> Limits {
        unsafe { snapshot(self.pointer()) }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_get_global_security_limits() -> *const SecurityLimits {
    &DEFAULT_LIMITS.0
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_get_disabled_security_limits() -> *const SecurityLimits {
    &DISABLED_LIMITS.0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_security_limits(
    context: *const HeifContext,
) -> *mut SecurityLimits {
    unsafe { context.as_ref() }.map_or(ptr::null_mut(), |c| c.shared.limits.pointer())
}
unsafe fn copy_fields(dst: *mut SecurityLimits, src: *const SecurityLimits) {
    unsafe {
        (*dst).max_image_size_pixels = (*src).max_image_size_pixels;
    }
    unsafe {
        (*dst).max_number_of_tiles = (*src).max_number_of_tiles;
    }
    unsafe {
        (*dst).max_bayer_pattern_pixels = (*src).max_bayer_pattern_pixels;
    }
    unsafe {
        (*dst).max_items = (*src).max_items;
    }
    unsafe {
        (*dst).max_color_profile_size = (*src).max_color_profile_size;
    }
    unsafe {
        (*dst).max_memory_block_size = (*src).max_memory_block_size;
    }
    unsafe {
        (*dst).max_components = (*src).max_components;
    }
    unsafe {
        (*dst).max_iloc_extents_per_item = (*src).max_iloc_extents_per_item;
    }
    unsafe {
        (*dst).max_size_entity_group = (*src).max_size_entity_group;
    }
    unsafe {
        (*dst).max_children_per_box = (*src).max_children_per_box;
    }
    if unsafe { (*src).version } >= 2 {
        unsafe {
            (*dst).max_total_memory = (*src).max_total_memory;
        }
        unsafe {
            (*dst).max_sample_description_box_entries = (*src).max_sample_description_box_entries;
        }
        unsafe {
            (*dst).max_sample_group_description_box_entries =
                (*src).max_sample_group_description_box_entries;
        }
    }
    if unsafe { (*src).version } >= 3 {
        unsafe {
            (*dst).max_sequence_frames = (*src).max_sequence_frames;
        }
        unsafe {
            (*dst).max_number_of_file_brands = (*src).max_number_of_file_brands;
        }
    }
    if unsafe { (*src).version } >= 4 {
        unsafe {
            (*dst).max_bad_pixels = (*src).max_bad_pixels;
        }
        unsafe {
            (*dst).max_iso23001_17_pixel_size_bytes = (*src).max_iso23001_17_pixel_size_bytes;
        }
    }
    unsafe {
        (*dst).parent = ptr::null();
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_security_limits(
    context: *mut HeifContext,
    src: *const SecurityLimits,
) -> HeifError {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return Error::NULL.into();
    };
    if src.is_null() {
        return Error::NULL.into();
    }
    let dst = context.shared.limits.pointer();
    if unsafe { (*src).version } < 4 {
        unsafe {
            copy_fields(dst, &GLOBAL);
        }
    }
    unsafe {
        copy_fields(dst, src);
    }
    drop(lock(&context.shared));
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_maximum_image_size_limit(
    context: *mut HeifContext,
    width: i32,
) {
    let Some(context) = (unsafe { context.as_ref() }) else {
        return;
    };
    unsafe {
        (*context.shared.limits.pointer()).max_image_size_pixels =
            (i64::from(width) * i64::from(width)) as u64;
    }
    drop(lock(&context.shared));
}

use libheifer::security::Budget;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
fn registry() -> &'static Mutex<BTreeMap<usize, Weak<Budget>>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<usize, Weak<Budget>>>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}
pub(super) fn register(context: &super::context::SharedContext) {
    let budget = lock(context).budget.clone();
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(context.limits.pointer() as usize, Arc::downgrade(&budget));
}
pub(super) fn unregister(context: &super::context::SharedContext) {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&(context.limits.pointer() as usize));
}
pub(super) unsafe fn allocation_budget(input: *const SecurityLimits) -> Option<Arc<Budget>> {
    if input.is_null() {
        return None;
    }
    let limits = unsafe { snapshot(input) };
    let mut root = input;
    let mut visited = std::collections::BTreeSet::new();
    while visited.insert(root as usize)
        && unsafe { (*root).version >= 4 && !(*root).parent.is_null() }
    {
        root = unsafe { (*root).parent };
    }
    let registered = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&(root as usize))
        .and_then(Weak::upgrade);
    Some(Arc::new(if let Some(registered) = registered {
        registered.with_limits(limits)
    } else {
        let mut limits = limits;
        // Unregistered/global limits enforce per-block limits only, as upstream.
        limits.max_total_memory = 0;
        Budget::new(Arc::new(std::sync::RwLock::new(limits)))
    }))
}

unsafe fn snapshot(p: *const SecurityLimits) -> Limits {
    Limits {
        version: unsafe { (*p).version },
        max_image_size_pixels: unsafe { (*p).max_image_size_pixels },
        max_number_of_tiles: unsafe { (*p).max_number_of_tiles },
        max_bayer_pattern_pixels: unsafe { (*p).max_bayer_pattern_pixels },
        max_items: unsafe { (*p).max_items },
        max_color_profile_size: unsafe { (*p).max_color_profile_size },
        max_memory_block_size: unsafe { (*p).max_memory_block_size },
        max_components: unsafe { (*p).max_components },
        max_iloc_extents_per_item: unsafe { (*p).max_iloc_extents_per_item },
        max_size_entity_group: unsafe { (*p).max_size_entity_group },
        max_children_per_box: unsafe { (*p).max_children_per_box },
        max_total_memory: unsafe { (*p).max_total_memory },
        max_sample_description_box_entries: unsafe { (*p).max_sample_description_box_entries },
        max_sample_group_description_box_entries: unsafe {
            (*p).max_sample_group_description_box_entries
        },
        max_sequence_frames: unsafe { (*p).max_sequence_frames },
        max_number_of_file_brands: unsafe { (*p).max_number_of_file_brands },
        max_bad_pixels: unsafe { (*p).max_bad_pixels },
        max_iso23001_17_pixel_size_bytes: unsafe { (*p).max_iso23001_17_pixel_size_bytes },
    }
}
