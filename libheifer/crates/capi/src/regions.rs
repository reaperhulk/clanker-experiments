// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, HeifHandle, SharedContext, lock, report},
};
use libheifer::{
    error::Error,
    image::Image,
    regions::{Geometry, RegionItem, Transform},
};
use std::{
    ffi::c_int,
    sync::{Arc, Mutex, MutexGuard},
};
pub struct HeifRegionItem {
    shared: Arc<SharedContext>,
    value: Arc<Mutex<RegionItem>>,
}
pub struct HeifRegion {
    shared: Arc<SharedContext>,
    item: Arc<Mutex<RegionItem>>,
    geometry: Arc<Geometry>,
}
fn item_lock(item: &Mutex<RegionItem>) -> MutexGuard<'_, RegionItem> {
    item.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn invalid() -> HeifError {
    Error::new(5, 2006, c"Invalid parameter value").into()
}
unsafe fn append(
    item: *mut HeifRegionItem,
    geometry: Geometry,
    out: *mut *mut HeifRegion,
) -> HeifError {
    let Some(item) = (unsafe { item.as_ref() }) else {
        return Error::NULL.into();
    };
    let geometry = Arc::new(geometry);
    item_lock(&item.value).regions.push(geometry.clone());
    if !out.is_null() {
        unsafe {
            out.write(Box::into_raw(Box::new(HeifRegion {
                shared: item.shared.clone(),
                item: item.value.clone(),
                geometry,
            })));
        }
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_number_of_region_items(
    handle: *const HeifHandle,
) -> c_int {
    unsafe { handle.as_ref() }.map_or(0, |h| {
        h.image()
            .region_ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len() as c_int
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_get_list_of_region_item_ids(
    handle: *const HeifHandle,
    out: *mut u32,
    count: c_int,
) -> c_int {
    let Some(h) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    if out.is_null() || count <= 0 {
        return 0;
    }
    let ids = h
        .image()
        .region_ids
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let n = ids.len().min(count as usize);
    unsafe { std::ptr::copy_nonoverlapping(ids.as_ptr(), out, n) };
    n as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_region_item(
    ctx: *const HeifContext,
    id: u32,
    out: *mut *mut HeifRegionItem,
) -> HeifError {
    if ctx.is_null() || out.is_null() {
        return Error::NULL.into();
    }
    let ctx = unsafe { &*ctx };
    let state = lock(&ctx.shared);
    let Some(value) = state.region_items.iter().find(|v| item_lock(v).id == id) else {
        return Error::new(5, 2000, c"Region item does not exist").into();
    };
    unsafe {
        out.write(Box::into_raw(Box::new(HeifRegionItem {
            shared: ctx.shared.clone(),
            value: value.clone(),
        })))
    };
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_image_handle_add_region_item(
    h: *mut HeifHandle,
    w: u32,
    height: u32,
    out: *mut *mut HeifRegionItem,
) -> HeifError {
    let Some(h) = (unsafe { h.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut state = lock(&h.shared);
    let value = match state.add_region(w, height) {
        Ok(v) => v,
        Err(e) => return report(&mut state, e),
    };
    h.image()
        .region_ids
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(item_lock(&value).id);
    if !out.is_null() {
        unsafe {
            out.write(Box::into_raw(Box::new(HeifRegionItem {
                shared: h.shared.clone(),
                value,
            })))
        };
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_get_id(item: *const HeifRegionItem) -> u32 {
    unsafe { item.as_ref() }.map_or(0, |i| item_lock(&i.value).id)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_release(item: *mut HeifRegionItem) {
    if !item.is_null() {
        drop(unsafe { Box::from_raw(item) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_release(region: *const HeifRegion) {
    if !region.is_null() {
        drop(unsafe { Box::from_raw(region.cast_mut()) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_release_many(regions: *const *const HeifRegion, count: c_int) {
    if !regions.is_null() {
        for i in 0..count.max(0) as usize {
            unsafe { heif_region_release(*regions.add(i)) };
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_get_reference_size(
    item: *const HeifRegionItem,
    w: *mut u32,
    h: *mut u32,
) {
    let Some(item) = (unsafe { item.as_ref() }) else {
        return;
    };
    let state = lock(&item.shared);
    let id = item_lock(&item.value).id;
    let Some(value) = state.region_items.iter().find(|v| item_lock(v).id == id) else {
        return;
    };
    let v = item_lock(value);
    if !w.is_null() {
        unsafe { w.write(v.width) }
    }
    if !h.is_null() {
        unsafe { h.write(v.height) }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_get_number_of_regions(
    item: *const HeifRegionItem,
) -> c_int {
    unsafe { item.as_ref() }.map_or(0, |i| item_lock(&i.value).regions.len() as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_get_list_of_regions(
    item: *const HeifRegionItem,
    out: *mut *mut HeifRegion,
    count: c_int,
) -> c_int {
    let Some(item) = (unsafe { item.as_ref() }) else {
        return 0;
    };
    let v = item_lock(&item.value);
    let n = count.min(v.regions.len() as c_int);
    if !out.is_null() {
        for i in 0..n.max(0) as usize {
            unsafe {
                out.add(i).write(Box::into_raw(Box::new(HeifRegion {
                    shared: item.shared.clone(),
                    item: item.value.clone(),
                    geometry: v.regions[i].clone(),
                })))
            }
        }
    }
    n
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_get_type(r: *const HeifRegion) -> c_int {
    unsafe { r.as_ref() }.map_or(-1, |r| r.geometry.kind)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_add_region_point(
    item: *mut HeifRegionItem,
    x: i32,
    y: i32,
    out: *mut *mut HeifRegion,
) -> HeifError {
    unsafe { append(item, Geometry::new(0, x, y, 0, 0), out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_get_point(
    r: *const HeifRegion,
    x: *mut i32,
    y: *mut i32,
) -> HeifError {
    if x.is_null() || y.is_null() || r.is_null() {
        return Error::NULL.into();
    }
    let r = unsafe { &*r };
    if r.geometry.kind != 0 {
        return invalid();
    }
    unsafe {
        x.write(r.geometry.x);
        y.write(r.geometry.y)
    };
    SUCCESS
}
fn transform(r: &HeifRegion, id: u32) -> Result<Transform, HeifError> {
    let mut state = lock(&r.shared);
    let item = item_lock(&r.item);
    Transform::create(&state.properties, id, (item.width, item.height))
        .map_err(|e| report(&mut state, e))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_get_point_transformed(
    r: *const HeifRegion,
    id: u32,
    x: *mut f64,
    y: *mut f64,
) -> HeifError {
    if x.is_null() || y.is_null() || r.is_null() {
        return Error::NULL.into();
    }
    let r = unsafe { &*r };
    if r.geometry.kind != 0 {
        return invalid();
    }
    let t = match transform(r, id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let (a, b) = t.point(r.geometry.x, r.geometry.y);
    unsafe {
        x.write(a);
        y.write(b)
    };
    SUCCESS
}
macro_rules! extent_api {
    ($add:ident,$get:ident,$transformed:ident,$kind:literal) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $add(
            item: *mut HeifRegionItem,
            x: i32,
            y: i32,
            w: u32,
            h: u32,
            out: *mut *mut HeifRegion,
        ) -> HeifError {
            unsafe { append(item, Geometry::new($kind, x, y, w, h), out) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(
            r: *const HeifRegion,
            x: *mut i32,
            y: *mut i32,
            w: *mut u32,
            h: *mut u32,
        ) -> HeifError {
            let Some(r) = (unsafe { r.as_ref() }) else {
                return Error::NULL.into();
            };
            let g = &r.geometry;
            if g.kind != $kind {
                return invalid();
            }
            if x.is_null() || y.is_null() || w.is_null() || h.is_null() {
                return Error::NULL.into();
            }
            unsafe {
                x.write(g.x);
                y.write(g.y);
                w.write(g.width);
                h.write(g.height)
            };
            SUCCESS
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $transformed(
            r: *const HeifRegion,
            id: u32,
            x: *mut f64,
            y: *mut f64,
            w: *mut f64,
            h: *mut f64,
        ) -> HeifError {
            let Some(r) = (unsafe { r.as_ref() }) else {
                return Error::NULL.into();
            };
            let g = &r.geometry;
            if g.kind != $kind {
                return invalid();
            }
            if x.is_null() || y.is_null() || w.is_null() || h.is_null() {
                return Error::NULL.into();
            }
            let t = match transform(r, id) {
                Ok(t) => t,
                Err(e) => return e,
            };
            let (a, b) = t.point(g.x, g.y);
            let (c, d) = t.extent(g.width, g.height);
            unsafe {
                x.write(a);
                y.write(b);
                w.write(c);
                h.write(d)
            };
            SUCCESS
        }
    };
}
extent_api!(
    heif_region_item_add_region_rectangle,
    heif_region_get_rectangle,
    heif_region_get_rectangle_transformed,
    1
);
extent_api!(
    heif_region_item_add_region_ellipse,
    heif_region_get_ellipse,
    heif_region_get_ellipse_transformed,
    2
);
macro_rules! poly_api {
    ($add:ident,$num:ident,$get:ident,$transformed:ident,$kind:literal) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $add(
            item: *mut HeifRegionItem,
            pts: *const i32,
            count: c_int,
            out: *mut *mut HeifRegion,
        ) -> HeifError {
            if !out.is_null() {
                unsafe { out.write(std::ptr::null_mut()) }
            }
            if count < 0 {
                return Error::new(5, 2006, c"Number of polygon points must not be negative")
                    .into();
            }
            if count > 0 && pts.is_null() {
                return Error::NULL.into();
            }
            let mut g = Geometry::new($kind, 0, 0, 0, 0);
            if count > 0 {
                g.points = unsafe { std::slice::from_raw_parts(pts, count as usize * 2) }.to_vec();
            }
            unsafe { append(item, g, out) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $num(r: *const HeifRegion) -> c_int {
            unsafe { r.as_ref() }.map_or(0, |r| {
                if matches!(r.geometry.kind, 3 | 6) {
                    (r.geometry.points.len() / 2) as c_int
                } else {
                    0
                }
            })
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(r: *const HeifRegion, out: *mut i32) -> HeifError {
            if out.is_null() {
                return invalid();
            }
            let Some(r) = (unsafe { r.as_ref() }) else {
                return Error::NULL.into();
            };
            if !matches!(r.geometry.kind, 3 | 6) {
                return invalid();
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    r.geometry.points.as_ptr(),
                    out,
                    r.geometry.points.len(),
                )
            };
            SUCCESS
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $transformed(
            r: *const HeifRegion,
            id: u32,
            out: *mut f64,
        ) -> HeifError {
            if out.is_null() {
                return invalid();
            }
            let Some(r) = (unsafe { r.as_ref() }) else {
                return Error::NULL.into();
            };
            if !matches!(r.geometry.kind, 3 | 6) {
                return invalid();
            }
            let t = match transform(r, id) {
                Ok(t) => t,
                Err(e) => return e,
            };
            for (i, p) in r.geometry.points.chunks_exact(2).enumerate() {
                let (x, y) = t.point(p[0], p[1]);
                unsafe {
                    out.add(i * 2).write(x);
                    out.add(i * 2 + 1).write(y)
                }
            }
            SUCCESS
        }
    };
}
poly_api!(
    heif_region_item_add_region_polygon,
    heif_region_get_polygon_num_points,
    heif_region_get_polygon_points,
    heif_region_get_polygon_points_transformed,
    3
);
poly_api!(
    heif_region_item_add_region_polyline,
    heif_region_get_polyline_num_points,
    heif_region_get_polyline_points,
    heif_region_get_polyline_points_transformed,
    6
);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_add_region_referenced_mask(
    item: *mut HeifRegionItem,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    id: u32,
    out: *mut *mut HeifRegion,
) -> HeifError {
    let mut g = Geometry::new(4, x, y, w, h);
    g.referenced = id;
    let e = unsafe { append(item, g, out) };
    if e.code == 0 {
        let item = unsafe { &*item };
        let mut state = lock(&item.shared);
        state.items.add_reference(libheifer::items::Reference {
            from: item_lock(&item.value).id,
            kind: u32::from_be_bytes(*b"mask"),
            to: vec![id],
        });
    }
    e
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_get_referenced_mask_ID(
    r: *const HeifRegion,
    x: *mut i32,
    y: *mut i32,
    w: *mut u32,
    h: *mut u32,
    id: *mut u32,
) -> HeifError {
    if r.is_null() || x.is_null() || y.is_null() || w.is_null() || h.is_null() || id.is_null() {
        return Error::NULL.into();
    }
    let g = &unsafe { &*r }.geometry;
    if g.kind != 4 {
        return invalid();
    }
    unsafe {
        x.write(g.x);
        y.write(g.y);
        w.write(g.width);
        h.write(g.height);
        id.write(g.referenced)
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_add_region_inline_mask_data(
    item: *mut HeifRegionItem,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    data: *const u8,
    len: usize,
    out: *mut *mut HeifRegion,
) -> HeifError {
    if !out.is_null() {
        unsafe { out.write(std::ptr::null_mut()) }
    }
    if data.is_null() {
        return Error::NULL.into();
    }
    if w == 0 || h == 0 {
        return Error::new(2, 136, c"Inline mask region has zero width or height").into();
    }
    let size = (u64::from(w) * u64::from(h)).div_ceil(8);
    if size > usize::MAX as u64 {
        return Error::new(6, 1000, c"Inline mask size overflow").into();
    }
    if len as u64 != size {
        return Error::new(
            2,
            136,
            c"Inline mask data length does not match the given region size",
        )
        .into();
    }
    let mut g = Geometry::new(5, x, y, w, h);
    g.mask = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
    unsafe { append(item, g, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_get_inline_mask_data_len(r: *const HeifRegion) -> usize {
    unsafe { r.as_ref() }.map_or(0, |r| r.geometry.mask.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_get_inline_mask_data(
    r: *const HeifRegion,
    x: *mut i32,
    y: *mut i32,
    w: *mut u32,
    h: *mut u32,
    data: *mut u8,
) -> HeifError {
    if r.is_null() || x.is_null() || y.is_null() || w.is_null() || h.is_null() {
        return Error::NULL.into();
    }
    let g = &unsafe { &*r }.geometry;
    if g.kind != 5 {
        return invalid();
    }
    unsafe {
        x.write(g.x);
        y.write(g.y);
        w.write(g.width);
        h.write(g.height);
        if !data.is_null() {
            std::ptr::copy_nonoverlapping(g.mask.as_ptr(), data, g.mask.len())
        }
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_item_add_region_inline_mask(
    item: *mut HeifRegionItem,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    image: *const Image,
    out: *mut *mut HeifRegion,
) -> HeifError {
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Error::NULL.into();
    };
    let Some(p) = image.plane(0) else {
        return Error::new(5, 2002, c"Inline mask image must have a Y channel").into();
    };
    let size = (u64::from(w) * u64::from(h)).div_ceil(8);
    if size > usize::MAX as u64 {
        return Error::new(6, 1000, c"Inline mask size overflow").into();
    }
    let mut g = Geometry::new(5, x, y, w, h);
    g.mask = vec![0; size as usize];
    for row in 0..h.min(image.height) {
        for col in 0..w.min(image.width) {
            let bit = u64::from(row) * u64::from(w) + u64::from(col);
            g.mask[bit as usize / 8] |=
                (p.data()[row as usize * p.stride + col as usize] & 0x80) >> (bit % 8);
        }
    }
    unsafe { append(item, g, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_region_get_mask_image(
    r: *const HeifRegion,
    x: *mut i32,
    y: *mut i32,
    w: *mut u32,
    h: *mut u32,
    out: *mut *mut Image,
) -> HeifError {
    let Some(r) = (unsafe { r.as_ref() }) else {
        return Error::NULL.into();
    };
    let g = &r.geometry;
    if !matches!(g.kind, 4 | 5) {
        return invalid();
    }
    if x.is_null() || y.is_null() || w.is_null() || h.is_null() {
        return Error::NULL.into();
    }
    unsafe {
        x.write(g.x);
        y.write(g.y);
        w.write(g.width);
        h.write(g.height)
    };
    if g.kind == 4 {
        let ctx = HeifContext {
            shared: r.shared.clone(),
        };
        let mut handle = std::ptr::null_mut();
        let e = unsafe {
            crate::context::heif_context_get_image_handle(&ctx, g.referenced, &mut handle)
        };
        if e.code != 0 {
            return e;
        }
        let e = unsafe { crate::decoding::heif_decode_image(handle, out, 2, 0, std::ptr::null()) };
        unsafe { crate::context::heif_image_handle_release(handle) };
        return e;
    }
    let e =
        unsafe { crate::image::heif_image_create(g.width as c_int, g.height as c_int, 2, 0, out) };
    if e.code != 0 {
        return e;
    }
    let image = unsafe { &mut **out };
    if let Err(e) = image.add_plane(0, g.width, g.height, 8) {
        unsafe { crate::image::heif_image_release(*out) };
        return e.into();
    }
    let p = image.plane_mut(0).unwrap();
    let stride = p.stride;
    for row in 0..g.height as usize {
        for col in 0..g.width as usize {
            let bit = row * g.width as usize + col;
            p.data_mut()[row * stride + col] = if g
                .mask
                .get(bit / 8)
                .is_some_and(|v| v & (0x80 >> (bit % 8)) != 0)
            {
                255
            } else {
                0
            };
        }
    }
    SUCCESS
}
