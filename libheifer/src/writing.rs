// SPDX-License-Identifier: LGPL-3.0-or-later
//! In-memory file layout and deterministic BMFF serialization.
use crate::{
    context::{Context, ContextError},
    items::Reference,
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
type Result<T> = std::result::Result<T, ContextError>;
pub type SharedLayout = Arc<Mutex<FileLayout>>;
#[derive(Clone)]
pub struct FileLayout {
    pub major: u32,
    pub minor: u32,
    pub brands: Vec<u32>,
    pub meta: Vec<[u8; 4]>,
    pub image: bool,
    pub primary: u32,
    pub mini: bool,
    pub unif: bool,
    next: [u32; 4],
}
impl Default for FileLayout {
    fn default() -> Self {
        Self {
            major: 0,
            minor: 0,
            brands: Vec::new(),
            meta: Vec::new(),
            image: false,
            primary: 0,
            mini: false,
            unif: false,
            next: [1; 4],
        }
    }
}
impl FileLayout {
    pub fn compatible(&mut self, brand: u32) {
        if !self.brands.contains(&brand) {
            self.brands.push(brand);
        }
    }
    pub fn init_meta(&mut self) {
        for kind in [*b"hdlr", *b"iloc", *b"iinf"] {
            if !self.meta.contains(&kind) {
                self.meta.push(kind);
            }
        }
    }
    pub fn init_properties(&mut self) {
        self.init_meta();
        self.add_meta(*b"iprp");
    }
    pub fn init_image(&mut self) {
        self.init_meta();
        self.image = true;
        self.add_meta(*b"pitm");
        self.init_properties();
    }
    pub fn add_meta(&mut self, kind: [u8; 4]) {
        if !self.meta.contains(&kind) {
            self.meta.push(kind);
        }
    }
    pub fn mint(&mut self, namespace: usize) -> Result<u32> {
        let n = if self.unif { 3 } else { namespace };
        let id = self.next[n];
        if id == 0 {
            return Err(ContextError::new(
                5,
                0,
                "Usage error: Unspecified: ID namespace overflow",
            ));
        }
        self.next[n] = id.checked_add(1).unwrap_or(0);
        if !self.unif && self.next[3] != 0 && id >= self.next[3] {
            self.next[3] = self.next[n];
        }
        Ok(id)
    }
    pub fn mark(&mut self, namespace: usize, id: u32) {
        for n in [namespace, 3] {
            if self.next[n] != 0 && id >= self.next[n] {
                self.next[n] = id.checked_add(1).unwrap_or(0);
            }
        }
    }
}
pub(crate) fn number(out: &mut Vec<u8>, n: u64, size: usize) {
    out.extend_from_slice(&n.to_be_bytes()[8 - size..]);
}
pub(crate) fn boxed(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if data.len() as u64 + 8 > u64::from(u32::MAX) {
        number(&mut out, 1, 4);
        out.extend(kind);
        number(&mut out, data.len() as u64 + 16, 8);
    } else {
        number(&mut out, data.len() as u64 + 8, 4);
        out.extend(kind);
    }
    out.extend(data);
    out
}
pub(crate) fn full(kind: [u8; 4], version: u8, flags: u32, data: &[u8]) -> Vec<u8> {
    let mut b = vec![version];
    number(&mut b, u64::from(flags), 3);
    b.extend(data);
    boxed(kind, &b)
}
impl Context {
    pub fn serialize(&mut self) -> Result<Vec<u8>> {
        if self.properties.read_only {
            return Err(ContextError::new(
                4,
                0,
                "Unsupported feature: Unspecified: Writing a context that was read from a file is not supported",
            ));
        }
        self.sequences.finalize();
        if let Some(doc) = &self.document {
            for image in doc.images.values() {
                for &id in image.region_ids.lock().unwrap().iter() {
                    self.items.add_reference(Reference {
                        from: id,
                        kind: u32::from_be_bytes(*b"cdsc"),
                        to: vec![image.id],
                    });
                }
                for &id in image.text_ids.lock().unwrap().iter() {
                    self.items.add_reference(Reference {
                        from: id,
                        kind: u32::from_be_bytes(*b"text"),
                        to: vec![image.id],
                    });
                }
            }
        }
        for region in &self.region_items {
            let r = region.lock().unwrap();
            self.items.append_written(r.id, 0, r.encode()?)?;
        }
        for text in &self.text_items {
            self.items
                .append_written(text.id, 1, text.content.clone())?;
        }
        // The native writer stably moves descriptive associations before transforms.
        for (id, indices) in &mut self.properties.items {
            let essential = self.properties.essential.entry(*id).or_default();
            essential.resize(indices.len(), false);
            let mut pairs: Vec<_> = indices
                .iter()
                .copied()
                .zip(essential.iter().copied())
                .collect();
            pairs.sort_by_key(|(i, _)| {
                self.properties
                    .boxes
                    .get(*i)
                    .is_some_and(|p| !p.raw && matches!(&p.kind, b"clap" | b"irot" | b"imir"))
            });
            *indices = pairs.iter().map(|p| p.0).collect();
            *essential = pairs.into_iter().map(|p| p.1).collect();
        }
        let mut layout = self.items.layout.lock().unwrap();
        let structural =
            layout.image && !self.items.items.is_empty() && layout.meta.contains(b"iprp");
        let image_brand = self
            .document
            .as_ref()
            .and_then(|doc| doc.images.get(&doc.primary))
            .and_then(|image| match &image.kind {
                b"av01" => Some(*b"avif"),
                b"avc1" => Some(*b"avci"),
                b"vvc1" => Some(*b"vvic"),
                b"jpeg" => Some(*b"jpeg"),
                b"j2k1" => Some(*b"j2ki"),
                b"hvc1" => {
                    let properties = image.retained_properties.lock().unwrap();
                    let config = properties.iter().find(|p| p.kind == *b"hvcC");
                    let main = config.is_some_and(|p| {
                        let profile = p.data.get(1).copied().unwrap_or(0) & 31;
                        let flags = p.data.get(2).copied().unwrap_or(0);
                        matches!(profile, 1 | 3) || flags & 0x50 != 0
                    });
                    Some(if main { *b"heic" } else { *b"heix" })
                }
                _ => None,
            });
        if layout.major == 0 && structural {
            layout.major = u32::from_be_bytes(image_brand.unwrap_or(*b"mif1"));
        }
        if !self.sequences.tracks.is_empty() && layout.major == 0 {
            layout.major = u32::from_be_bytes(*b"msf1");
        }
        if layout.major == 0 {
            return Err(ContextError::new(
                5,
                0,
                "Usage error: Unspecified: Cannot write a file that contains neither images nor an image sequence",
            ));
        }
        if structural {
            layout.compatible(u32::from_be_bytes(*b"mif1"));
            if let Some(brand) = image_brand {
                layout.compatible(u32::from_be_bytes(brand));
            }
        }
        if structural
            && self
                .document
                .as_ref()
                .is_some_and(|doc| doc.images.get(&doc.primary).is_some_and(|image| image.miaf))
        {
            layout.compatible(u32::from_be_bytes(*b"miaf"));
        }
        if !self.sequences.tracks.is_empty() {
            layout.compatible(u32::from_be_bytes(*b"msf1"));
            layout.compatible(u32::from_be_bytes(*b"isom"));
        }
        if layout.unif {
            layout.compatible(u32::from_be_bytes(*b"unif"));
        }
        layout.minor = 0;
        if layout.mini
            && let Some((out, brand)) = self.write_mini(layout.primary)
        {
            layout.major = u32::from_be_bytes(*b"mif3");
            layout.minor = brand;
            layout.brands.clear();
            return Ok(out);
        }
        let snapshot = layout.clone();
        drop(layout);
        let layout = snapshot;
        let mut ftyp = Vec::new();
        number(&mut ftyp, u64::from(layout.major), 4);
        number(&mut ftyp, 0, 4);
        for brand in &layout.brands {
            number(&mut ftyp, u64::from(*brand), 4);
        }
        let mut out = boxed(*b"ftyp", &ftyp);
        let metadata = |base: u64| self.write_meta(&layout, base, false);
        let meta_size = if layout.meta.is_empty() {
            0
        } else {
            metadata(0).len()
        };
        let moov_size = self.sequences.moov(0).len();
        let item_base = out.len() as u64 + meta_size as u64 + moov_size as u64 + 8;
        let mut item_data = Vec::new();
        for id in &self.items.location_order {
            let loc = &self.items.locations[id];
            if loc.method == 0
                && let Some(bytes) = &loc.owned
            {
                item_data.extend_from_slice(bytes);
            }
        }
        let sequence_base = item_base
            + if meta_size == 0 {
                0
            } else {
                item_data.len() as u64 + 8
            };
        if self.sequences.before_meta {
            out.extend(self.sequences.moov(sequence_base));
        }
        if meta_size != 0 {
            out.extend(metadata(item_base));
        }
        if !self.sequences.before_meta {
            out.extend(self.sequences.moov(sequence_base));
        }
        if meta_size != 0 {
            out.extend(boxed(*b"mdat", &item_data));
        }
        if !self.sequences.tracks.is_empty() {
            out.extend(boxed(*b"mdat", &self.sequences.data));
        }
        // A child write error stops its parent; the native pointer patch walk
        // still visits subsequent unwritten tracks, whose offset position is zero.
        let mut unwritten = false;
        for track in self.sequences.tracks.values() {
            let t = track.lock().unwrap();
            if unwritten {
                for (i, offset) in t.offsets.iter().enumerate() {
                    if let Some(dst) = out.get_mut(i * 4..i * 4 + 4) {
                        dst.copy_from_slice(&((sequence_base + offset) as u32).to_be_bytes());
                    }
                }
            }
            if t.references.iter().any(|(_, ids)| {
                ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
            }) {
                unwritten = true;
            }
        }
        let mut debug_base = item_base;
        for id in &self.items.location_order {
            let loc = self.items.locations.get_mut(id).unwrap();
            if loc.method == 0
                && let Some(data) = &loc.owned
            {
                loc.base = debug_base;
                debug_base += data.len() as u64;
            }
        }
        Ok(out)
    }
    pub(crate) fn write_meta(
        &self,
        layout: &FileLayout,
        mdat_base: u64,
        include_unwritable_references: bool,
    ) -> Vec<u8> {
        let mut children = Vec::new();
        for kind in &layout.meta {
            match kind {
                b"hdlr" => {
                    let mut p = vec![0; 4];
                    p.extend(if layout.image { b"pict" } else { b"null" });
                    p.extend([0; 13]);
                    children.extend(full(*kind, 0, 0, &p));
                }
                b"pitm" => {
                    let mut p = Vec::new();
                    let wide = layout.primary > 65535;
                    number(&mut p, u64::from(layout.primary), if wide { 4 } else { 2 });
                    children.extend(full(*kind, u8::from(wide), 0, &p));
                }
                b"iinf" => {
                    let wide = self.items.items.len() > 65535;
                    let mut p = Vec::new();
                    number(
                        &mut p,
                        self.items.items.len() as u64,
                        if wide { 4 } else { 2 },
                    );
                    for (&id, item) in &self.items.items {
                        let wide = id > 65535;
                        let mut info = Vec::new();
                        number(&mut info, u64::from(id), if wide { 4 } else { 2 });
                        number(&mut info, 0, 2);
                        number(&mut info, u64::from(item.kind), 4);
                        info.extend(item.name.to_bytes_with_nul());
                        if item.kind == u32::from_be_bytes(*b"mime") {
                            info.extend(item.content_type.to_bytes_with_nul());
                            info.extend(item.content_encoding.to_bytes_with_nul());
                        } else if item.kind == u32::from_be_bytes(*b"uri ") {
                            info.extend(item.uri_type.to_bytes_with_nul());
                        }
                        p.extend(full(
                            *b"infe",
                            if wide { 3 } else { 2 },
                            u32::from(item.hidden),
                            &info,
                        ));
                    }
                    children.extend(full(*kind, u8::from(wide), 0, &p));
                }
                b"iloc" => {
                    let mut idat = Vec::new();
                    for id in &self.items.location_order {
                        let loc = &self.items.locations[id];
                        if loc.method == 1
                            && let Some(data) = &loc.owned
                        {
                            idat.extend_from_slice(data);
                        }
                    }
                    if !idat.is_empty() {
                        children.extend(boxed(*b"idat", &idat));
                    }
                    let wide = self.items.location_order.len() > 65535
                        || self.items.location_order.iter().any(|id| *id > 65535);
                    let method = self.items.locations.values().any(|l| l.method != 0);
                    let version = if wide { 2 } else { u8::from(method) };
                    let mut p = vec![0x44, 0x40];
                    number(
                        &mut p,
                        self.items.location_order.len() as u64,
                        if wide { 4 } else { 2 },
                    );
                    let mut mdat_at = mdat_base;
                    let mut idat_at = 0;
                    for id in &self.items.location_order {
                        let loc = &self.items.locations[id];
                        let bytes = loc.owned.as_deref().map(Vec::as_slice).unwrap_or_default();
                        number(&mut p, u64::from(*id), if wide { 4 } else { 2 });
                        if version > 0 {
                            number(&mut p, loc.method, 2);
                        }
                        number(&mut p, 0, 2);
                        number(&mut p, if loc.method == 0 { mdat_at } else { 0 }, 4);
                        number(&mut p, loc.extents.len() as u64, 2);
                        for &(offset, len) in &loc.extents {
                            number(
                                &mut p,
                                offset + if loc.method == 1 { idat_at } else { 0 },
                                4,
                            );
                            number(&mut p, len, 4);
                        }
                        if loc.method == 0 {
                            mdat_at += bytes.len() as u64;
                        } else {
                            idat_at += bytes.len() as u64;
                        }
                    }
                    children.extend(full(*kind, version, 0, &p));
                }
                b"iprp" => {
                    let mut ipco = Vec::new();
                    let mut incomplete = false;
                    let mut property_failed = false;
                    for prop in &self.properties.boxes {
                        if prop.kind == *b"icef"
                            && !prop.raw
                            && crate::uncompressed::compression::units(&prop.data, None)
                                .is_ok_and(|units| units.iter().any(|&(_, size)| size == 0))
                        {
                            // Native container writers preserve the already written prefix
                            // when an icef child rejects an undefined compressed tile.
                            incomplete = true;
                            break;
                        }
                        let mut p = Vec::new();
                        if prop.kind == *b"uuid" {
                            p.extend(prop.uuid.unwrap_or([0; 16]));
                        }
                        p.extend(prop.serialized_data());
                        if prop.write_error.is_some() {
                            ipco.extend([0; 8]);
                            ipco.extend(p);
                            property_failed = true;
                            break;
                        }
                        ipco.extend(boxed(prop.kind, &p));
                    }
                    let mut iprp = boxed(*b"ipco", &ipco);
                    if property_failed {
                        children.extend(boxed(*kind, &iprp));
                        break;
                    }
                    if incomplete {
                        children.extend(boxed(*kind, &iprp));
                        continue;
                    }
                    let wide = self.properties.items.keys().any(|id| *id > 65535);
                    let large = self.properties.items.values().flatten().any(|i| *i >= 127);
                    let mut p = Vec::new();
                    number(&mut p, self.properties.items.len() as u64, 4);
                    for (&id, props) in &self.properties.items {
                        number(&mut p, u64::from(id), if wide { 4 } else { 2 });
                        number(&mut p, props.len() as u64, 1);
                        for (n, &i) in props.iter().enumerate() {
                            let essential = self
                                .properties
                                .essential
                                .get(&id)
                                .and_then(|v| v.get(n))
                                .copied()
                                .unwrap_or(false);
                            let raw = (i as u64 + 1) & if large { 0x7fff } else { 0x7f };
                            number(
                                &mut p,
                                raw | if essential {
                                    if large { 0x8000 } else { 0x80 }
                                } else {
                                    0
                                },
                                if large { 2 } else { 1 },
                            );
                        }
                    }
                    iprp.extend(full(*b"ipma", u8::from(wide), u32::from(large), &p));
                    children.extend(boxed(*kind, &iprp));
                }
                b"iref" => {
                    let mut duplicate = false;
                    for r in &self.items.references {
                        let mut ids = BTreeSet::new();
                        if r.to.iter().any(|id| !ids.insert(id)) {
                            duplicate = true;
                        }
                    }
                    if duplicate && !include_unwritable_references {
                        continue;
                    }
                    let wide = self
                        .items
                        .references
                        .iter()
                        .any(|r| r.from > 65535 || r.to.iter().any(|id| *id > 65535));
                    let mut p = Vec::new();
                    for r in &self.items.references {
                        let mut b = Vec::new();
                        number(&mut b, u64::from(r.from), if wide { 4 } else { 2 });
                        number(&mut b, r.to.len() as u64, 2);
                        for id in &r.to {
                            number(&mut b, u64::from(*id), if wide { 4 } else { 2 });
                        }
                        p.extend(boxed(r.kind.to_be_bytes(), &b));
                    }
                    children.extend(full(*kind, u8::from(wide), 0, &p));
                }
                _ => {}
            }
        }
        full(*b"meta", 0, 0, &children)
    }
}
impl crate::regions::RegionItem {
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.regions.len() >= 256 {
            return Err(ContextError::new(
                9,
                5004,
                "Error during encoding or writing output file: Too many regions (>255) in an 'rgan' item.",
            ));
        }
        let wide = (self.width <= 65535 && self.height <= 65535)
            || self.regions.iter().any(|g| {
                g.x < i16::MIN as i32
                    || g.x > i16::MAX as i32
                    || g.y < i16::MIN as i32
                    || g.y > i16::MAX as i32
                    || g.width > 65535
                    || g.height > 65535
                    || g.points.len() / 2 > 65535
                    || g.points
                        .iter()
                        .any(|p| *p < i16::MIN as i32 || *p > i16::MAX as i32)
            });
        let size = if wide { 4 } else { 2 };
        let mut p = vec![0, u8::from(wide)];
        number(&mut p, u64::from(self.width), size);
        number(&mut p, u64::from(self.height), size);
        number(&mut p, self.regions.len() as u64, 1);
        for g in &self.regions {
            p.push(g.kind as u8);
            if matches!(g.kind, 3 | 6) {
                number(&mut p, (g.points.len() / 2) as u64, size);
                for &v in &g.points {
                    number(&mut p, v as u64, size);
                }
            } else {
                number(&mut p, g.x as u64, size);
                number(&mut p, g.y as u64, size);
                if g.kind != 0 {
                    number(&mut p, u64::from(g.width), size);
                    number(&mut p, u64::from(g.height), size);
                }
                if g.kind == 5 {
                    p.push(0);
                    p.extend(&g.mask);
                }
            }
        }
        Ok(p)
    }
}
