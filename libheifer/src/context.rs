// SPDX-License-Identifier: LGPL-3.0-or-later
// Semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Shared file/handle model. Media extents remain lazy; metadata is owned.
use crate::{
    color::{ColorMetadata, Nclx},
    container::{Container, ParseError},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct ContextError {
    pub code: i32,
    pub subcode: i32,
    pub message: String,
}
impl ContextError {
    pub fn new(code: i32, subcode: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            subcode,
            message: message.into(),
        }
    }
    pub fn invalid(subcode: i32, description: &str) -> Self {
        Self::new(2, subcode, format!("Invalid input: {description}"))
    }
    fn truncated() -> Self {
        Self::invalid(100, "Unexpected end of file")
    }
}
impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ContextError {}
impl From<ParseError> for ContextError {
    fn from(e: ParseError) -> Self {
        match e {
            ParseError::Truncated => Self::truncated(),
            ParseError::Limit => {
                Self::new(6, 1000, "Memory allocation error: Security limit exceeded")
            }
            ParseError::Unsupported => {
                Self::new(4, 3000, "Unsupported feature: Unsupported data version")
            }
            ParseError::EmptyReferences => Self::invalid(
                0,
                "Unspecified: Input file has an 'iref' box with no references.",
            ),
            ParseError::DoubleReferences => {
                Self::invalid(0, "Unspecified: 'iref' has double references")
            }
            ParseError::InvalidSize => Self::invalid(101, "Invalid box size"),
            _ => Self::invalid(0, "Unspecified"),
        }
    }
}
type Result<T> = std::result::Result<T, ContextError>;

/// The owner must keep its immutable bytes valid for the life of the object.
/// Foreign borrowed buffers implement this trait only in the unsafe C adapter.
pub trait Input: Send + Sync {
    fn bytes(&self) -> &[u8];
}
impl Input for Vec<u8> {
    fn bytes(&self) -> &[u8] {
        self
    }
}

#[derive(Clone, Copy)]
struct Header {
    kind: [u8; 4],
    size: u64,
    header: usize,
}
fn header(data: &[u8]) -> Result<Header> {
    let bytes = data.get(..8).ok_or_else(ContextError::truncated)?;
    let mut size = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as u64;
    let kind = bytes[4..8].try_into().unwrap();
    let mut n = 8;
    if size == 1 {
        size = u64::from_be_bytes(
            data.get(8..16)
                .ok_or_else(ContextError::truncated)?
                .try_into()
                .unwrap(),
        );
        n += 8;
        if size > 0x0fff_ffff_ffff_ffff {
            return Err(ContextError::new(
                6,
                1000,
                format!(
                    "Memory allocation error: Security limit exceeded: Box size {size} exceeds security limit."
                ),
            ));
        }
    }
    if kind == *b"uuid" {
        n += 16;
    }
    data.get(..n).ok_or_else(ContextError::truncated)?;
    Ok(Header {
        kind,
        size,
        header: n,
    })
}
fn body(data: &[u8], h: Header) -> Result<&[u8]> {
    let size = if h.size == 0 {
        data.len()
    } else {
        usize::try_from(h.size).map_err(|_| ContextError::truncated())?
    };
    if size < h.header {
        return Err(ContextError::invalid(
            101,
            &format!(
                "Invalid box size: Box size ({size} bytes) smaller than header size ({} bytes)",
                h.header
            ),
        ));
    }
    data.get(h.header..size).ok_or_else(ContextError::truncated)
}
fn children(mut data: &[u8]) -> Result<Vec<([u8; 4], &[u8])>> {
    let mut out = Vec::new();
    while !data.is_empty() {
        if out.len() >= 1000 {
            return Err(ParseError::Limit.into());
        }
        let h = header(data)?;
        let b = body(data, h)?;
        out.push((h.kind, b));
        data = &data[h.header + b.len()..];
    }
    Ok(out)
}
fn required<'a>(boxes: &[([u8; 4], &'a [u8])], kind: [u8; 4], subcode: i32) -> Result<&'a [u8]> {
    boxes
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            ContextError::invalid(
                subcode,
                &format!("No '{}' box", String::from_utf8_lossy(&kind)),
            )
        })
}
fn has_images(boxes: &[([u8; 4], &[u8])]) -> bool {
    boxes
        .iter()
        .find(|(k, _)| k == b"hdlr")
        .is_some_and(|(_, b)| b.get(8..12) == Some(b"pict"))
}
fn validate_property(kind: [u8; 4], p: &[u8]) -> Result<()> {
    match &kind {
        b"ispe" if p.len() < 12 => return Err(ContextError::truncated()),
        b"clap" => {
            crate::geometry::CleanAperture::parse(p)?;
        }
        b"hvcC" => {
            if p.len() < 23 {
                return Err(ContextError::truncated());
            }
            let mut at = 23;
            for _ in 0..p[22] {
                let array = p.get(at..at + 3).ok_or_else(ContextError::truncated)?;
                at += 3;
                for _ in 0..u16::from_be_bytes([array[1], array[2]]) {
                    let len = p.get(at..at + 2).ok_or_else(ContextError::truncated)?;
                    let n = u16::from_be_bytes([len[0], len[1]]) as usize;
                    at += 2;
                    p.get(at..at + n).ok_or_else(ContextError::truncated)?;
                    at += n;
                }
            }
        }
        b"colr" => match p.get(..4) {
            Some(b"nclx") if p.len() >= 11 => {}
            Some(b"nclc") if p.len() >= 10 => {}
            Some(b"nclx" | b"nclc") => return Err(ContextError::truncated()),
            Some(b"prof" | b"rICC") => {}
            _ => return Err(ContextError::invalid(126, "Unknown color profile type")),
        },
        _ => {}
    }
    Ok(())
}

/// Match the layout reader's lazy scanning: media boxes are skipped by size,
/// and a partial trailing header is accepted once metadata has been read.
fn metadata(data: &[u8]) -> Result<&[u8]> {
    if data.len() < 32 {
        return Err(ContextError::invalid(
            0,
            "Unspecified: File size too small.",
        ));
    }
    let first = header(data)?;
    if first.kind != *b"ftyp" {
        return Err(ContextError::invalid(
            102,
            "No 'ftyp' box: File does not start with 'ftyp' box.",
        ));
    }
    if first.size == 0 {
        return Err(ContextError::invalid(
            102,
            "No 'ftyp' box: ftyp box shall not be the only box in the file",
        ));
    }
    if first.size > data.len() as u64 {
        return Err(ContextError::invalid(
            102,
            "No 'ftyp' box: ftyp box larger than initial read range",
        ));
    }
    let ftyp = body(data, first)?;
    if ftyp.len() < 8 {
        return Err(ContextError::invalid(
            101,
            "Invalid box size: ftyp box too small (less than 8 bytes)",
        ));
    }
    if (ftyp.len() - 8) / 4 > 1000 {
        return Err(ContextError::new(
            6,
            1000,
            "Memory allocation error: Security limit exceeded: Number of minor brands in file exceeds security limit",
        ));
    }
    let supported = &ftyp[..4] == b"mif3"
        || ftyp[8..].chunks_exact(4).any(|b| {
            matches!(
                b,
                b"heic"
                    | b"heix"
                    | b"mif1"
                    | b"avif"
                    | b"1pic"
                    | b"jpeg"
                    | b"isom"
                    | b"mp42"
                    | b"mp41"
                    | b"msf1"
            )
        });
    let mut pos = first.size;
    let mut found = None;
    loop {
        if pos.checked_add(32).is_none_or(|n| n > data.len() as u64) {
            if found.is_some() {
                break;
            }
            return Err(ContextError::invalid(
                0,
                "Unspecified: Insufficient input data",
            ));
        }
        let h = header(&data[pos as usize..])?;
        if h.kind == *b"meta" {
            if h.size != 0
                && pos
                    .checked_add(h.size)
                    .is_none_or(|n| n > data.len() as u64)
            {
                return Err(ContextError::invalid(
                    104,
                    "No 'meta' box: Cannot read full meta box",
                ));
            }
            found = Some(body(&data[pos as usize..], h)?);
        } else if matches!(&h.kind, b"mini" | b"moov") {
            return Err(ParseError::Unsupported.into());
        }
        if h.size == 0 {
            if found.is_some() {
                break;
            }
            return Err(ContextError::invalid(0, "Unspecified: No meta box found"));
        }
        pos = pos
            .checked_add(h.size)
            .ok_or_else(ContextError::truncated)?;
    }
    if !supported {
        return Err(ContextError::new(
            3,
            0,
            "Unsupported file-type: Unspecified: File does not include any supported brands.\n",
        ));
    }
    let meta = found.unwrap();
    if meta.len() < 4 {
        return Err(ContextError::truncated());
    }
    let boxes = children(&meta[4..])?;
    required(&boxes, *b"iinf", 111)?;
    if has_images(&boxes) {
        required(&boxes, *b"pitm", 107)?;
        let props = children(required(&boxes, *b"iprp", 112)?)?;
        for (kind, p) in children(required(&props, *b"ipco", 108)?)? {
            validate_property(kind, p)?;
        }
        required(&props, *b"ipma", 109)?;
    }
    required(&boxes, *b"iloc", 110)?;
    Ok(meta)
}

pub struct Metadata {
    pub id: u32,
    pub kind: CString,
    pub content_type: CString,
    pub uri_type: CString,
    pub data: Vec<u8>,
}
pub struct ImageInfo {
    pub id: u32,
    pub primary: bool,
    pub width: u32,
    pub height: u32,
    pub ispe: (u32, u32),
    pub luma_bits: i32,
    pub chroma_bits: i32,
    pub colorspace: i32,
    pub chroma: i32,
    pub has_alpha: bool,
    pub premultiplied_alpha: bool,
    pub pixel_aspect: Option<(u32, u32)>,
    pub color: ColorMetadata,
    pub thumbnails: Vec<u32>,
    pub metadata: Vec<Arc<Metadata>>,
    pub error: Option<ContextError>,
}
pub struct Document {
    pub input: Arc<dyn Input>,
    pub images: BTreeMap<u32, ImageInfo>,
    pub primary: u32,
    pub top_level: Vec<u32>,
}
impl Document {
    pub fn container(&self) -> Result<Container<'_>> {
        Ok(Container::parse_meta(
            self.input.bytes(),
            metadata(self.input.bytes())?,
        )?)
    }
    pub fn parse(input: Arc<dyn Input>) -> Result<Self> {
        let mut context = Context::default();
        context.read(input)?;
        Ok(Arc::try_unwrap(context.document.unwrap()).ok().unwrap())
    }
    fn interpret(&mut self, container: &Container<'_>) -> Result<()> {
        let images = &mut self.images;
        for item in container.items.values() {
            if !matches!(
                &item.kind,
                b"hvc1"
                    | b"av01"
                    | b"grid"
                    | b"iovl"
                    | b"iden"
                    | b"unci"
                    | b"jpeg"
                    | b"j2k1"
                    | b"vvc1"
                    | b"avc1"
            ) {
                continue;
            }
            let ispe = container.dimensions(item.id).unwrap_or((0, 0));
            let mut image = ImageInfo {
                id: item.id,
                primary: item.id == container.primary && !item.hidden,
                width: 0,
                height: 0,
                ispe,
                luma_bits: -1,
                chroma_bits: -1,
                colorspace: 99,
                chroma: 99,
                has_alpha: false,
                premultiplied_alpha: item.references.contains_key(b"prem"),
                pixel_aspect: None,
                color: ColorMetadata::default(),
                thumbnails: Vec::new(),
                metadata: Vec::new(),
                error: None,
            };
            if item.kind == *b"hvc1" {
                if let Ok(config) = container.property(item.id, *b"hvcC") {
                    if config.len() < 23 {
                        return Err(ContextError::truncated());
                    }
                    image.luma_bits = 8 + i32::from(config[17] & 7);
                    image.chroma_bits = 8 + i32::from(config[18] & 7);
                    image.chroma = i32::from(config[16] & 3);
                    image.colorspace = if image.chroma == 0 { 2 } else { 0 };
                } else {
                    image.error = Some(ContextError::invalid(106, "No 'hvcC' box"));
                }
            } else if item.kind == *b"grid" {
                image.error = crate::derived::Grid::load(container, item.id).err();
            } else if item.kind == *b"iden" {
                // Identity images are resolved when decoded or queried.
            } else {
                // The file and its image ID are still visible, but this initial
                // implementation cannot yet construct other codec/derived handles.
                image.error = Some(ContextError::new(
                    4,
                    3003,
                    "Unsupported feature: Unsupported image type",
                ));
            }
            images.insert(item.id, image);
            if !item.hidden {
                self.top_level.push(item.id);
                if item.id == container.primary {
                    self.primary = item.id;
                }
            }
        }
        if !images.get(&container.primary).is_some_and(|i| i.primary) {
            return Err(ContextError::invalid(
                2000,
                "Non-existing item ID referenced: 'pitm' box references an unsupported or non-existing image",
            ));
        }
        for item in container.items.values() {
            let Some(image) = images.get_mut(&item.id) else {
                continue;
            };
            if image.error.is_some() {
                continue;
            }
            if container
                .properties(item.id)?
                .any(|(kind, p)| (kind == *b"irot" || kind == *b"imir") && p.is_empty())
            {
                return Err(ContextError::truncated());
            }
            if container.property(item.id, *b"ispe").is_ok() {
                if image.ispe.0 == 0 || image.ispe.1 == 0 {
                    return Err(ContextError::invalid(
                        129,
                        "Invalid image size: Zero image width or height",
                    ));
                }
                (image.width, image.height) = image.ispe;
            }
            for (kind, p) in container.properties(item.id)? {
                match &kind {
                    b"irot" => {
                        if p.first().is_some_and(|n| n & 1 != 0) {
                            if container.property(item.id, *b"ispe").is_err() {
                                return Err(ContextError::invalid(
                                    137,
                                    "Image has no 'ispe' property",
                                ));
                            }
                            std::mem::swap(&mut image.width, &mut image.height);
                        }
                    }
                    b"clap" => {
                        (image.width, image.height) =
                            crate::geometry::CleanAperture::parse(p)?.dimensions();
                        if image.width == 0 || image.height == 0 {
                            return Err(ContextError::invalid(
                                120,
                                "Invalid clean-aperture specification: Clean aperture (clap) reduces image to zero size",
                            ));
                        }
                    }
                    b"pasp" if p.len() >= 8 => {
                        image.pixel_aspect = Some((
                            u32::from_be_bytes(p[..4].try_into().unwrap()),
                            u32::from_be_bytes(p[4..8].try_into().unwrap()),
                        ));
                    }
                    b"colr" if p.len() >= 4 => {
                        if matches!(&p[..4], b"nclx" | b"nclc") && p.len() >= 10 {
                            image.color.nclx = Some(Nclx {
                                primaries: u16::from_be_bytes(p[4..6].try_into().unwrap()),
                                transfer: u16::from_be_bytes(p[6..8].try_into().unwrap()),
                                matrix: u16::from_be_bytes(p[8..10].try_into().unwrap()),
                                full_range: if &p[..4] == b"nclc" {
                                    p[8..10] == [0, 0]
                                } else {
                                    p[10] & 128 != 0
                                },
                            });
                        } else if matches!(&p[..4], b"prof" | b"rICC") {
                            image
                                .color
                                .set_raw(u32::from_be_bytes(p[..4].try_into().unwrap()), &p[4..])
                                .map_err(|_| ParseError::Limit)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut thumbnails = BTreeSet::new();
        for item in container.items.values() {
            if !images.contains_key(&item.id) {
                continue;
            }
            for (kind, targets) in &item.references {
                for target in targets {
                    if kind == b"thmb" {
                        thumbnails.insert(item.id);
                        let Some(master) = images.get_mut(target) else {
                            return Err(ContextError::invalid(
                                2000,
                                "Non-existing item ID referenced: Thumbnail references a non-existing image",
                            ));
                        };
                        if thumbnails.contains(target) {
                            return Err(ContextError::invalid(
                                2000,
                                "Non-existing item ID referenced: Thumbnail references another thumbnail",
                            ));
                        }
                        master.thumbnails.push(item.id);
                        self.top_level.retain(|id| *id != item.id);
                    } else if kind == b"auxl" {
                        let p=container.property(item.id,*b"auxC").map_err(|_|ContextError::invalid(123,&format!("Type of auxiliary image unspecified: No auxC property for image {}",item.id)))?;
                        let aux = p
                            .get(4..)
                            .and_then(|p| p.split(|b| *b == 0).next())
                            .unwrap_or_default();
                        if matches!(
                            aux,
                            b"urn:mpeg:avc:2015:auxid:1"
                                | b"urn:mpeg:hevc:2015:auxid:1"
                                | b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha"
                        ) {
                            if let Some(master) = images.get_mut(target) {
                                if item.id == *target {
                                    return Err(ContextError::invalid(
                                        2000,
                                        "Non-existing item ID referenced: Recursive alpha image detected",
                                    ));
                                }
                                master.has_alpha = true;
                            } else if !container.items.contains_key(target) {
                                return Err(ContextError::invalid(
                                    2000,
                                    "Non-existing item ID referenced: Non-existing alpha image referenced",
                                ));
                            }
                        }
                        if images.contains_key(target) {
                            if item.id == *target {
                                return Err(ContextError::invalid(
                                    2000,
                                    "Non-existing item ID referenced: Recursive aux image detected",
                                ));
                            }
                            self.top_level.retain(|id| *id != item.id);
                        } else if !container.items.contains_key(target) {
                            return Err(ContextError::invalid(
                                2000,
                                "Non-existing item ID referenced: Non-existing aux image referenced",
                            ));
                        }
                    }
                }
            }
        }
        // Derived descriptions follow the first coded descendant; cycles and
        // unavailable children retain the reference's unknown-query values.
        for item in container
            .items
            .values()
            .filter(|i| matches!(&i.kind, b"grid" | b"iden"))
        {
            let mut id = item.id;
            let mut visited = BTreeSet::new();
            let description = loop {
                if !visited.insert(id) {
                    break None;
                }
                let Some(child) = container.items.get(&id) else {
                    break None;
                };
                if matches!(&child.kind, b"grid" | b"iden" | b"iovl") {
                    let Some(next) = child.references.get(b"dimg").and_then(|r| r.first()) else {
                        break None;
                    };
                    id = *next;
                } else {
                    break images
                        .get(&id)
                        .map(|i| (i.luma_bits, i.chroma_bits, i.colorspace, i.chroma));
                }
            };
            if let Some((l, c, cs, ch)) = description {
                if let Some(i) = images.get_mut(&item.id) {
                    i.luma_bits = l;
                    i.chroma_bits = c;
                    i.colorspace = cs;
                    i.chroma = ch;
                }
            }
            if item.kind == *b"grid" {
                let child = item
                    .references
                    .get(b"dimg")
                    .and_then(|r| r.first())
                    .and_then(|id| images.get(id));
                let color = child
                    .map(|i| i.color.try_clone())
                    .transpose()
                    .map_err(|_| ParseError::Limit)?;
                let alpha = item.references.get(b"dimg").is_some_and(|r| {
                    r.iter()
                        .any(|id| images.get(id).is_some_and(|i| i.has_alpha))
                });
                if let Some(i) = images.get_mut(&item.id) {
                    i.has_alpha |= alpha;
                    if let Some(color) = color {
                        if i.color.nclx.is_none() {
                            i.color.nclx = color.nclx;
                        }
                        if i.color.raw.is_none() {
                            i.color.raw = color.raw;
                        }
                    }
                }
            }
        }
        for item in container.items.values() {
            if images.contains_key(&item.id)
                || item.kind == *b"rgan"
                || !item.references.contains_key(b"cdsc")
            {
                continue;
            }
            if !item.content_encoding.is_empty() {
                continue;
            } // compressed metadata remains an explicit coverage gap
            let data = container.payload(item.id)?;
            let metadata = Arc::new(Metadata {
                id: item.id,
                kind: CString::new(item.kind.to_vec()).map_err(|_| ParseError::InvalidField)?,
                content_type: CString::new(item.content_type.clone()).unwrap(),
                uri_type: CString::new(item.uri_type.clone()).unwrap(),
                data,
            });
            for target in &item.references[b"cdsc"] {
                if let Some(image) = images.get_mut(target) {
                    image.metadata.push(metadata.clone());
                } else if !container.items.contains_key(target) {
                    return Err(ContextError::invalid(
                        2000,
                        "Non-existing item ID referenced: Metadata assigned to non-existing image",
                    ));
                }
            }
        }
        Ok(())
    }
}
pub struct Context {
    pub max_decoding_threads: i32,
    pub document: Option<Arc<Document>>,
    pub last_error: CString,
}
impl Default for Context {
    fn default() -> Self {
        Self {
            document: None,
            last_error: CString::default(),
            max_decoding_threads: 4,
        }
    }
}
impl Context {
    pub fn read(&mut self, input: Arc<dyn Input>) -> Result<()> {
        // File parsing failures preserve the previous image model. Once image
        // interpretation begins, even a failed load exposes its partial model.
        let meta = metadata(input.bytes())?;
        if !has_images(&children(&meta[4..])?) {
            if self.document.is_none() {
                self.document = Some(Arc::new(Document {
                    input,
                    images: BTreeMap::new(),
                    primary: 0,
                    top_level: Vec::new(),
                }));
            }
            return Ok(());
        }
        let container = Container::parse_meta(input.bytes(), meta)?;
        let mut document = Document {
            input: input.clone(),
            images: BTreeMap::new(),
            primary: 0,
            top_level: Vec::new(),
        };
        let outcome = document.interpret(&container);
        self.document = Some(Arc::new(document));
        outcome
    }
}
