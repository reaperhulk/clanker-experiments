// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::context::{HeifContext, lock};
use std::ffi::c_int;
#[repr(C)]
pub struct EntityGroup {
    pub entity_group_id: u32,
    pub entity_group_type: u32,
    pub entities: *mut u32,
    pub num_entities: u32,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_entity_groups(
    ctx: *const HeifContext,
    type_filter: u32,
    item_filter: u32,
    out: *mut c_int,
) -> *mut EntityGroup {
    if ctx.is_null() || out.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { out.write(0) };
    let state = lock(&unsafe { &*ctx }.shared);
    let Some(groups) = &state.entity_groups else {
        return std::ptr::null_mut();
    };
    if groups.children == 0 {
        return std::ptr::null_mut();
    }
    let values: Box<[EntityGroup]> = groups
        .groups
        .iter()
        .filter(|g| {
            (type_filter == 0 || g.kind == type_filter)
                && (item_filter == 0 || g.entities.contains(&item_filter))
        })
        .map(|g| {
            let entities = if g.entities.is_empty() {
                std::ptr::null_mut()
            } else {
                Box::into_raw(g.entities.clone().into_boxed_slice()).cast::<u32>()
            };
            EntityGroup {
                entity_group_id: g.id,
                entity_group_type: g.kind,
                entities,
                num_entities: g.entities.len() as u32,
            }
        })
        .collect();
    unsafe { out.write(values.len() as c_int) };
    Box::into_raw(values).cast::<EntityGroup>()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_entity_groups_release(groups: *mut EntityGroup, count: c_int) {
    if groups.is_null() {
        return;
    }
    let values = unsafe {
        Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            groups,
            count.max(0) as usize,
        ))
    };
    for g in &values {
        if !g.entities.is_null() {
            drop(unsafe {
                Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                    g.entities,
                    g.num_entities as usize,
                ))
            });
        }
    }
}
