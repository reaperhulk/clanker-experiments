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
    sync::{Arc, RwLock},
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
            ParseError::MissingItemProperties(id) => Self::invalid(
                116,
                &format!(
                    "No properties assigned to item: Item (ID={id}) has no properties assigned to it in ipma box"
                ),
            ),
            ParseError::InvalidPropertyIndex { item, index } => Self::invalid(
                115,
                &format!(
                    "'ipma' box references a non-existing property: Nonexisting property (index={index}) for item  ID={item} referenced in ipma box"
                ),
            ),
            ParseError::UnsupportedBoxVersion { kind, version } => Self::new(
                4,
                3002,
                format!(
                    "Unsupported feature: Unsupported data version: {} box data version {version} is not implemented yet",
                    String::from_utf8_lossy(&kind)
                ),
            ),
            ParseError::Security { invalid, message } => Self::new(
                if invalid { 2 } else { 6 },
                1000,
                format!(
                    "{}: Security limit exceeded: {message}",
                    if invalid {
                        "Invalid input"
                    } else {
                        "Memory allocation error"
                    }
                ),
            ),
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
pub(crate) fn validate_property(kind: [u8; 4], p: &[u8]) -> Result<()> {
    if matches!(&kind, b"taic" | b"itai") {
        crate::tai::TaiProperty::parse(kind, p)?;
    }
    match &kind {
        b"auxC" => {
            if p.len() < 4 {
                return Err(ContextError::truncated());
            }
            if p[0] != 0 {
                return Err(ContextError::new(
                    4,
                    3002,
                    format!(
                        "Unsupported feature: Unsupported data version: {} box data version {} is not implemented yet",
                        String::from_utf8_lossy(&kind),
                        p[0]
                    ),
                ));
            }
        }
        b"mskC" if p.len() < 5 => return Err(ContextError::truncated()),
        b"ispe" if p.len() < 12 => return Err(ContextError::truncated()),
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

fn validate_limit_boxes(
    mut data: &[u8],
    parent: [u8; 4],
    limits: &crate::security::Limits,
) -> Result<()> {
    let mut count = 0usize;
    while !data.is_empty() {
        let h = header(data)?;
        let p = body(data, h)?;
        match &h.kind {
            b"iprp" | b"ipco" => validate_limit_boxes(p, h.kind, limits)?,
            b"iinf" if p.len() >= 6 => {
                let n = if p[0] == 0 {
                    u32::from(u16::from_be_bytes([p[4], p[5]]))
                } else if p.len() >= 8 {
                    u32::from_be_bytes(p[4..8].try_into().unwrap())
                } else {
                    0
                };
                if limits.max_items != 0 && n > limits.max_items {
                    return Err(ParseError::Security { invalid: false, message: format!("iinf box contains {n} items, which exceeds the security limit of {} items.", limits.max_items) }.into());
                }
            }
            b"colr" if matches!(p.get(..4), Some(b"prof" | b"rICC")) => {
                if limits.max_color_profile_size != 0
                    && p.len() - 4 > limits.max_color_profile_size as usize
                {
                    return Err(ContextError::invalid(
                        1000,
                        "Security limit exceeded: Color profile exceeds maximum supported size",
                    ));
                }
            }
            _ => {}
        }
        if limits.max_children_per_box != 0 && count > limits.max_children_per_box as usize {
            return Err(ParseError::Security {
                invalid: false,
                message: format!(
                    "Maximum number of child boxes ({}) in '{}' box exceeded.",
                    limits.max_children_per_box,
                    String::from_utf8_lossy(&parent)
                ),
            }
            .into());
        }
        count += 1;
        data = &data[h.header + p.len()..];
    }
    Ok(())
}

/// Match the layout reader's lazy scanning: media boxes are skipped by size,
/// and a partial trailing header is accepted once metadata has been read.
fn metadata<'a>(
    data: &'a [u8],
    limits: &crate::security::Limits,
    properties: Option<&mut crate::properties::PropertyStore>,
    mut items: Option<&mut crate::items::ItemStore>,
) -> Result<&'a [u8]> {
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
    if limits.max_number_of_file_brands != 0
        && (ftyp.len() - 8) / 4 > limits.max_number_of_file_brands as usize
    {
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
    validate_limit_boxes(&meta[4..], *b"meta", limits)?;
    let boxes = children(&meta[4..])?;
    // Parse all table bodies before installing any mandatory-box pointers.
    let mut parsed_items = crate::items::ItemStore::parse_tables(&boxes, *limits)?;
    // Box parsing precedes assignment of the file's mandatory-box pointers.
    // Preserve those assignment stages so property queries after failed reads
    // expose exactly the portion of the new file that was installed.
    let mut parsed = crate::properties::PropertyStore {
        read_only: true,
        ..Default::default()
    };
    if let Some((_, iprp)) = boxes.iter().find(|(k, _)| k == b"iprp") {
        let props = children(iprp)?;
        if let Some((_, ipco)) = props.iter().find(|(k, _)| k == b"ipco") {
            for (kind, p) in children(ipco)? {
                validate_property(kind, p)?;
            }
            if properties.is_some() {
                parsed.boxes = crate::container::property_boxes(ipco)?;
            }
            parsed.has_ipco = true;
        }
        for (_, ipma) in props.iter().filter(|(k, _)| k == b"ipma") {
            for (id, (indices, essential)) in crate::container::parse_associations(ipma, *limits)? {
                if let std::collections::btree_map::Entry::Vacant(entry) = parsed.items.entry(id) {
                    entry.insert(indices);
                    parsed.essential.insert(id, essential);
                }
            }
            parsed.has_ipma = true;
        }
    }
    required(&boxes, *b"iinf", 111)?;
    if let Some(items) = items.as_deref_mut() {
        items.items = std::mem::take(&mut parsed_items.items);
    }
    if has_images(&boxes) {
        required(&boxes, *b"pitm", 107)?;
        let props = children(required(&boxes, *b"iprp", 112)?)?;
        required(&props, *b"ipco", 108)?;
        if let Some(properties) = properties {
            *properties = parsed;
        }
        required(&props, *b"ipma", 109)?;
    }
    required(&boxes, *b"iloc", 110)?;
    if let Some(items) = items {
        items.install_data(parsed_items);
        if let Some((_, idat)) = boxes.iter().find(|(kind, _)| kind == b"idat") {
            items.set_idat(idat);
        }
        items.seed();
    }
    Ok(meta)
}

pub struct Metadata {
    _reservation: crate::security::Reservation,
    pub id: u32,
    pub kind: CString,
    pub content_type: CString,
    pub uri_type: CString,
    pub data: Vec<u8>,
}
pub(crate) struct DecoderInput {
    pub data: Arc<Vec<u8>>,
    pub _reservation: crate::security::Reservation,
}
pub struct ImageInfo {
    pub retained_properties: std::sync::Mutex<Vec<Arc<crate::properties::Property>>>,
    pub text_ids: std::sync::Mutex<Vec<u32>>,
    pub tai_timestamp: Option<crate::tai::Timestamp>,
    pub description_error: Option<ContextError>,
    description_input: Option<DecoderInput>,
    pub components: crate::components::ComponentIds,
    pub intrinsic: Option<crate::camera::IntrinsicMatrix>,
    pub extrinsic: Option<crate::camera::ExtrinsicMatrix>,
    pub warnings: Vec<crate::image::DecodingWarning>,
    pub auxiliary: crate::auxiliary::Auxiliary,
    pub last_error: std::sync::Mutex<CString>,
    pub(crate) decode_mutex: std::sync::Mutex<()>,
    pub related_images: Vec<u32>,
    #[cfg(feature = "hevc")]
    pub(crate) decoder_input: std::sync::Mutex<Option<DecoderInput>>,
    pub kind: [u8; 4],
    pub grid: Option<crate::derived::Grid>,
    pub overlay: Option<crate::overlay::Overlay>,
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
    pub budget: Arc<crate::security::Budget>,
    read_limits: crate::security::Limits,
    pub limits: Arc<RwLock<crate::security::Limits>>,
    pub input: Arc<dyn Input>,
    pub images: BTreeMap<u32, Arc<ImageInfo>>,
    pub primary: u32,
    pub top_level: Vec<u32>,
}
impl Document {
    pub fn current_limits(&self) -> crate::security::Limits {
        *self
            .limits
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Resolve descriptions at query time, including after a partial context reload.
    pub fn first_coded_image(&self, mut id: u32) -> Result<&ImageInfo> {
        let container = self.container()?;
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(id) {
                return Err(ContextError::invalid(
                    117,
                    "Item has no data: Derived image references form a cycle",
                ));
            }
            if let Some(item) = container.items.get(&id)
                && matches!(&item.kind, b"grid" | b"iden" | b"iovl")
            {
                id = *item.references.get(b"dimg").and_then(|ids| ids.first()).ok_or_else(|| {
                    ContextError::invalid(117, "Item has no data: Derived image does not reference any other image items")
                })?;
            } else {
                return self.images.get(&id).map(AsRef::as_ref).ok_or_else(|| ContextError::invalid(
                    2000, &format!("Non-existing item ID referenced: Image item {id} referenced, but it does not exist\n"),
                ));
            }
        }
    }

    pub fn preferred_colorspace(&self, image: &ImageInfo) -> Result<(i32, i32)> {
        let mut color = &image.color;
        let coded = match &image.kind {
            b"iovl" => return Ok((1, 3)),
            b"iden" => {
                let child = self.first_coded_image(image.id)?;
                color = &child.color;
                child
            }
            b"grid" => {
                let child = self.first_coded_image(image.id)?;
                if let Some(error) = &child.error {
                    return Err(error.clone());
                }
                child
            }
            _ => image,
        };
        if coded.kind == *b"mski" || coded.error.is_some() {
            return Err(ContextError::new(
                4,
                6003,
                "Unsupported feature: Support for this compression format has not been built in: No decoder for this image format",
            ));
        }
        if let Some(error) = &coded.description_error {
            return Err(error.clone());
        }
        Ok(
            if coded.colorspace == 0 && color.nclx.is_some_and(|p| p.matrix == 0) {
                (1, 3)
            } else {
                (coded.colorspace, coded.chroma)
            },
        )
    }

    pub fn container(&self) -> Result<Container<'_>> {
        let mut container = Container::parse_meta_with_limits(
            self.input.bytes(),
            metadata(self.input.bytes(), &self.read_limits, None, None)?,
            self.read_limits,
        )?;
        container.limits = self.current_limits();
        Ok(container)
    }
    pub fn parse(input: Arc<dyn Input>) -> Result<Self> {
        let mut context = Context::default();
        context.read(input)?;
        Ok(Arc::try_unwrap(context.document.unwrap()).ok().unwrap())
    }
    fn interpret(
        &mut self,
        container: &Container<'_>,
        items: &crate::items::ItemStore,
        properties: &crate::properties::PropertyStore,
        text_items: &mut Vec<Arc<crate::text::TextItem>>,
    ) -> Result<()> {
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
                    | b"mski"
            ) {
                continue;
            }
            let ispe = container.dimensions(item.id).unwrap_or((0, 0));
            let mut image = ImageInfo {
                retained_properties: std::sync::Mutex::new(
                    properties
                        .items
                        .get(&item.id)
                        .into_iter()
                        .flatten()
                        .filter_map(|index| properties.boxes.get(*index).cloned())
                        .collect(),
                ),
                description_error: None,
                description_input: None,
                components: crate::components::ComponentIds::default(),
                intrinsic: None,
                text_ids: std::sync::Mutex::new(Vec::new()),
                tai_timestamp: None,
                extrinsic: None,
                warnings: Vec::new(),
                auxiliary: crate::auxiliary::Auxiliary::default(),
                last_error: std::sync::Mutex::new(CString::new("Success").unwrap()),
                decode_mutex: std::sync::Mutex::new(()),
                related_images: Vec::new(),
                #[cfg(feature = "hevc")]
                decoder_input: std::sync::Mutex::new(None),
                kind: item.kind,
                grid: None,
                overlay: None,
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
            if let Err(e) = container.properties(item.id) {
                image.error = Some(e.into());
            } else if item.kind == *b"hvc1" {
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
            } else if item.kind == *b"mski" {
                image.luma_bits = container
                    .property(item.id, *b"mskC")
                    .ok()
                    .and_then(|p| p.get(4))
                    .map_or(-1, |v| i32::from(*v));
                image.chroma_bits = 0;
            } else if item.kind == *b"grid" {
                match crate::derived::Grid::load(container, item.id) {
                    Ok(grid) => image.grid = Some(grid),
                    Err(error) => image.error = Some(error),
                }
            } else if item.kind == *b"iovl" {
                match crate::overlay::Overlay::load(container, item.id) {
                    Ok(overlay) => image.overlay = Some(overlay),
                    Err(error) => image.error = Some(error),
                }
                image.colorspace = 1;
                image.chroma = 3;
            } else if item.kind == *b"jpeg" {
                // Configuration is optional: SOF may span jpgC and item data.
                // Keep decoder-input accounting alive for the image lifetime.
                let description = (|| -> Result<crate::jpeg_config::Description> {
                    let data = container.payload(item.id)?;
                    let reservation = self
                        .budget
                        .reserve(data.len() as u64, "decoder input buffer (iloc)")?;
                    image.description_input = Some(DecoderInput {
                        data: Arc::new(data),
                        _reservation: reservation,
                    });
                    let payload = &image.description_input.as_ref().unwrap().data;
                    let config = container.property(item.id, *b"jpgC").unwrap_or_default();
                    let mut combined = Vec::new();
                    combined
                        .try_reserve_exact(config.len().saturating_add(payload.len()))
                        .map_err(|_| crate::error::Error::ALLOCATION)?;
                    combined.extend_from_slice(config);
                    combined.extend_from_slice(payload);
                    crate::jpeg_config::parse(&combined)
                })();
                match description {
                    Ok(d) => {
                        image.colorspace = d.colorspace;
                        image.chroma = d.chroma;
                        image.luma_bits = i32::from(d.precision);
                        image.chroma_bits = i32::from(d.precision);
                    }
                    Err(e) => image.description_error = Some(e),
                }
            } else if item.kind == *b"iden" {
                // Identity items have no decoder of their own.
            } else {
                // The file and its image ID are still visible, but this initial
                // implementation cannot yet construct other codec/derived handles.
                image.error = Some(ContextError::new(
                    4,
                    3003,
                    "Unsupported feature: Unsupported image type",
                ));
            }
            // The reference populates descriptions during its first item pass,
            // before interpreted dimensions and auxiliary links are installed.
            // A derived item's coded descendant must already have been loaded.
            if image.error.is_none() {
                let coded = if item.kind == *b"grid" {
                    let mut next = item.id;
                    let mut visited = BTreeSet::new();
                    loop {
                        if !visited.insert(next) {
                            break None;
                        }
                        let Some(node) = container.items.get(&next) else {
                            break None;
                        };
                        if matches!(&node.kind, b"grid" | b"iden" | b"iovl") {
                            let Some(child) = node.references.get(b"dimg").and_then(|r| r.first())
                            else {
                                break None;
                            };
                            next = *child;
                        } else {
                            break images.get(&next).filter(|i| i.error.is_none());
                        }
                    }
                } else {
                    None
                };
                let description = match &item.kind {
                    b"hvc1" | b"jpeg" => Some((
                        image.colorspace,
                        image.chroma,
                        image.luma_bits,
                        image.chroma_bits,
                    )),
                    b"iovl" if ispe.0 != 0 && ispe.1 != 0 => Some((1, 3, 8, 8)),
                    b"grid" => coded.map(|c| (c.colorspace, c.chroma, c.luma_bits, c.chroma_bits)),
                    _ => None,
                };
                if let Some((cs, ch, l, c)) = description {
                    image.components =
                        crate::components::ComponentIds::visual(ispe.0, ispe.1, cs, ch, l, c)?;
                }
            }
            images.insert(item.id, Arc::new(image));
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
            let Some(image) = images.get_mut(&item.id).and_then(Arc::get_mut) else {
                continue;
            };
            if image.error.is_some() {
                continue;
            }
            if let Some(kind) = container.unknown_essential_property(item.id)? {
                return Err(ContextError::new(
                    4,
                    3007,
                    format!(
                        "Unsupported feature: Unsupported essential item property: could not parse item property '{kind}'"
                    ),
                ));
            }
            for (kind, p) in container.properties(item.id)? {
                if let Some((error, optional)) = crate::properties::parse_error(kind, p) {
                    if optional {
                        image.warnings.push(error.into());
                    } else {
                        return Err(error);
                    }
                }
            }
            let mut has_ispe = false;
            for (kind, p) in container.properties(item.id)? {
                if kind != *b"ispe" {
                    continue;
                }
                let width = u32::from_be_bytes(p[4..8].try_into().unwrap());
                let height = u32::from_be_bytes(p[8..12].try_into().unwrap());
                if width == 0 || height == 0 {
                    return Err(ContextError::invalid(
                        129,
                        "Invalid image size: Zero image width or height",
                    ));
                }
                (image.width, image.height) = (width, height);
                has_ispe = true;
            }
            if !has_ispe {
                image
                    .warnings
                    .push(ContextError::invalid(137, "Image has no 'ispe' property").into());
            }
            for (kind, p) in container.properties(item.id)? {
                match &kind {
                    b"itai" if image.tai_timestamp.is_none() => {
                        if let Ok(crate::tai::TaiProperty::Timestamp(value)) =
                            crate::tai::TaiProperty::parse(kind, p)
                        {
                            image.tai_timestamp = Some(value);
                        }
                    }
                    b"cmin" => {
                        if let Ok(matrix) = crate::camera::intrinsic(p, image.ispe.0, image.ispe.1)
                        {
                            if !has_ispe {
                                return Err(ContextError::invalid(
                                    137,
                                    "Image has no 'ispe' property",
                                ));
                            }
                            image.intrinsic = Some(matrix);
                        }
                    }
                    b"cmex" => {
                        if let Ok(matrix) = crate::camera::ExtrinsicMatrix::parse(p) {
                            image.extrinsic = Some(matrix);
                        }
                    }
                    _ => {}
                }
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
                    b"imir" => {
                        if !has_ispe {
                            return Err(ContextError::invalid(137, "Image has no 'ispe' property"));
                        }
                        if let Some(matrix) = &mut image.intrinsic {
                            matrix.mirror(p[0] & 1 != 0, image.width, image.height);
                        }
                    }
                    b"clap" => {
                        let aperture = crate::geometry::CleanAperture::parse(p)?;
                        (image.width, image.height) = aperture.dimensions();
                        if image.width == 0 || image.height == 0 {
                            return Err(ContextError::invalid(
                                120,
                                "Invalid clean-aperture specification: Clean aperture (clap) reduces image to zero size",
                            ));
                        }
                        if let Some(matrix) = &mut image.intrinsic {
                            let (x, y) = aperture.camera_crop_offset(image.width, image.height)?;
                            matrix.principal_point_x -= x;
                            matrix.principal_point_y -= y;
                        }
                    }
                    b"pasp" if p.len() >= 8 && image.pixel_aspect.is_none() => {
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
                if kind == b"auxl" {
                    attach_auxiliary(images, &mut self.top_level, container, item.id, targets)?;
                    continue;
                }
                for target in targets {
                    if kind == b"thmb" {
                        thumbnails.insert(item.id);
                        let Some(master) = images.get_mut(target).and_then(Arc::get_mut) else {
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
                        master.related_images.push(item.id);
                        self.top_level.retain(|id| *id != item.id);
                    }
                }
            }
        }
        // Derived descriptions follow the first coded descendant; cycles and
        // unavailable children retain the reference's unknown-query values.
        for item in container
            .items
            .values()
            .filter(|i| matches!(&i.kind, b"grid" | b"iden" | b"iovl"))
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
            if let Some((l, c, cs, ch)) = description
                && let Some(i) = images.get_mut(&item.id).and_then(Arc::get_mut)
            {
                i.luma_bits = l;
                i.chroma_bits = c;
                if item.kind != *b"iovl" {
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
                if let Some(i) = images.get_mut(&item.id).and_then(Arc::get_mut) {
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
            if images.contains_key(&item.id) || item.kind == *b"rgan" {
                continue;
            }
            let method = items
                .items
                .get(&item.id)
                .map_or(0, crate::items::Item::compression);
            if !matches!(method, 0 | 3 | 4) {
                continue;
            }
            let data = match items
                .item_data(item.id, self.read_limits)
                .and_then(|data| crate::compression::decompress(data, method, &self.budget))
            {
                Ok(data) => data,
                Err(error) if matches!(&item.kind, b"Exif" | b"mime") => return Err(error),
                Err(_) => continue,
            };
            let metadata = Arc::new(Metadata {
                _reservation: self
                    .budget
                    .reserve(data.len() as u64, "decompressed item metadata")?,
                id: item.id,
                kind: CString::new(item.kind.to_vec()).map_err(|_| ParseError::InvalidField)?,
                content_type: CString::new(item.content_type.clone()).unwrap(),
                uri_type: CString::new(item.uri_type.clone()).unwrap(),
                data,
            });
            for target in item.references.get(b"cdsc").into_iter().flatten() {
                if let Some(image) = images.get_mut(target).and_then(Arc::get_mut) {
                    image.metadata.push(metadata.clone());
                } else if !container.items.contains_key(target) {
                    return Err(ContextError::invalid(
                        2000,
                        "Non-existing item ID referenced: Metadata assigned to non-existing image",
                    ));
                }
            }
        }
        // The reference retains the context's text registry across reads, and
        // adds one registry entry per target in each ordered text reference.
        for (&id, item) in &items.items {
            if item.kind != u32::from_be_bytes(*b"mime") || !matches!(item.compression(), 0 | 3 | 4)
            {
                continue;
            }
            let content = crate::compression::decompress(
                items.item_data(id, self.read_limits)?,
                item.compression(),
                &self.budget,
            )?;
            let text = Arc::new(crate::text::TextItem { id, content });
            for reference in items
                .references
                .iter()
                .filter(|r| r.from == id && r.kind == u32::from_be_bytes(*b"text"))
            {
                for target in &reference.to {
                    let Some(image) = images.get(target) else {
                        return Err(ContextError::invalid(
                            2000,
                            "Non-existing item ID referenced: Text item assigned to non-existing image",
                        ));
                    };
                    image
                        .text_ids
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(id);
                    text_items.push(text.clone());
                }
            }
        }
        Ok(())
    }
}
pub struct Context {
    pub text_items: Vec<Arc<crate::text::TextItem>>,
    pub items: crate::items::ItemStore,
    pub properties: crate::properties::PropertyStore,
    pub budget: Arc<crate::security::Budget>,
    pub limits: Arc<RwLock<crate::security::Limits>>,
    pub max_decoding_threads: i32,
    pub document: Option<Arc<Document>>,
    pub last_error: CString,
}
impl Default for Context {
    fn default() -> Self {
        let limits = Arc::new(RwLock::new(crate::security::Limits::default()));
        Self {
            text_items: Vec::new(),
            items: crate::items::ItemStore::default(),
            properties: crate::properties::PropertyStore::default(),
            budget: Arc::new(crate::security::Budget::new(limits.clone())),
            document: None,
            last_error: CString::default(),
            limits,
            max_decoding_threads: 4,
        }
    }
}
impl Context {
    pub fn read(&mut self, input: Arc<dyn Input>) -> Result<()> {
        self.items = crate::items::ItemStore::reading(input.clone());
        self.properties = crate::properties::PropertyStore {
            read_only: true,
            ..Default::default()
        };
        // File parsing failures preserve the previous image model. Once image
        // interpretation begins, even a failed load exposes its partial model.
        let read_limits = *self
            .limits
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let meta = metadata(
            input.bytes(),
            &read_limits,
            Some(&mut self.properties),
            Some(&mut self.items),
        )?;
        if !has_images(&children(&meta[4..])?) {
            if self.document.is_none() {
                self.document = Some(Arc::new(Document {
                    budget: self.budget.clone(),
                    read_limits: *self
                        .limits
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                    limits: self.limits.clone(),
                    input,
                    images: BTreeMap::new(),
                    primary: 0,
                    top_level: Vec::new(),
                }));
            }
            return Ok(());
        }
        let container = Container::parse_meta_with_limits(input.bytes(), meta, read_limits)?;
        let mut document = Document {
            budget: self.budget.clone(),
            read_limits: *self
                .limits
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            limits: self.limits.clone(),
            input: input.clone(),
            images: BTreeMap::new(),
            primary: 0,
            top_level: Vec::new(),
        };
        // Interpretation replaces the context's ownership immediately. Old
        // handles retain only their own objects and referenced auxiliary images.
        self.document = None;
        let outcome = document.interpret(
            &container,
            &self.items,
            &self.properties,
            &mut self.text_items,
        );
        self.document = Some(Arc::new(document));
        outcome
    }
}

fn attach_auxiliary(
    images: &mut BTreeMap<u32, Arc<ImageInfo>>,
    top_level: &mut Vec<u32>,
    container: &Container<'_>,
    id: u32,
    targets: &[u32],
) -> Result<()> {
    let property = container.property(id, *b"auxC").map_err(|_| {
        ContextError::invalid(
            123,
            &format!("Type of auxiliary image unspecified: No auxC property for image {id}"),
        )
    })?;
    let data = property.get(4..).unwrap_or_default();
    // read_string() consumes the final byte without appending it when the box
    // ends before a NUL terminator. Preserve that observable upstream behavior.
    let end = data
        .iter()
        .position(|v| *v == 0)
        .unwrap_or(data.len().saturating_sub(1));
    let kind = &data[..end];
    let subtypes = data.get(end + 1..).unwrap_or_default();
    let alpha = matches!(
        kind,
        b"urn:mpeg:avc:2015:auxid:1"
            | b"urn:mpeg:hevc:2015:auxid:1"
            | b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha"
    );
    let depth = matches!(
        kind,
        b"urn:mpeg:hevc:2015:auxid:2" | b"urn:mpeg:mpegB:cicp:systems:auxiliary:depth"
    );
    if alpha {
        let alpha_depth = {
            let mut next = id;
            let mut visited = BTreeSet::new();
            loop {
                if !visited.insert(next) {
                    break -1;
                }
                let Some(item) = container.items.get(&next) else {
                    break -1;
                };
                if matches!(&item.kind, b"iden" | b"grid" | b"iovl") {
                    let Some(child) = item.references.get(b"dimg").and_then(|ids| ids.first())
                    else {
                        break -1;
                    };
                    next = *child;
                } else {
                    break images.get(&next).map_or(-1, |i| i.luma_bits);
                }
            }
        };
        images
            .get_mut(&id)
            .and_then(Arc::get_mut)
            .unwrap()
            .auxiliary
            .is_alpha = true;
        for target in targets {
            if let Some(master) = images.get_mut(target).and_then(Arc::get_mut) {
                if id == *target {
                    return Err(ContextError::invalid(
                        2000,
                        "Non-existing item ID referenced: Recursive alpha image detected",
                    ));
                }
                master.has_alpha = true;
                master
                    .components
                    .alpha(master.ispe.0, master.ispe.1, alpha_depth)?;
                master.related_images.push(id);
            } else if !container.items.contains_key(target) {
                return Err(ContextError::invalid(
                    2000,
                    "Non-existing item ID referenced: Non-existing alpha image referenced",
                ));
            }
        }
    }
    if depth {
        images
            .get_mut(&id)
            .and_then(Arc::get_mut)
            .unwrap()
            .auxiliary
            .is_depth = true;
        for target in targets {
            if let Some(master) = images.get_mut(target).and_then(Arc::get_mut) {
                if id == *target {
                    return Err(ContextError::invalid(
                        2000,
                        "Non-existing item ID referenced: Recursive depth image detected",
                    ));
                }
                master.auxiliary.depth_image = Some(id);
                master.related_images.push(id);
                if !subtypes.is_empty() {
                    let info = crate::auxiliary::depth_info(subtypes)?;
                    images
                        .get_mut(&id)
                        .and_then(Arc::get_mut)
                        .unwrap()
                        .auxiliary
                        .depth_info = info;
                }
            } else if !container.items.contains_key(target) {
                return Err(ContextError::invalid(
                    2000,
                    "Non-existing item ID referenced: Non-existing depth image referenced",
                ));
            }
        }
    }
    images
        .get_mut(&id)
        .and_then(Arc::get_mut)
        .unwrap()
        .auxiliary
        .kind = CString::new(kind).unwrap();
    for target in targets {
        if let Some(master) = images.get_mut(target).and_then(Arc::get_mut) {
            if id == *target {
                return Err(ContextError::invalid(
                    2000,
                    "Non-existing item ID referenced: Recursive aux image detected",
                ));
            }
            master.related_images.push(id);
            master.auxiliary.images.push(id);
            top_level.retain(|i| *i != id);
        } else if !container.items.contains_key(target) {
            return Err(ContextError::invalid(
                2000,
                "Non-existing item ID referenced: Non-existing aux image referenced",
            ));
        }
    }
    Ok(())
}
