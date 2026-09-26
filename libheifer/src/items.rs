// SPDX-License-Identifier: LGPL-3.0-or-later
// Item semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! File-level items, owned writer payloads, and ordered reference entries.
use crate::{
    context::{ContextError, Input},
    security::Limits,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
    sync::Arc,
};
type Result<T> = std::result::Result<T, ContextError>;

#[derive(Clone)]
pub struct Item {
    pub kind: u32,
    pub hidden: bool,
    pub name: CString,
    pub content_type: CString,
    pub content_encoding: CString,
    pub uri_type: CString,
}
impl Item {
    pub fn new(kind: [u8; 4]) -> Self {
        Self {
            kind: u32::from_be_bytes(kind),
            hidden: true,
            name: CString::default(),
            content_type: CString::default(),
            content_encoding: CString::default(),
            uri_type: CString::default(),
        }
    }
    pub fn compression(&self) -> i32 {
        if self.kind != u32::from_be_bytes(*b"mime") {
            return 0;
        }
        match self.content_encoding.to_bytes() {
            b"" | b"identity" => 0,
            b"deflate" => 3,
            b"compress_zlib" => 4,
            b"br" => 5,
            _ => 2,
        }
    }
}
#[derive(Clone)]
pub struct Reference {
    pub from: u32,
    pub kind: u32,
    pub to: Vec<u32>,
}
#[derive(Clone)]
pub(crate) struct Location {
    pub(crate) method: u64,
    pub(crate) base: u64,
    pub(crate) extents: Vec<(u64, u64)>,
    pub(crate) owned: Option<Arc<Vec<u8>>>,
}

#[derive(Clone, Default)]
pub struct ItemStore {
    pub items: BTreeMap<u32, Item>,
    pub references: Vec<Reference>,
    pub has_iloc: bool,
    pub(crate) locations: BTreeMap<u32, Location>,
    pub(crate) location_order: Vec<u32>,
    pub layout: crate::writing::SharedLayout,
    pub(crate) input: Option<Arc<dyn Input>>,
    pub(crate) idat: Option<(usize, u64)>,
}
impl ItemStore {
    pub(crate) fn parse_tables(boxes: &[([u8; 4], &[u8])], limits: Limits) -> Result<Self> {
        let mut result = Self::default();
        let mut seen = BTreeSet::new();
        for (kind, data) in boxes {
            let mut parsed = Self::default();
            match kind {
                b"iinf" => parsed.items = Self::parse_info(data, limits)?,
                b"iloc" => parsed.parse_locations(data, limits)?,
                b"iref" => parsed.parse_references(data, limits)?,
                _ => continue,
            }
            // Every box is parsed, but pointer installation selects the first.
            if !seen.insert(kind) {
                continue;
            }
            match kind {
                b"iinf" => result.items = parsed.items,
                b"iloc" => {
                    result.locations = parsed.locations;
                    result.has_iloc = true;
                }
                b"iref" => result.references = parsed.references,
                _ => unreachable!(),
            }
        }
        Ok(result)
    }
    pub(crate) fn reading(input: Arc<dyn Input>) -> Self {
        Self {
            input: Some(input),
            ..Self::default()
        }
    }
    pub(crate) fn seed(&mut self) {
        if let Some(id) = self.items.keys().next_back() {
            self.layout.lock().unwrap().mark(0, *id);
        }
    }
    pub fn add(&mut self, item: Item, data: Vec<u8>) -> Result<u32> {
        self.add_compressed(item, &data, 0)
    }
    /// Text payloads are materialized into iloc only when the context is written.
    pub fn add_pending(&mut self, item: Item) -> Result<u32> {
        self.layout.lock().unwrap().init_meta();
        self.has_iloc = true;
        let id = self.mint()?;
        self.items.insert(id, item);
        Ok(id)
    }
    pub fn add_compressed(&mut self, mut item: Item, data: &[u8], compression: i32) -> Result<u32> {
        self.layout.lock().unwrap().init_meta();
        self.has_iloc = true;
        let id = self.mint()?;
        item.content_encoding = match compression {
            3 => c"deflate".into(),
            4 => c"compress_zlib".into(),
            5 => c"br".into(),
            _ => item.content_encoding,
        };
        self.items.insert(id, item);
        let data = match crate::compression::compress(data, compression) {
            Ok(data) => data,
            Err(error) => {
                self.items.remove(&id);
                return Err(error);
            }
        };
        self.location_order.push(id);
        self.locations.insert(
            id,
            Location {
                method: 0,
                base: 0,
                extents: vec![(0, data.len() as u64)],
                owned: Some(Arc::new(data)),
            },
        );
        Ok(id)
    }
    pub fn mint(&mut self) -> Result<u32> {
        self.layout.lock().unwrap().mint(0)
    }
    pub fn add_reference(&mut self, reference: Reference) {
        self.layout.lock().unwrap().add_meta(*b"iref");
        self.references.push(reference);
    }
    pub(crate) fn append_written(&mut self, id: u32, method: u64, data: Vec<u8>) -> Result<()> {
        self.has_iloc = true;
        if !self.locations.contains_key(&id) {
            self.location_order.push(id);
        }
        let loc = self.locations.entry(id).or_insert(Location {
            method,
            base: 0,
            extents: Vec::new(),
            owned: Some(Arc::new(Vec::new())),
        });
        let bytes = Arc::make_mut(loc.owned.get_or_insert_with(|| Arc::new(Vec::new())));
        if let Some((_, len)) = loc.extents.last_mut() {
            *len = bytes.len() as u64 + data.len() as u64;
        } else {
            loc.extents.push((0, data.len() as u64));
        }
        bytes.try_reserve(data.len()).map_err(|_| allocation())?;
        bytes.extend(data);
        Ok(())
    }
    pub fn item_data(&self, id: u32, limits: Limits) -> Result<Vec<u8>> {
        if !self.has_iloc {
            return Err(ContextError::invalid(110, "No 'iloc' box"));
        }
        if !self.items.contains_key(&id) {
            return Err(ContextError::new(
                5,
                2000,
                "Usage error: Non-existing item ID referenced",
            ));
        }
        let loc = self.locations.get(&id).ok_or_else(|| {
            ContextError::invalid(
                117,
                &format!("Item has no data: Item with ID {id} has no compressed data"),
            )
        })?;
        let mut out = Vec::new();
        if let Some(data) = &loc.owned {
            if limits.max_memory_block_size != 0 && data.len() as u64 > limits.max_memory_block_size
            {
                return Err(memory(
                    "iloc item data exceeds the maximum memory block size",
                ));
            }
            out.try_reserve_exact(data.len())
                .map_err(|_| allocation())?;
            out.extend_from_slice(data);
            return Ok(out);
        }
        let source = self.input.as_ref().map_or(&[][..], |i| i.bytes());
        for &(offset, size) in &loc.extents {
            if size == 0 {
                continue;
            }
            let relative = offset.checked_add(loc.base).ok_or_else(|| {
                ContextError::invalid(
                    1000,
                    "Security limit exceeded: iloc data pointers out of allowed range",
                )
            })?;
            let (start, end) = if loc.method == 0 {
                if [offset, loc.base, size]
                    .iter()
                    .any(|n| *n > 0x007f_ffff_ffff_ffff)
                {
                    return Err(ContextError::invalid(
                        1000,
                        "Security limit exceeded: iloc data pointers out of allowed range",
                    ));
                }
                let end = relative.checked_add(size);
                if end.is_none_or(|n| n > self.input.as_ref().map_or(0, |i| i.length())) {
                    return Err(ContextError::invalid(
                        100,
                        &format!(
                            "Unexpected end of file: Extent in iloc box references data outside of file bounds (points to file position {relative})\n"
                        ),
                    ));
                }
                check_extent(&out, size, limits, "iloc")?;
                (relative, end.unwrap())
            } else if loc.method == 1 {
                let (at, box_size) = self.idat.ok_or_else(|| {
                    ContextError::invalid(
                        103,
                        "No 'idat' box: idat box referenced in iref box is not present in file",
                    )
                })?;
                check_extent(&out, size, limits, "idat")?;
                let relative_end = relative.checked_add(size).ok_or_else(truncated)?;
                if relative_end > box_size {
                    return Err(truncated());
                }
                let base = self.input.as_ref().map_or(at as u64, |input| {
                    input.original_offset(&input.bytes()[at..])
                });
                let start = base.checked_add(relative).ok_or_else(truncated)?;
                (start, start.checked_add(size).ok_or_else(truncated)?)
            } else {
                return Err(ContextError::new(
                    4,
                    3004,
                    format!(
                        "Unsupported feature: Unsupported item construction method: Item construction method {} not implemented",
                        loc.method
                    ),
                ));
            };
            let data = if let Some(input) = &self.input {
                if loc.method == 1 {
                    input.read_idat(start, end - start)?
                } else {
                    input.read_range(start, end - start)?
                }
            } else {
                std::borrow::Cow::Borrowed(
                    source
                        .get(
                            usize::try_from(start).map_err(|_| truncated())?
                                ..usize::try_from(end).map_err(|_| truncated())?,
                        )
                        .ok_or_else(truncated)?,
                )
            };
            out.try_reserve_exact(data.len())
                .map_err(|_| allocation())?;
            out.extend_from_slice(&data);
        }
        Ok(out)
    }
    pub(crate) fn parse_info(data: &[u8], limits: Limits) -> Result<BTreeMap<u32, Item>> {
        let mut r = Reader(data);
        let (v, _) = r.full(255, "iinf").unwrap_or((0, 0));
        // Box_iinf treats an unreadable zero count as an empty box.
        let count = r.n(if v == 0 { 2 } else { 4 }).unwrap_or(0);
        if limits.max_items != 0 && count > u64::from(limits.max_items) {
            return Err(memory(&format!(
                "iinf box contains {count} items, which exceeds the security limit of {} items.",
                limits.max_items
            )));
        }
        let mut items = BTreeMap::new();
        for _ in 0..count {
            let (kind, body) = r.box_body()?;
            if kind != *b"infe" {
                continue;
            }
            let mut info = Reader(body);
            let (v, flags) = info.full(3, "infe")?;
            let id = info.n(if v == 3 { 4 } else { 2 })? as u32;
            info.n(2)?;
            let kind = if v >= 2 { info.n(4)? as u32 } else { 0 };
            let mut item = Item::new(kind.to_be_bytes());
            item.hidden = v >= 2 && flags & 1 != 0;
            item.name = info.string();
            if v < 2 || kind == u32::from_be_bytes(*b"mime") {
                item.content_type = info.string();
                item.content_encoding = info.string();
            }
            if kind == u32::from_be_bytes(*b"uri ") {
                item.uri_type = info.string();
            }
            items.entry(id).or_insert(item);
        }
        Ok(items)
    }
    pub(crate) fn parse_locations(&mut self, data: &[u8], limits: Limits) -> Result<()> {
        let mut r = Reader(data);
        let (v, _) = r.full(2, "iloc")?;
        let sizes = r.n(2)?;
        let offset = sizes >> 12;
        let length = (sizes >> 8) & 15;
        let base = (sizes >> 4) & 15;
        let index = if v == 0 { 0 } else { sizes & 15 };
        let count = r.n(if v < 2 { 2 } else { 4 })?;
        if limits.max_items != 0 && count > u64::from(limits.max_items) {
            return Err(memory(&format!(
                "iloc box contains {count} items, which exceeds the security limit of {} items.",
                limits.max_items
            )));
        }
        for _ in 0..count {
            let id = r.n(if v < 2 { 2 } else { 4 })? as u32;
            let method = if v > 0 { r.n(2)? & 15 } else { 0 };
            r.n(2)?;
            let base = r.width(base)?;
            let count = r.n(2)?;
            if limits.max_iloc_extents_per_item != 0
                && count > u64::from(limits.max_iloc_extents_per_item)
            {
                return Err(memory(&format!(
                    "Number of extents in iloc box ({count}) exceeds security limit ({})\n",
                    limits.max_iloc_extents_per_item
                )));
            }
            let mut extents = Vec::new();
            for _ in 0..count {
                if r.0.is_empty() {
                    return Err(truncated());
                }
                r.width(index)?;
                extents.push((r.width(offset)?, r.width(length)?));
            }
            self.locations.entry(id).or_insert(Location {
                method,
                base,
                extents,
                owned: None,
            });
        }
        self.has_iloc = true;
        Ok(())
    }
    pub(crate) fn parse_references(&mut self, data: &[u8], limits: Limits) -> Result<()> {
        let mut r = Reader(data);
        let (v, _) = r.full(1, "iref")?;
        let mut duplicates = false;
        while !r.0.is_empty() {
            if limits.max_items != 0 && self.references.len() >= limits.max_items as usize {
                return Err(ContextError::invalid(
                    1000,
                    &format!(
                        "Security limit exceeded: 'iref' box contains more than {} reference entries, which exceeds the security limit.",
                        limits.max_items
                    ),
                ));
            }
            let (_, kind) = r.header()?;
            let from = r.n(if v == 0 { 2 } else { 4 })? as u32;
            let count = r.n(2)?;
            if count == 0 {
                return Err(crate::container::ParseError::EmptyReferences.into());
            }
            if limits.max_items != 0 && count > u64::from(limits.max_items) {
                return Err(ContextError::invalid(
                    1000,
                    &format!(
                        "Security limit exceeded: Number of references in iref box ({count}) exceeds the security limits of {} references.",
                        limits.max_items
                    ),
                ));
            }
            let mut to = Vec::new();
            let mut seen = BTreeSet::new();
            for _ in 0..count {
                let id = r.n(if v == 0 { 2 } else { 4 })? as u32;
                duplicates |= !seen.insert(id);
                to.push(id);
            }
            self.references.push(Reference {
                from,
                kind: u32::from_be_bytes(kind),
                to,
            });
        }
        if duplicates {
            return Err(crate::container::ParseError::DoubleReferences.into());
        }
        Ok(())
    }
    pub(crate) fn set_idat(&mut self, data: &[u8]) {
        if let Some(input) = &self.input {
            self.idat = Some((
                data.as_ptr() as usize - input.bytes().as_ptr() as usize,
                data.len() as u64 + 8,
            ));
        }
    }
    pub(crate) fn install_data(&mut self, parsed: Self) {
        self.locations = parsed.locations;
        self.references = parsed.references;
        self.has_iloc = parsed.has_iloc;
        self.idat = parsed.idat;
    }
}
fn check_extent(out: &[u8], size: u64, limits: Limits, kind: &str) -> Result<()> {
    if limits.max_memory_block_size != 0
        && size > limits.max_memory_block_size.wrapping_sub(out.len() as u64)
    {
        return Err(memory(&format!(
            "{kind} box contained {size} bytes, total memory size would be {} bytes, exceeding the security limit of {} bytes",
            (out.len() as u64).saturating_add(size),
            limits.max_memory_block_size
        )));
    }
    Ok(())
}
fn truncated() -> ContextError {
    ContextError::invalid(100, "Unexpected end of file")
}
fn memory(message: &str) -> ContextError {
    ContextError::new(
        6,
        1000,
        format!("Memory allocation error: Security limit exceeded: {message}"),
    )
}
fn allocation() -> ContextError {
    ContextError::new(6, 0, "Memory allocation error")
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn n(&mut self, size: usize) -> Result<u64> {
        let Some((bytes, rest)) = self.0.split_at_checked(size) else {
            self.0 = &[];
            return Err(truncated());
        };
        self.0 = rest;
        Ok(bytes.iter().fold(0, |v, b| v << 8 | u64::from(*b)))
    }
    fn width(&mut self, size: u64) -> Result<u64> {
        if matches!(size, 4 | 8) {
            self.n(size as usize)
        } else {
            Ok(0)
        }
    }
    fn full(&mut self, maximum: u8, kind: &str) -> Result<(u8, u32)> {
        let value = self.n(4)? as u32;
        let v = (value >> 24) as u8;
        if v > maximum {
            return Err(ContextError::new(
                4,
                3002,
                format!(
                    "Unsupported feature: Unsupported data version: {kind} box data version {v} is not implemented yet"
                ),
            ));
        }
        Ok((v, value & 0xffffff))
    }
    fn string(&mut self) -> CString {
        let end = self
            .0
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(self.0.len().saturating_sub(1));
        let string = CString::new(&self.0[..end]).unwrap();
        self.0 = self.0.get(end + 1..).unwrap_or_default();
        string
    }
    fn header(&mut self) -> Result<(u64, [u8; 4])> {
        let mut size = self.n(4)?;
        let kind = (self.n(4)? as u32).to_be_bytes();
        if size == 1 {
            size = self.n(8)?;
            if size > 0x0fff_ffff_ffff_ffff {
                return Err(memory("Box size exceeds security limit."));
            }
        }
        if kind == *b"uuid" {
            self.n(8)?;
            self.n(8)?;
        }
        Ok((size, kind))
    }
    fn box_body(&mut self) -> Result<([u8; 4], &'a [u8])> {
        let before = self.0.len();
        let (size, kind) = self.header()?;
        let head = before - self.0.len();
        let size = if size == 0 {
            before
        } else {
            usize::try_from(size).map_err(|_| truncated())?
        };
        let size = size
            .checked_sub(head)
            .ok_or_else(|| ContextError::invalid(101, "Invalid box size"))?;
        let (body, rest) = self.0.split_at_checked(size).ok_or_else(truncated)?;
        self.0 = rest;
        Ok((kind, body))
    }
}
