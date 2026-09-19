// SPDX-License-Identifier: LGPL-3.0-or-later
//! Item-local property IDs, typed lookup and in-memory property insertion.
use crate::context::ContextError;
use std::{collections::BTreeMap, ffi::CString, sync::Arc};

#[derive(Clone, Debug)]
pub struct Property {
    pub kind: [u8; 4],
    pub uuid: Option<[u8; 16]>,
    pub data: Vec<u8>,
    pub raw: bool,
    pub tai: Option<crate::tai::TaiProperty>,
}
impl Property {
    pub(crate) fn parsed(kind: [u8; 4], uuid: Option<[u8; 16]>, data: &[u8]) -> Self {
        let malformed = parse_error(crate::camera::kind(kind, uuid), data).is_some();
        Self {
            kind: if malformed { *b"ERR " } else { kind },
            uuid,
            data: data.to_vec(),
            raw: !malformed && parsed_raw(kind, uuid),
            tai: if matches!(&kind, b"taic" | b"itai") {
                crate::tai::TaiProperty::parse(kind, data).ok()
            } else {
                None
            },
        }
    }

    pub fn user_description(strings: [&[u8]; 4]) -> Self {
        let mut data = vec![0; 4];
        for value in strings {
            data.extend_from_slice(value);
            data.push(0);
        }
        Self {
            kind: *b"udes",
            uuid: None,
            data,
            raw: false,
            tai: None,
        }
    }
    pub fn description(&self) -> [CString; 4] {
        let mut data = self.data.get(4..).unwrap_or_default();
        std::array::from_fn(|_| {
            let end = data
                .iter()
                .position(|v| *v == 0)
                .unwrap_or(data.len().saturating_sub(1));
            let string = CString::new(&data[..end]).unwrap();
            data = data.get(end + 1..).unwrap_or_default();
            string
        })
    }
}
/// Optional descriptions survive as warnings; malformed transforms are retained
/// as error boxes and rejected when their image is interpreted.
pub(crate) fn parse_error(kind: [u8; 4], data: &[u8]) -> Option<(ContextError, bool)> {
    match &kind {
        b"taic" | b"itai" => crate::tai::TaiProperty::parse(kind, data)
            .err()
            .map(|e| (e, false)),
        b"cmin" => crate::camera::intrinsic(data, 0, 0)
            .err()
            .map(|e| (e, true)),
        b"cmex" => crate::camera::ExtrinsicMatrix::parse(data)
            .err()
            .map(|e| (e, true)),
        b"udes" | b"elng" if data.len() < 4 => {
            Some((ContextError::invalid(100, "Unexpected end of file"), true))
        }
        b"udes" | b"elng" if data[0] != 0 => Some((
            ContextError::new(
                4,
                3002,
                format!(
                    "Unsupported feature: Unsupported data version: {} box data version {} is not implemented yet",
                    String::from_utf8_lossy(&kind),
                    data[0]
                ),
            ),
            true,
        )),
        b"irot" | b"imir" if data.is_empty() => {
            Some((ContextError::invalid(100, "Unexpected end of file"), false))
        }
        b"clap" => crate::geometry::CleanAperture::parse(data)
            .err()
            .map(|e| (e, false)),
        _ => None,
    }
}

#[derive(Default)]
pub struct PropertyStore {
    pub read_only: bool,
    pub has_ipco: bool,
    pub has_ipma: bool,
    pub boxes: Vec<Arc<Property>>,
    pub items: BTreeMap<u32, Vec<usize>>,
    pub essential: BTreeMap<u32, Vec<bool>>,
}
impl PropertyStore {
    pub fn get(&self, id: u32) -> Result<Vec<&Property>, ContextError> {
        if !self.has_ipco {
            return Err(ContextError::invalid(108, "No 'ipco' box"));
        }
        if !self.has_ipma {
            return Err(ContextError::invalid(109, "No 'ipma' box"));
        }
        let Some(indices) = self.items.get(&id) else {
            return Err(ContextError::invalid(
                116,
                &format!(
                    "No properties assigned to item: Item (ID={id}) has no properties assigned to it in ipma box"
                ),
            ));
        };
        indices.iter().map(|i| self.boxes.get(*i).map(AsRef::as_ref).ok_or_else(|| ContextError::invalid(115, &format!("'ipma' box references a non-existing property: Nonexisting property (index={}) for item  ID={id} referenced in ipma box", i+1)))).collect()
    }
    /// None selects the Box_other class, including raw boxes inserted using a
    /// known fourcc. A zero property ID selects the first matching dynamic type.
    pub fn find(
        &self,
        item: u32,
        id: u32,
        kind: Option<[u8; 4]>,
    ) -> Result<&Property, ContextError> {
        let matches = |p: &&Property| match kind {
            None => p.raw,
            Some(kind) => !p.raw && p.kind == kind,
        };
        if id == 0 {
            return self
                .get(item)
                .ok()
                .and_then(|p| p.into_iter().find(matches))
                .ok_or_else(|| {
                    ContextError::invalid(
                        116,
                        "No properties assigned to item: property not found on item",
                    )
                });
        }
        let properties = self.get(item)?;
        let property = properties
            .get(id as usize - 1)
            .ok_or_else(|| invalid("property index out of range"))?;
        if !matches(property) {
            return Err(invalid("wrong property type"));
        }
        Ok(property)
    }
    pub fn add(
        &mut self,
        item: u32,
        property: Property,
        essential: bool,
    ) -> Result<u32, ContextError> {
        if self.read_only {
            return Err(ContextError::new(
                4,
                0,
                "Unsupported feature: Unspecified: Adding a property to a context that was read from a file is not supported",
            ));
        }
        self.has_ipco = true;
        self.has_ipma = true;
        // Box equality compares serialized data, so a previously inserted raw
        // box can win deduplication against a later typed box with the same bytes.
        let index = self
            .boxes
            .iter()
            .position(|p| {
                p.kind == property.kind
                    && p.uuid == property.uuid
                    && if p.tai.is_some() {
                        p.tai == property.tai
                    } else {
                        p.data == property.data
                    }
            })
            .unwrap_or_else(|| {
                self.boxes.push(Arc::new(property));
                self.boxes.len() - 1
            });
        if index >= u16::MAX as usize {
            return Err(ContextError::new(
                8,
                0,
                "Encoding error: Unspecified: Cannot add property to item",
            ));
        }
        let properties = self.items.entry(item).or_default();
        if let Some(id) = properties.iter().position(|i| *i == index) {
            return Ok(id as u32 + 1);
        }
        properties.push(index);
        self.essential.entry(item).or_default().push(essential);
        Ok(properties.len() as u32)
    }
}
fn invalid(message: &str) -> ContextError {
    ContextError::new(5, 2007, format!("Usage error: Invalid property: {message}"))
}

pub(crate) fn parsed_raw(kind: [u8; 4], uuid: Option<[u8; 16]>) -> bool {
    if kind == *b"uuid" {
        return ![
            [
                0x22, 0xcc, 0x04, 0xc7, 0xd6, 0xd9, 0x4e, 0x07, 0x9d, 0x90, 0x4e, 0xb6, 0xec, 0xba,
                0xf3, 0xa3,
            ],
            [
                0x43, 0x63, 0xe9, 0x14, 0x5b, 0x7d, 0x4a, 0xab, 0x97, 0xae, 0xbe, 0xa6, 0x98, 0x03,
                0xb4, 0x34,
            ],
            [
                0x26, 0x1e, 0xf3, 0x74, 0x1d, 0x97, 0x5b, 0xba, 0xac, 0xbd, 0x9d, 0x2c, 0x8e, 0xa7,
                0x35, 0x22,
            ],
            [
                0x9d, 0xb9, 0xdd, 0x6e, 0x37, 0x3c, 0x5a, 0x4e, 0x81, 0x10, 0x21, 0xfc, 0x83, 0xa9,
                0x11, 0xfd,
            ],
        ]
        .into_iter()
        .any(|known| uuid == Some(known));
    }
    !KNOWN_BOXES.contains(&kind)
}

// Filled from the pinned box factory; experimental tilC is disabled in the contract.
const KNOWN_BOXES: &[[u8; 4]] = &[
    *b"ftyp", *b"free", *b"skip", *b"meta", *b"hdlr", *b"pitm", *b"iloc", *b"iinf", *b"infe",
    *b"iprp", *b"ipco", *b"ipma", *b"ispe", *b"auxC", *b"irot", *b"imir", *b"clap", *b"iscl",
    *b"iref", *b"rref", *b"hvcC", *b"hvc1", *b"av1C", *b"av01", *b"vvcC", *b"vvc1", *b"idat",
    *b"grpl", *b"pymd", *b"altr", *b"ster", *b"dinf", *b"dref", *b"url ", *b"colr", *b"pixi",
    *b"pasp", *b"lsel", *b"a1op", *b"a1lx", *b"clli", *b"mdcv", *b"amve", *b"ndwt", *b"cmin",
    *b"cmex", *b"udes", *b"jpgC", *b"mjpg", *b"elng", *b"cmpd", *b"uncC", *b"cmpC", *b"icef",
    *b"cpat", *b"splz", *b"sbpm", *b"snuc", *b"cloc", *b"uncv", *b"j2kH", *b"cdef", *b"cmap",
    *b"pclr", *b"j2kL", *b"j2ki", *b"mskC", *b"itai", *b"taic", *b"avcC", *b"avc1", *b"mini",
    *b"mdat", *b"moov", *b"mvhd", *b"trak", *b"tkhd", *b"mdia", *b"mdhd", *b"minf", *b"vmhd",
    *b"stbl", *b"stsd", *b"stts", *b"ctts", *b"stsc", *b"stco", *b"stsz", *b"stss", *b"ccst",
    *b"auxi", *b"edts", *b"elst", *b"sbgp", *b"sgpd", *b"btrt", *b"saiz", *b"saio", *b"urim",
    *b"uri ", *b"nmhd", *b"tref", *b"sdtp", *b"prfr",
];
