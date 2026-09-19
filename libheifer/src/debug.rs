// SPDX-License-Identifier: LGPL-3.0-or-later
//! Human-readable BMFF diagnostics. Output bytes retain foreign FourCC/string data.
use crate::context::{Context, header};
use std::io::Write;

struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
    fn bytes(&mut self, n: usize) -> &'a [u8] {
        let end = self.at.saturating_add(n).min(self.data.len());
        let b = &self.data[self.at..end];
        self.at = end;
        b
    }
    fn n(&mut self, n: usize) -> u64 {
        self.bytes(n)
            .iter()
            .fold(0, |a, b| (a << 8) | u64::from(*b))
    }
    fn string(&mut self) -> &'a [u8] {
        let data = &self.data[self.at..];
        let n = data
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(data.len().saturating_sub(1));
        let out = self.bytes(n);
        self.bytes(1);
        out
    }
}
fn line(out: &mut Vec<u8>, indent: &str, key: &str, value: &[u8]) {
    out.extend_from_slice(indent.as_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(value);
    out.push(b'\n');
}
fn name(kind: &[u8; 4]) -> Option<&'static str> {
    Some(match kind {
        b"ftyp" => "File Type",
        b"hvcC" => "HEVC Configuration Item",
        b"free" => "Free Space",
        b"meta" => "Metadata",
        b"hdlr" => "Handler Reference",
        b"pitm" => "Primary Item",
        b"iloc" => "Item Location",
        b"infe" => "Item Info Entry",
        b"iinf" => "Item Information",
        b"iprp" => "Item Properties",
        b"ipco" => "Item Property Container",
        b"ipma" => "Item Property Association",
        b"ispe" => "Image Spatial Extents",
        b"iref" => "Item Reference",
        b"idat" => "Item Data",
        b"pixi" => "Pixel Information",
        b"pasp" => "Pixel Aspect Ratio",
        b"irot" => "Image Rotation",
        b"imir" => "Image Mirroring",
        b"auxC" => "Image Properties for Auxiliary Images",
        b"clap" => "Clean Aperture",
        b"elng" => "Extended language",
        _ => return None,
    })
}
fn header_text(out: &mut Vec<u8>, indent: &str, kind: &[u8; 4], size: u64, header_size: usize) {
    out.extend_from_slice(indent.as_bytes());
    out.extend_from_slice(b"Box: ");
    out.extend_from_slice(kind);
    if let Some(name) = name(kind) {
        let _ = writeln!(out, " ----- ({name})");
    } else {
        out.extend_from_slice(b" -----\n");
    }
    let _ = writeln!(out, "{indent}size: {size}   (header size: {header_size})");
}
fn children(out: &mut Vec<u8>, data: &[u8], depth: usize, indices: bool, owned: Option<&Context>) {
    if depth > 64 {
        return;
    }
    let indent = "| ".repeat(depth);
    let mut rest = data;
    let mut index = 1;
    while let Ok(h) = header(rest) {
        let size = if h.size == 0 {
            rest.len()
        } else {
            match usize::try_from(h.size) {
                Ok(n) => n,
                Err(_) => break,
            }
        };
        if size < h.header || size > rest.len() {
            break;
        }
        if index > 1 {
            line(out, &indent, "", b"");
        }
        if indices {
            let _ = writeln!(out, "{indent}index: {index}");
        }
        let raw = indices
            && owned
                .and_then(|c| c.properties.boxes.get(index - 1))
                .is_some_and(|p| p.raw);
        if raw {
            out.extend_from_slice(indent.as_bytes());
            out.extend_from_slice(b"Box: ");
            if h.kind == *b"uuid" {
                uuid_text(out, &rest[h.header - 16..h.header]);
            } else {
                out.extend_from_slice(&h.kind);
            }
            let _ = writeln!(out, " -----\n{indent}size: 0   (header size: 0)");
        } else {
            dump_box(out, &rest[..size], depth, owned);
        }
        index += 1;
        rest = &rest[size..];
    }
}
fn dump_box(out: &mut Vec<u8>, data: &[u8], depth: usize, owned: Option<&Context>) {
    let Ok(h) = header(data) else {
        return;
    };
    let indent = "| ".repeat(depth);
    let minimum = match &h.kind {
        b"pasp" => 8,
        b"irot" | b"imir" => 1,
        b"clap" => 32,
        _ => 0,
    };
    if data.len() - h.header < minimum {
        out.extend_from_slice(indent.as_bytes());
        out.push(b'\'');
        out.extend_from_slice(&h.kind);
        out.extend_from_slice(b"' parse error: \n");
        line(
            out,
            &indent,
            "fatality: ",
            if matches!(&h.kind, b"irot" | b"imir" | b"clap") {
                b"ignorable"
            } else {
                b"optional"
            },
        );
        return;
    }
    let full = matches!(
        &h.kind,
        b"meta"
            | b"hdlr"
            | b"pitm"
            | b"iinf"
            | b"infe"
            | b"iloc"
            | b"ipma"
            | b"ispe"
            | b"pixi"
            | b"iref"
            | b"mskC"
            | b"auxC"
            | b"elng"
    );
    let mut r = Cursor::new(&data[h.header..]);
    let vf = if full { r.n(4) } else { 0 };
    let version = vf >> 24;
    let flags = vf & 0xffffff;
    header_text(
        out,
        &indent,
        &h.kind,
        if owned.is_some() { 0 } else { h.size },
        if owned.is_some() {
            0
        } else {
            h.header + if full { 4 } else { 0 }
        },
    );
    match &h.kind {
        b"elng" => {
            line(out, &indent, "extended_language: ", r.string());
        }
        b"hvcC" => {
            let ver = r.n(1);
            let p = r.n(1);
            let _ = writeln!(
                out,
                "{indent}configuration_version: {ver}\n{indent}general_profile_space: {}\n{indent}general_tier_flag: {}\n{indent}general_profile_idc: {}",
                p >> 6,
                (p >> 5) & 1,
                p & 31
            );
            let compat = r.n(4);
            let _ = write!(out, "{indent}general_profile_compatibility_flags: ");
            for i in 0..32 {
                let _ = write!(out, "{}", (compat >> (31 - i)) & 1);
                if i % 8 == 7 {
                    out.push(b' ');
                } else if i % 4 == 3 {
                    out.push(b'.');
                }
            }
            out.push(b'\n');
            let constraint = r.n(6);
            let _ = write!(out, "{indent}general_constraint_indicator_flags: ");
            for i in 0..48 {
                let _ = write!(out, "{}", (constraint >> (47 - i)) & 1);
                if i % 8 == 7 {
                    out.push(b' ');
                }
            }
            out.push(b'\n');
            let _ = writeln!(
                out,
                "{indent}general_level_idc: {}\n{indent}min_spatial_segmentation_idc: {}\n{indent}parallelism_type: {}",
                r.n(1),
                r.n(2) & 4095,
                r.n(1) & 3
            );
            let c = ["monochrome", "4:2:0", "4:2:2", "4:4:4"][(r.n(1) & 3) as usize];
            let _ = writeln!(
                out,
                "{indent}chroma_format: {c}\n{indent}bit_depth_luma: {}\n{indent}bit_depth_chroma: {}\n{indent}avg_frame_rate: {}",
                (r.n(1) & 7) + 8,
                (r.n(1) & 7) + 8,
                r.n(2)
            );
            let t = r.n(1);
            let _ = writeln!(
                out,
                "{indent}constant_frame_rate: {}\n{indent}num_temporal_layers: {}\n{indent}temporal_id_nested: {}\n{indent}length_size: {}",
                t >> 6,
                (t >> 3) & 7,
                (t >> 2) & 1,
                (t & 3) + 1
            );
            let n = r.n(1);
            for _ in 0..n {
                let a = r.n(1);
                let _ = writeln!(
                    out,
                    "{indent}<array>\n{indent}| array_completeness: {}\n{indent}| NAL_unit_type: {}",
                    a >> 7,
                    a & 63
                );
                let n = r.n(2);
                for _ in 0..n.min(r.data.len() as u64) {
                    let size = r.n(2) as usize;
                    let _ = write!(out, "{indent}| ");
                    for b in r.bytes(size) {
                        let _ = write!(out, "{b:02x} ");
                    }
                    out.push(b'\n');
                }
            }
        }
        b"colr" => {
            let kind = r.bytes(4);
            if kind == b"nclx" || kind == b"nclc" {
                line(out, &indent, "colour_type: ", b"nclx");
                let primaries = r.n(2);
                let transfer = r.n(2);
                let matrix = r.n(2);
                let full = if kind == b"nclc" {
                    u64::from(matrix == 0)
                } else {
                    r.n(1) >> 7
                };
                let _ = writeln!(
                    out,
                    "{indent}colour_primaries: {primaries}\n{indent}transfer_characteristics: {transfer}\n{indent}matrix_coefficients: {matrix}\n{indent}full_range_flag: {full}"
                );
            } else {
                line(out, &indent, "colour_type: ", kind);
                let _ = writeln!(out, "{indent}profile size: {}", r.data.len() - r.at);
            }
        }
        b"irot" => {
            let _ = writeln!(out, "{indent}rotation: {} degrees (CCW)", (r.n(1) & 3) * 90);
        }
        b"imir" => {
            line(
                out,
                &indent,
                "mirror direction: ",
                if r.n(1) & 1 == 0 {
                    b"vertical"
                } else {
                    b"horizontal"
                },
            );
        }
        b"clap" => {
            let _ = writeln!(
                out,
                "{indent}clean_aperture: {}/{} x {}/{}",
                r.n(4),
                r.n(4),
                r.n(4),
                r.n(4)
            );
            let _ = writeln!(
                out,
                "{indent}offset: {}/{} ; {}/{}",
                r.n(4) as i32,
                r.n(4),
                r.n(4) as i32,
                r.n(4)
            );
        }
        b"auxC" => {
            line(out, &indent, "aux type: ", r.string());
            let _ = write!(out, "{indent}aux subtypes: ");
            for b in &r.data[r.at..] {
                let _ = write!(out, "{b:02x} ");
            }
            out.push(b'\n');
        }
        b"ftyp" => {
            line(out, &indent, "major brand: ", r.bytes(4));
            let minor = r.n(4);
            if minor < 0x41000000 {
                let _ = writeln!(out, "{indent}minor version: {minor}");
            } else {
                line(
                    out,
                    &indent,
                    "minor version: ",
                    &(minor as u32).to_be_bytes(),
                );
            }
            out.extend_from_slice(indent.as_bytes());
            out.extend_from_slice(b"compatible brands: ");
            let mut first = true;
            while r.at + 4 <= r.data.len() {
                if !first {
                    out.push(b',');
                }
                out.extend_from_slice(r.bytes(4));
                first = false;
            }
            out.push(b'\n');
        }
        b"meta" | b"iprp" | b"ipco" => {
            children(out, &r.data[r.at..], depth + 1, h.kind == *b"ipco", owned)
        }
        b"iinf" => {
            r.n(if version == 0 { 2 } else { 4 });
            children(out, &r.data[r.at..], depth + 1, false, owned);
        }
        b"hdlr" => {
            let _ = writeln!(out, "{indent}pre_defined: {}", r.n(4));
            line(out, &indent, "handler_type: ", r.bytes(4));
            r.bytes(12);
            line(out, &indent, "name: ", r.string());
        }
        b"pitm" => {
            let _ = writeln!(
                out,
                "{indent}item_ID: {}",
                r.n(if version == 0 { 2 } else { 4 })
            );
        }
        b"infe" => {
            let _ = writeln!(
                out,
                "{indent}item_ID: {}",
                r.n(if version >= 3 { 4 } else { 2 })
            );
            let _ = writeln!(out, "{indent}item_protection_index: {}", r.n(2));
            let kind = if version >= 2 { r.bytes(4) } else { b"mime" };
            line(out, &indent, "item_type: ", kind);
            line(out, &indent, "item_name: ", r.string());
            if kind == b"mime" {
                line(out, &indent, "content_type: ", r.string());
                line(out, &indent, "content_encoding: ", r.string());
            }
            if kind == b"uri " {
                line(out, &indent, "item uri type: ", r.string());
            }
            let _ = writeln!(out, "{indent}hidden item: {}", flags & 1 != 0);
        }
        b"iloc" => {
            let widths = r.n(2);
            let offset = ((widths >> 12) & 15) as usize;
            let length = ((widths >> 8) & 15) as usize;
            let base = ((widths >> 4) & 15) as usize;
            let index = if version > 0 {
                (widths & 15) as usize
            } else {
                0
            };
            let count = r.n(if version < 2 { 2 } else { 4 });
            for _ in 0..count.min(r.data.len() as u64) {
                let id = r.n(if version < 2 { 2 } else { 4 });
                let method = if version > 0 { r.n(2) & 15 } else { 0 };
                let reference = r.n(2);
                let b = r.n(base);
                let b = owned
                    .and_then(|c| c.items.locations.get(&(id as u32)))
                    .map_or(b, |loc| loc.base);
                let _ = writeln!(
                    out,
                    "{indent}item ID: {id}\n{indent}  construction method: {method}\n{indent}  data_reference_index: {reference:x}\n{indent}  base_offset: {b}"
                );
                let count = r.n(2);
                let _ = write!(out, "{indent}  extents: ");
                for _ in 0..count.min(r.data.len() as u64) {
                    let i = r.n(index);
                    let a = r.n(offset);
                    let n = r.n(length);
                    let _ = write!(out, "{a},{n}");
                    if i != 0 {
                        let _ = write!(out, ";index={i}");
                    }
                    out.push(b' ');
                }
                out.push(b'\n');
            }
        }
        b"ipma" => {
            let count = r.n(4);
            for _ in 0..count.min(r.data.len() as u64) {
                let id = r.n(if version < 1 { 2 } else { 4 });
                let _ = writeln!(out, "{indent}associations for item ID: {id}");
                let n = r.n(1);
                for _ in 0..n {
                    let large = flags & 1 != 0;
                    let a = r.n(if large { 2 } else { 1 });
                    let mask = if large { 0x8000 } else { 0x80 };
                    let _ = writeln!(
                        out,
                        "{indent}| property index: {} (essential: {})",
                        a & (mask - 1),
                        a & mask != 0
                    );
                }
            }
        }
        b"iref" => {
            let width = if version == 0 { 2 } else { 4 };
            while let Ok(h) = header(&r.data[r.at..]) {
                let Some(end) =
                    r.at.checked_add(h.size as usize)
                        .filter(|n| *n <= r.data.len())
                else {
                    break;
                };
                if h.size < 8 {
                    break;
                }
                r.bytes(h.header);
                out.extend_from_slice(indent.as_bytes());
                out.extend_from_slice(b"reference with type '");
                out.extend_from_slice(&h.kind);
                let _ = write!(out, "' from ID: {} to IDs: ", r.n(width));
                let count = r.n(2);
                for _ in 0..count.min(r.data.len() as u64) {
                    let _ = write!(out, "{} ", r.n(width));
                }
                out.push(b'\n');
                r.at = end;
            }
        }
        b"ispe" => {
            let _ = writeln!(
                out,
                "{indent}image width: {}\n{indent}image height: {}",
                r.n(4),
                r.n(4)
            );
        }
        b"mskC" => {
            let _ = writeln!(out, "{indent}bits_per_pixel: {}", r.n(1));
        }
        b"idat" => {
            let _ = writeln!(
                out,
                "{indent}number of data bytes: {}",
                h.size.saturating_sub(h.header as u64)
            );
        }
        b"pixi" => {
            let n = r.n(1);
            let _ = write!(out, "{indent}bits_per_channel: ");
            for i in 0..n {
                if i > 0 {
                    out.push(b',');
                }
                let _ = write!(out, "{}", r.n(1));
            }
            out.push(b'\n');
        }
        b"pasp" => {
            let _ = writeln!(
                out,
                "{indent}hSpacing: {}\n{indent}vSpacing: {}",
                r.n(4),
                r.n(4)
            );
        }
        _ => {
            for (i, b) in r.data.iter().enumerate() {
                if i % 16 == 0 {
                    let _ = write!(
                        out,
                        "{indent}{}{i:04x}: ",
                        if i == 0 { "data: " } else { "      " }
                    );
                } else {
                    out.extend_from_slice(if i % 16 == 8 { b"  " } else { b" " });
                }
                let _ = write!(out, "{b:02x}");
                if i % 16 == 15 || i + 1 == r.data.len() {
                    out.push(b'\n');
                }
            }
        }
    }
}
impl Context {
    pub fn debug_dump_boxes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(input) = &self.items.input {
            if self.debug_loaded == 0 {
                return out;
            }
            let data = input.bytes();
            let Ok(first) = header(data) else {
                return out;
            };
            if let Some(ftyp) = data.get(..first.size as usize) {
                dump_box(&mut out, ftyp, 0, None);
            }
            if self.debug_loaded == 1 {
                return out;
            }
            if let Some(mini) = input.minimized_diagnostic() {
                out.push(b'\n');
                out.extend_from_slice(mini);
                return out;
            }
            let mut rest = data.get(first.size as usize..).unwrap_or_default();
            let mut meta = None;
            let mut moov = None;
            while let Ok(h) = header(rest) {
                let size = if h.size == 0 {
                    rest.len()
                } else {
                    h.size as usize
                };
                if size < h.header || size > rest.len() {
                    break;
                }
                if h.kind == *b"meta" {
                    meta = Some(&rest[..size]);
                }
                if h.kind == *b"moov" {
                    moov = Some(&rest[..size]);
                }
                rest = &rest[size..];
            }
            for b in [meta, moov].into_iter().flatten() {
                out.push(b'\n');
                dump_box(&mut out, b, 0, None);
            }
        } else {
            let layout = self.items.layout.lock().unwrap();
            header_text(&mut out, "", b"ftyp", 0, 0);
            line(&mut out, "", "major brand: ", &layout.major.to_be_bytes());
            if layout.minor >= 0x20202020 {
                line(&mut out, "", "minor version: ", &layout.minor.to_be_bytes());
            } else {
                let _ = writeln!(out, "minor version: {}", layout.minor);
            }
            out.extend_from_slice(b"compatible brands: ");
            for (i, b) in layout.brands.iter().enumerate() {
                if i != 0 {
                    out.push(b',');
                }
                out.extend_from_slice(&b.to_be_bytes());
            }
            out.push(b'\n');
            if !layout.meta.is_empty() {
                let meta = self.write_meta(&layout, 0, true);
                out.push(b'\n');
                dump_box(&mut out, &meta, 0, Some(self));
            }
        }
        self.debug_dump_item_data(&mut out);
        out
    }
}

fn uuid_text(out: &mut Vec<u8>, bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        if [4, 6, 8, 10].contains(&i) {
            out.push(b'-');
        }
        let _ = write!(out, "{b:02x}");
    }
}

impl Context {
    fn debug_dump_item_data(&self, out: &mut Vec<u8>) {
        let limits = *self.limits.read().unwrap();
        let mut details = Vec::new();
        let has_iref = if let Some(input) = &self.items.input {
            let mut rest = input.bytes();
            let mut found = false;
            while let Ok(h) = header(rest) {
                let size = if h.size == 0 {
                    rest.len()
                } else {
                    h.size as usize
                };
                if size < h.header || size > rest.len() {
                    break;
                }
                if h.kind == *b"meta" {
                    found = rest
                        .get(h.header + 4..size)
                        .and_then(|b| crate::context::children(b).ok())
                        .is_some_and(|b| b.iter().any(|(kind, _)| kind == b"iref"));
                }
                rest = &rest[size..];
            }
            found
        } else {
            self.items.layout.lock().unwrap().meta.contains(b"iref")
        };
        for (&id, item) in &self.items.items {
            let kind = item.kind.to_be_bytes();
            if kind != *b"grid" && kind != *b"iovl" {
                continue;
            }
            let Ok(data) = self.items.item_data(id, limits) else {
                continue;
            };
            if kind == *b"grid" {
                let Ok(grid) = crate::derived::Grid::parse(&data) else {
                    continue;
                };
                let _ = writeln!(
                    details,
                    "\nitem ID {id} (grid):\n  rows: {}\n  columns: {}\n  output width: {}\n  output height: {}",
                    grid.rows, grid.columns, grid.width, grid.height
                );
            } else if has_iref {
                let count = self
                    .items
                    .references
                    .iter()
                    .find(|r| r.from == id && r.kind == u32::from_be_bytes(*b"dimg"))
                    .map_or(0, |r| r.to.len());
                let Ok(overlay) = crate::overlay::Overlay::parse(&data, count) else {
                    continue;
                };
                let [r, g, b, a] = overlay.background;
                let _ = write!(
                    details,
                    "\nitem ID {id} (iovl):\n  version: {}\n  flags: {}\n  background color: {r};{g};{b};{a}\n  canvas size: {}x{}\n  offsets: ",
                    data[0], data[1], overlay.width, overlay.height
                );
                for (x, y) in overlay.offsets {
                    let _ = write!(details, "{x};{y} ");
                }
                details.push(b'\n');
            }
        }
        if !details.is_empty() {
            out.extend_from_slice(b"\n=== Item Data ===\n");
            out.extend(details);
        }
    }
}
