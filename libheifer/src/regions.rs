// SPDX-License-Identifier: LGPL-3.0-or-later
// Region semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Retained region objects, bounded geometry parsing, and property transforms.
use crate::{
    context::{Context, ContextError},
    items::Item,
    properties::PropertyStore,
    security::{Budget, Limits, Reservation},
};
use std::sync::{Arc, Mutex};
pub struct Geometry {
    pub kind: i32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub points: Vec<i32>,
    pub mask: Vec<u8>,
    pub referenced: u32,
    pub reservation: Option<Reservation>,
}
impl Geometry {
    pub fn new(kind: i32, x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            kind,
            x,
            y,
            width,
            height,
            points: Vec::new(),
            mask: Vec::new(),
            referenced: 0,
            reservation: None,
        }
    }
}
pub struct RegionItem {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub regions: Vec<Arc<Geometry>>,
}
impl Context {
    pub fn add_region(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<Arc<Mutex<RegionItem>>, ContextError> {
        self.items.layout.lock().unwrap().init_image();
        self.items.has_iloc = true;
        self.properties.has_ipco = true;
        self.properties.has_ipma = true;
        let id = self.items.mint()?;
        self.items.items.insert(id, Item::new(*b"rgan"));
        let item = Arc::new(Mutex::new(RegionItem {
            id,
            width,
            height,
            regions: Vec::new(),
        }));
        self.region_items.push(item.clone());
        Ok(item)
    }
}
struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
    size: usize,
}
impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let end = self.offset.checked_add(n)?;
        let out = self.data.get(self.offset..end)?;
        self.offset = end;
        Some(out)
    }
    fn unsigned(&mut self) -> Option<u32> {
        let size = self.size;
        let b = self.bytes(size)?;
        Some(if size == 2 {
            u16::from_be_bytes(b.try_into().ok()?) as u32
        } else {
            u32::from_be_bytes(b.try_into().ok()?)
        })
    }
    fn signed(&mut self) -> Option<i32> {
        let value = self.unsigned()?;
        Some(if self.size == 2 {
            value as i16 as i32
        } else {
            value as i32
        })
    }
    fn geometry(&mut self, kind: i32, limits: Limits, budget: &Arc<Budget>) -> Option<Geometry> {
        let mut g = Geometry::new(kind, 0, 0, 0, 0);
        if matches!(kind, 3 | 6) {
            let count = self.unsigned()? as usize;
            if count < if kind == 3 { 3 } else { 2 } || count > u32::MAX as usize / 8 {
                return None;
            }
            if count.checked_mul(2 * self.size)? > self.data.len() - self.offset {
                return None;
            }
            g.reservation = Some(budget.reserve((count * 8) as u64, "region polygon").ok()?);
            g.points.try_reserve_exact(count * 2).ok()?;
            for _ in 0..count * 2 {
                g.points.push(self.signed()?);
            }
        } else {
            g.x = self.signed()?;
            g.y = self.signed()?;
            if kind != 0 {
                g.width = self.unsigned()?;
                g.height = self.unsigned()?;
            }
            if kind == 5 {
                if self.bytes(1)?[0] != 0 || g.width == 0 || g.height == 0 {
                    return None;
                }
                let len = (u64::from(g.width) * u64::from(g.height)).div_ceil(8);
                if len > isize::MAX as u64
                    || limits.max_image_size_pixels / u64::from(g.width) < u64::from(g.height)
                {
                    return None;
                }
                let data = self.bytes(len as usize)?;
                g.reservation = Some(budget.reserve(len, "region mask").ok()?);
                g.mask = data.to_vec();
            }
        }
        Some(g)
    }
}
impl RegionItem {
    pub fn parse(id: u32, data: &[u8], limits: Limits, budget: &Arc<Budget>) -> Self {
        let mut item = Self {
            id,
            width: 0,
            height: 0,
            regions: Vec::new(),
        };
        // The pinned context deliberately ignores the parser's returned error.
        // Completed geometries remain visible; an incomplete geometry is discarded.
        if data.len() < 8 {
            return item;
        }
        let size = if data[1] & 1 == 0 { 2 } else { 4 };
        if size == 4 && data.len() < 12 {
            return item;
        }
        let mut r = Reader {
            data,
            offset: 2,
            size,
        };
        item.width = r.unsigned().unwrap();
        item.height = r.unsigned().unwrap();
        let count = r.bytes(1).unwrap()[0];
        for _ in 0..count {
            let Some(kind) = r.bytes(1).map(|b| i32::from(b[0])) else {
                break;
            };
            if kind > 6 {
                continue;
            }
            let Some(g) = r.geometry(kind, limits, budget) else {
                break;
            };
            item.regions.push(Arc::new(g));
        }
        item
    }
}
pub struct Transform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}
impl Transform {
    pub fn create(
        properties: &PropertyStore,
        id: u32,
        reference: (u32, u32),
    ) -> Result<Self, ContextError> {
        let mut t = Self {
            a: 1.,
            b: 0.,
            c: 0.,
            d: 1.,
            tx: 0.,
            ty: 0.,
        };
        let Ok(props) = properties.get(id) else {
            return Ok(t);
        };
        let Some(p) = props
            .iter()
            .find(|p| !p.raw && p.kind == *b"ispe" && p.data.len() >= 12)
        else {
            return Ok(t);
        };
        let mut w = u32::from_be_bytes(p.data[4..8].try_into().unwrap());
        let mut h = u32::from_be_bytes(p.data[8..12].try_into().unwrap());
        if w == 0 || h == 0 {
            return Ok(t);
        }
        t.a = f64::from(w) / f64::from(reference.0 as i32);
        t.d = f64::from(h) / f64::from(reference.1 as i32);
        for p in props {
            if p.raw {
                continue;
            }
            match &p.kind {
                b"imir" => {
                    if p.data.first().copied().unwrap_or(0) & 1 != 0 {
                        t.a = -t.a;
                        t.b = -t.b;
                        t.tx = f64::from(w.wrapping_sub(1)) - t.tx;
                    } else {
                        t.c = -t.c;
                        t.d = -t.d;
                        t.ty = f64::from(h.wrapping_sub(1)) - t.ty;
                    }
                }
                b"irot" => match p.data.first().copied().unwrap_or(0) & 3 {
                    1 => {
                        t = Self {
                            a: t.c,
                            b: t.d,
                            c: -t.a,
                            d: -t.b,
                            tx: t.ty,
                            ty: -t.tx + f64::from(w) - 1.,
                        };
                        std::mem::swap(&mut w, &mut h);
                    }
                    2 => {
                        t.a = -t.a;
                        t.b = -t.b;
                        t.tx = f64::from(w.wrapping_sub(1)) - t.tx;
                        t.c = -t.c;
                        t.d = -t.d;
                        t.ty = f64::from(h.wrapping_sub(1)) - t.ty;
                    }
                    3 => {
                        t = Self {
                            a: -t.c,
                            b: -t.d,
                            c: t.a,
                            d: t.b,
                            tx: -t.ty + f64::from(h) - 1.,
                            ty: t.tx,
                        };
                        std::mem::swap(&mut w, &mut h);
                    }
                    _ => (),
                },
                b"clap" => {
                    let c = crate::geometry::CleanAperture::parse(&p.data)?;
                    let (left, _, top, _) = c.unclamped_crop(w, h)?;
                    t.tx -= left as f64;
                    t.ty -= top as f64;
                    (w, h) = c.dimensions();
                }
                _ => (),
            }
        }
        Ok(t)
    }
    pub fn point(&self, x: i32, y: i32) -> (f64, f64) {
        // Upstream uses x for both terms of the first row (also on rotations).
        (
            f64::from(x) * self.a + f64::from(x) * self.b + self.tx,
            f64::from(x) * self.c + f64::from(y) * self.d + self.ty,
        )
    }
    pub fn extent(&self, w: u32, h: u32) -> (f64, f64) {
        (
            f64::from(w) * self.a + f64::from(h) * self.b,
            f64::from(w) * self.c + f64::from(h) * self.d,
        )
    }
}
