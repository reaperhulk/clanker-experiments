// SPDX-License-Identifier: LGPL-3.0-or-later
// Minimized conversion semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Minimized output from the owned file model, with ordinary-output fallback.
use crate::{context::Context, properties::Property, writing::boxed};
use std::sync::Arc;

#[derive(Default)]
struct Bits {
    data: Vec<u8>,
    at: usize,
}
impl Bits {
    fn put(&mut self, value: u32, count: usize) {
        for i in (0..count).rev() {
            if self.at.is_multiple_of(8) {
                self.data.push(0);
            }
            self.data[self.at / 8] |= ((value >> i & 1) as u8) << (7 - self.at % 8);
            self.at += 1;
        }
    }
    fn flag(&mut self, value: bool) {
        self.put(u32::from(value), 1);
    }
    fn bytes(&mut self, data: &[u8]) {
        for &v in data {
            self.put(u32::from(v), 8);
        }
    }
}
fn scalar(p: &[u8], at: usize, n: usize) -> u32 {
    p.get(at..at + n)
        .unwrap_or_default()
        .iter()
        .fold(0, |a, &b| a << 8 | u32::from(b))
}
impl Context {
    fn mini_properties(&self, id: u32) -> Vec<&Property> {
        self.properties
            .items
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|&i| self.properties.boxes.get(i))
            .map(Arc::as_ref)
            .filter(|p| !p.raw)
            .collect()
    }
    fn mini_data(&self, id: u32) -> &[u8] {
        self.items
            .locations
            .get(&id)
            .and_then(|l| l.owned.as_deref())
            .map_or(&[], Vec::as_slice)
    }
    pub(crate) fn write_mini(&self, primary: u32) -> Option<(Vec<u8>, u32)> {
        let item = self.items.items.get(&primary)?;
        let kind = item.kind.to_be_bytes();
        let brand = match &kind {
            b"av01" => *b"avif",
            b"hvc1" => *b"heic",
            _ => return None,
        };
        let props = self.mini_properties(primary);
        if props.iter().any(|p| {
            p.kind == *b"ispe" && (scalar(&p.data, 4, 4) > 32768 || scalar(&p.data, 8, 4) > 32768)
        }) {
            return None;
        }
        let reference = |id, kind| {
            self.items
                .references
                .iter()
                .filter(|r| r.from == id && r.kind == u32::from_be_bytes(kind))
                .flat_map(|r| r.to.iter())
                .next()
                .copied()
        };
        let (mut alpha, mut exif, mut xmp) = (0, 0, 0);
        for (&id, other) in &self.items.items {
            if id == primary {
                continue;
            }
            let other_kind = other.kind.to_be_bytes();
            if matches!(&other_kind, b"grid" | b"iovl" | b"iden") {
                return None;
            }
            if reference(id, *b"auxl") == Some(primary) {
                if alpha != 0 {
                    return None;
                }
                alpha = id;
                continue;
            }
            if reference(id, *b"cdsc") == Some(primary) {
                if other_kind == *b"Exif" {
                    if exif != 0 {
                        return None;
                    }
                    exif = id;
                    continue;
                }
                if other_kind == *b"mime" {
                    if xmp != 0 || other.content_type.to_bytes() != b"application/rdf+xml" {
                        return None;
                    }
                    xmp = id;
                    continue;
                }
            }
            if !other.hidden && other.kind != item.kind {
                return None;
            }
        }
        let compressed = |id| {
            self.items
                .items
                .get(&id)
                .is_some_and(|item| item.content_encoding.to_bytes() == b"deflate")
        };
        if exif != 0 && xmp != 0 && compressed(exif) != compressed(xmp) {
            return None;
        }
        let find = |kind| {
            props
                .iter()
                .rev()
                .find(|p| p.kind == kind)
                .map(|p| p.data.as_slice())
        };
        let config_prop = props
            .iter()
            .rev()
            .find(|p| matches!(&p.kind, b"av1C" | b"hvcC"));
        let config = config_prop.map_or(&[][..], |p| p.data.as_slice());
        let chroma = if brand == *b"avif" {
            if config_prop.is_some_and(|p| p.kind == *b"av1C") {
                let flags = config.get(2).copied().unwrap_or(0);
                match flags & 12 {
                    12 => 1,
                    8 => 2,
                    0 if flags & 16 == 0 => 3,
                    _ => 0,
                }
            } else {
                0
            }
        } else if config_prop.is_some_and(|p| p.kind == *b"hvcC") {
            u32::from(config.get(16).copied().unwrap_or(0) & 3)
        } else {
            0
        };
        let icc = props
            .iter()
            .rev()
            .find(|p| p.kind == *b"colr" && !p.data.starts_with(b"nclx"));
        let icc_data = icc.and_then(|p| p.data.get(4..)).unwrap_or_default();
        let nclx = props
            .iter()
            .rev()
            .find(|p| p.kind == *b"colr" && p.data.starts_with(b"nclx"));
        let defaults = (
            if icc.is_some() { 2 } else { 1 },
            if icc.is_some() { 2 } else { 13 },
            if chroma == 0 { 2 } else { 6 },
        );
        let color = nclx.map_or(defaults, |p| {
            (
                scalar(&p.data, 4, 2),
                scalar(&p.data, 6, 2),
                scalar(&p.data, 8, 2),
            )
        });
        let full_range = nclx.is_some_and(|p| p.data.get(10).is_some_and(|v| v & 128 != 0));
        let ispe = find(*b"ispe").unwrap_or_default();
        let (width, height) = (scalar(ispe, 4, 4), scalar(ispe, 8, 4));
        let depth = find(*b"pixi")
            .filter(|p| p.get(4).is_some_and(|n| *n != 0))
            .and_then(|p| p.get(5))
            .copied()
            .unwrap_or(8);
        let mut orientation = 1;
        for prop in &props {
            let transform = match &prop.kind {
                b"irot" => [1, 8, 3, 6][usize::from(prop.data.first().copied().unwrap_or(0) & 3)],
                b"imir" => {
                    if prop.data.first().copied().unwrap_or(0) & 1 == 1 {
                        2
                    } else {
                        4
                    }
                }
                _ => continue,
            };
            orientation = crate::geometry::orientation_concat(orientation, transform);
        }
        let hdr: Vec<_> = [*b"clli", *b"mdcv", *b"cclv", *b"amve", *b"ndwt"]
            .map(find)
            .into_iter()
            .collect();
        let has_hdr = hdr.iter().any(Option::is_some);
        let alpha_props = self.mini_properties(alpha);
        let alpha_config = alpha_props
            .iter()
            .find(|p| matches!(&p.kind, b"av1C" | b"hvcC"))
            .map_or(&[][..], |p| p.data.as_slice());
        let (main_data, alpha_data, exif_data, xmp_data) = (
            self.mini_data(primary),
            self.mini_data(alpha),
            self.mini_data(exif),
            self.mini_data(xmp),
        );
        let alpha_config = if alpha_data.is_empty() || alpha_config == config {
            &[][..]
        } else {
            alpha_config
        };
        let mut bits = Bits::default();
        bits.put(0, 2);
        for flag in [
            false,
            false,
            full_range,
            alpha != 0,
            color != defaults,
            has_hdr,
            icc.is_some(),
            exif != 0,
            xmp != 0,
        ] {
            bits.flag(flag);
        }
        bits.put(chroma, 2);
        bits.put((orientation - 1) as u32, 3);
        let dimensions = if width > 128 || height > 128 { 15 } else { 7 };
        bits.flag(dimensions == 15);
        bits.put(width.wrapping_sub(1), dimensions);
        bits.put(height.wrapping_sub(1), dimensions);
        if matches!(chroma, 1 | 2) {
            bits.flag(false);
        }
        if chroma == 1 {
            bits.flag(false);
        }
        bits.flag(depth > 8);
        if depth > 8 {
            bits.put(u32::from(depth - 9), 3);
        }
        if alpha != 0 {
            bits.flag(false);
        }
        if color != defaults {
            for v in [color.0, color.1, color.2] {
                bits.put(v, 8);
            }
        }
        if has_hdr {
            bits.flag(false); // Gain-map construction is not implemented by the native converter.
            for present in [
                hdr[0].is_some(),
                hdr[1].is_some(),
                hdr[2].is_some(),
                hdr[3].is_some(),
                false,
                hdr[4].is_some(),
            ] {
                bits.flag(present);
            }
            for (i, p) in hdr.iter().enumerate() {
                if let Some(p) = p {
                    match i {
                        2 => {
                            bits.put(u32::from(p.first().copied().unwrap_or(0) & 0x3c), 8);
                            bits.bytes(&p[1..]);
                        }
                        4 => bits.bytes(p.get(4..).unwrap_or_default()),
                        _ => bits.bytes(p),
                    }
                }
            }
        }
        let metadata_bits = if [icc_data.len(), exif_data.len(), xmp_data.len()]
            .into_iter()
            .max()
            .unwrap_or(0)
            > 1024
        {
            20
        } else {
            10
        };
        if icc.is_some() || exif != 0 || xmp != 0 {
            bits.flag(metadata_bits == 20);
        }
        let config_bits = if config.len() > 7 || alpha_config.len() > 7 {
            12
        } else {
            3
        };
        bits.flag(config_bits == 12);
        let item_bits = if main_data.len() > 32768 || alpha_data.len() > 32767 {
            28
        } else {
            15
        };
        bits.flag(item_bits == 28);
        if icc.is_some() {
            bits.put((icc_data.len() as u32).wrapping_sub(1), metadata_bits);
        }
        bits.put(config.len() as u32, config_bits);
        bits.put((main_data.len() as u32).wrapping_sub(1), item_bits);
        if alpha != 0 {
            bits.put(alpha_data.len() as u32, item_bits);
        }
        if !alpha_data.is_empty() {
            bits.put(alpha_config.len() as u32, config_bits);
        }
        if exif != 0 || xmp != 0 {
            bits.flag(compressed(exif) || compressed(xmp));
        }
        if exif != 0 {
            bits.put((exif_data.len() as u32).wrapping_sub(1), metadata_bits);
        }
        if xmp != 0 {
            bits.put((xmp_data.len() as u32).wrapping_sub(1), metadata_bits);
        }
        for bytes in [
            config,
            alpha_config,
            icc_data,
            alpha_data,
            main_data,
            exif_data,
            xmp_data,
        ] {
            bits.data.extend_from_slice(bytes);
        }
        let mut out = boxed(*b"ftyp", &[b"mif3".as_slice(), &brand].concat());
        out.extend(boxed(*b"mini", &bits.data));
        Some((out, u32::from_be_bytes(brand)))
    }
}
