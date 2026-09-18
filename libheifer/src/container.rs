// SPDX-License-Identifier: LGPL-3.0-or-later
//! Bounded, borrowed ISO BMFF item parsing. This is an initial subset, not the
//! complete libheif parser; unsupported construction methods fail explicitly.
use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Truncated,
    InvalidSize,
    InvalidField,
    Limit,
    Unsupported,
    MissingItem,
    MissingProperty,
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ParseError {}
type Result<T> = std::result::Result<T, ParseError>;

#[derive(Clone, Copy, Debug)]
struct BoxView<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

fn boxes(mut data: &[u8], limit: usize) -> Result<Vec<BoxView<'_>>> {
    let mut result = Vec::new();
    while !data.is_empty() {
        if result.len() >= limit {
            return Err(ParseError::Limit);
        }
        let mut reader = Reader(data);
        let size = reader.number(4)?;
        let kind = reader.fourcc()?;
        let size = match size {
            0 => data.len() as u64,
            1 => reader.number(8)?,
            n => n,
        };
        if kind == *b"uuid" {
            reader.take(16)?;
        }
        let header = data.len() - reader.0.len();
        let size = usize::try_from(size).map_err(|_| ParseError::InvalidSize)?;
        if size < header {
            return Err(ParseError::InvalidSize);
        }
        let whole = data.get(..size).ok_or(ParseError::Truncated)?;
        result.push(BoxView {
            kind,
            data: &whole[header..],
        });
        data = &data[size..];
    }
    Ok(result)
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let (a, b) = self.0.split_at_checked(n).ok_or(ParseError::Truncated)?;
        self.0 = b;
        Ok(a)
    }
    fn number(&mut self, bytes: usize) -> Result<u64> {
        if bytes > 8 {
            return Err(ParseError::InvalidField);
        }
        Ok(self
            .take(bytes)?
            .iter()
            .fold(0, |a, b| (a << 8) | u64::from(*b)))
    }
    fn fourcc(&mut self) -> Result<[u8; 4]> {
        Ok(self.take(4)?.try_into().unwrap())
    }
    fn fullbox(&mut self) -> Result<(u8, u32)> {
        Ok((self.number(1)? as u8, self.number(3)? as u32))
    }
    fn string(&mut self) -> Result<Vec<u8>> {
        let end = self
            .0
            .iter()
            .position(|b| *b == 0)
            .ok_or(ParseError::Truncated)?;
        let bytes = self.take(end + 1)?;
        Ok(bytes[..end].to_vec())
    }
    fn id(&mut self, wide: bool) -> Result<u32> {
        Ok(self.number(if wide { 4 } else { 2 })? as u32)
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    pub id: u32,
    pub kind: [u8; 4],
    pub hidden: bool,
    pub name: Vec<u8>,
    pub content_type: Vec<u8>,
    pub content_encoding: Vec<u8>,
    pub uri_type: Vec<u8>,
    properties: Vec<usize>,
    extents: Vec<(bool, Range<usize>)>,
    pub references: BTreeMap<[u8; 4], Vec<u32>>,
}

/// All slices refer to the original input, whose lifetime is enforced by Rust.
#[derive(Debug)]
pub struct Container<'a> {
    data: &'a [u8],
    idat: Option<&'a [u8]>,
    pub primary: u32,
    pub items: BTreeMap<u32, Item>,
    properties: Vec<BoxView<'a>>,
}

impl<'a> Container<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let top = boxes(data, 100)?;
        if !top.iter().any(|b| b.kind == *b"ftyp") {
            return Err(ParseError::InvalidField);
        }
        let meta = top
            .iter()
            .find(|b| b.kind == *b"meta")
            .ok_or(ParseError::MissingItem)?;
        Self::parse_meta(data, meta.data)
    }

    /// Parse the metadata independently of the availability of media payloads.
    pub(crate) fn parse_meta(data: &'a [u8], meta: &'a [u8]) -> Result<Self> {
        let mut reader = Reader(meta);
        if reader.fullbox()?.0 != 0 {
            return Err(ParseError::Unsupported);
        }
        let children = boxes(reader.0, 100)?;
        let child = |kind| {
            children
                .iter()
                .find(|b| b.kind == kind)
                .map(|b| b.data)
                .ok_or(ParseError::MissingProperty)
        };
        let mut primary = Reader(child(*b"pitm")?);
        let (version, _) = primary.fullbox()?;
        if version > 1 {
            return Err(ParseError::Unsupported);
        }
        let mut result = Self {
            data,
            idat: children.iter().find(|b| b.kind == *b"idat").map(|b| b.data),
            primary: primary.id(version == 1)?,
            items: BTreeMap::new(),
            properties: Vec::new(),
        };
        let mut info = Reader(child(*b"iinf")?);
        let (version, _) = info.fullbox()?;
        let count = info.number(if version == 0 { 2 } else { 4 })? as usize;
        if count > 1000 {
            return Err(ParseError::Limit);
        }
        let entries = boxes(info.0, 1000)?;
        if entries.len() != count {
            return Err(ParseError::InvalidField);
        }
        for entry in entries {
            if entry.kind != *b"infe" {
                return Err(ParseError::InvalidField);
            }
            let mut r = Reader(entry.data);
            let (version, flags) = r.fullbox()?;
            if !matches!(version, 2 | 3) {
                return Err(ParseError::Unsupported);
            }
            let id = r.id(version == 3)?;
            if r.number(2)? != 0 {
                return Err(ParseError::Unsupported);
            }
            let kind = r.fourcc()?;
            let name = r.string()?;
            let content_type = if kind == *b"mime" {
                r.string()?
            } else {
                Vec::new()
            };
            let content_encoding = if kind == *b"mime" && !r.0.is_empty() {
                r.string()?
            } else {
                Vec::new()
            };
            let uri_type = if kind == *b"uri " {
                r.string()?
            } else {
                Vec::new()
            };
            if result
                .items
                .insert(
                    id,
                    Item {
                        id,
                        kind,
                        hidden: flags & 1 != 0,
                        name,
                        content_type,
                        content_encoding,
                        uri_type,
                        properties: Vec::new(),
                        extents: Vec::new(),
                        references: BTreeMap::new(),
                    },
                )
                .is_some()
            {
                return Err(ParseError::InvalidField);
            }
        }
        result.parse_locations(child(*b"iloc")?)?;
        if let Ok(properties) = child(*b"iprp") {
            let children = boxes(properties, 100)?;
            let ipco = children
                .iter()
                .find(|b| b.kind == *b"ipco")
                .ok_or(ParseError::MissingProperty)?;
            result.properties = boxes(ipco.data, 1000)?;
            for entry in children.iter().filter(|b| b.kind == *b"ipma") {
                result.parse_associations(entry.data)?;
            }
        }
        if let Ok(references) = child(*b"iref") {
            let mut r = Reader(references);
            let (version, _) = r.fullbox()?;
            if version > 1 {
                return Err(ParseError::Unsupported);
            }
            for reference in boxes(r.0, 1000)? {
                let mut r = Reader(reference.data);
                let from = r.id(version == 1)?;
                let count = r.number(2)? as usize;
                if count > 1000 {
                    return Err(ParseError::Limit);
                }
                let item = result.items.get_mut(&from).ok_or(ParseError::MissingItem)?;
                let refs = item.references.entry(reference.kind).or_default();
                for _ in 0..count {
                    refs.push(r.id(version == 1)?);
                }
            }
        }
        Ok(result)
    }

    fn parse_locations(&mut self, data: &[u8]) -> Result<()> {
        let mut r = Reader(data);
        let (version, _) = r.fullbox()?;
        if version > 2 {
            return Err(ParseError::Unsupported);
        }
        let sizes = r.number(1)? as usize;
        let sizes2 = r.number(1)? as usize;
        let (offset_size, length_size, base_size) = (sizes >> 4, sizes & 15, sizes2 >> 4);
        let index_size = if version > 0 { sizes2 & 15 } else { 0 };
        if [offset_size, length_size, base_size, index_size]
            .into_iter()
            .any(|s| s > 8)
        {
            return Err(ParseError::InvalidField);
        }
        let count = r.number(if version < 2 { 2 } else { 4 })? as usize;
        if count > 1000 {
            return Err(ParseError::Limit);
        }
        for _ in 0..count {
            let id = r.id(version == 2)?;
            let construction = if version > 0 { r.number(2)? & 15 } else { 0 };
            if construction > 1 || r.number(2)? != 0 {
                return Err(ParseError::Unsupported);
            }
            let base = r.number(base_size)?;
            let count = r.number(2)? as usize;
            if count > 32 {
                return Err(ParseError::Limit);
            }
            let item = self.items.get_mut(&id).ok_or(ParseError::MissingItem)?;
            for _ in 0..count {
                r.number(index_size)?;
                let start = base
                    .checked_add(r.number(offset_size)?)
                    .ok_or(ParseError::InvalidSize)?;
                let length = r.number(length_size)?;
                let end = start.checked_add(length).ok_or(ParseError::InvalidSize)?;
                let start = usize::try_from(start).map_err(|_| ParseError::InvalidSize)?;
                let end = usize::try_from(end).map_err(|_| ParseError::InvalidSize)?;
                // Extents are checked on access, not on file load. Upstream
                // permits querying handles before compressed payloads arrive.
                item.extents.push((construction == 1, start..end));
            }
        }
        Ok(())
    }

    fn parse_associations(&mut self, data: &[u8]) -> Result<()> {
        let mut r = Reader(data);
        let (version, flags) = r.fullbox()?;
        if version > 1 {
            return Err(ParseError::Unsupported);
        }
        let count = r.number(4)? as usize;
        if count > 1000 {
            return Err(ParseError::Limit);
        }
        for _ in 0..count {
            let id = r.id(version == 1)?;
            let count = r.number(1)? as usize;
            let item = self.items.get_mut(&id).ok_or(ParseError::MissingItem)?;
            for _ in 0..count {
                let index = if flags & 1 != 0 {
                    r.number(2)? & 0x7fff
                } else {
                    r.number(1)? & 0x7f
                } as usize;
                if index == 0 {
                    continue;
                }
                if index > self.properties.len() {
                    return Err(ParseError::MissingProperty);
                }
                item.properties.push(index - 1);
            }
        }
        Ok(())
    }

    pub fn top_level_images(&self) -> impl Iterator<Item = &Item> {
        self.items.values().filter(|i| {
            !i.hidden
                && !i.references.contains_key(b"thmb")
                && !i.references.contains_key(b"auxl")
                && matches!(
                    &i.kind,
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
                )
        })
    }
    pub fn property(&self, id: u32, kind: [u8; 4]) -> Result<&'a [u8]> {
        let item = self.items.get(&id).ok_or(ParseError::MissingItem)?;
        item.properties
            .iter()
            .map(|i| self.properties[*i])
            .find(|p| p.kind == kind)
            .map(|p| p.data)
            .ok_or(ParseError::MissingProperty)
    }
    pub fn properties(&self, id: u32) -> Result<impl Iterator<Item = ([u8; 4], &'a [u8])> + '_> {
        let item = self.items.get(&id).ok_or(ParseError::MissingItem)?;
        Ok(item.properties.iter().map(|i| {
            let p = self.properties[*i];
            (p.kind, p.data)
        }))
    }
    pub fn dimensions(&self, id: u32) -> Result<(u32, u32)> {
        let mut r = Reader(self.property(id, *b"ispe")?);
        r.fullbox()?;
        Ok((r.number(4)? as u32, r.number(4)? as u32))
    }
    pub fn payload(&self, id: u32) -> Result<Vec<u8>> {
        let item = self.items.get(&id).ok_or(ParseError::MissingItem)?;
        let len = item.extents.iter().try_fold(0_usize, |sum, (_, r)| {
            sum.checked_add(r.len()).ok_or(ParseError::Limit)
        })?;
        // Initial local cap; it is not yet the configurable libheif resource policy.
        if len > 256 * 1024 * 1024 {
            return Err(ParseError::Limit);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(len)
            .map_err(|_| ParseError::Limit)?;
        for (idat, range) in &item.extents {
            bytes.extend_from_slice(
                if *idat {
                    self.idat.ok_or(ParseError::MissingItem)?
                } else {
                    self.data
                }
                .get(range.clone())
                .ok_or(ParseError::Truncated)?,
            );
        }
        Ok(bytes)
    }

    /// Return parameter-set and picture NAL units for one direct HEVC item.
    /// Grids/transforms/auxiliary composition are separate operations, not silently ignored.
    pub fn hevc_nals(&self, id: u32) -> Result<Vec<Vec<u8>>> {
        if self.items.get(&id).ok_or(ParseError::MissingItem)?.kind != *b"hvc1" {
            return Err(ParseError::Unsupported);
        }
        let mut config = Reader(self.property(id, *b"hvcC")?);
        let header = config.take(23)?;
        if header[0] != 1 {
            return Err(ParseError::Unsupported);
        }
        let length_size = usize::from(header[21] & 3) + 1;
        let mut nals = Vec::new();
        for _ in 0..header[22] {
            config.number(1)?;
            let count = config.number(2)?;
            for _ in 0..count {
                let size = config.number(2)? as usize;
                if nals.len() >= 65536 {
                    return Err(ParseError::Limit);
                }
                nals.push(config.take(size)?.to_vec());
            }
        }
        let payload = self.payload(id)?;
        let mut r = Reader(&payload);
        while !r.0.is_empty() {
            let size = r.number(length_size)? as usize;
            if nals.len() >= 65536 {
                return Err(ParseError::Limit);
            }
            nals.push(r.take(size)?.to_vec());
        }
        Ok(nals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_truncations_of_box_header_fail_safely() {
        let data = b"\0\0\0\x01ftyp\0\0\0\0\0\0\0\x18avif\0\0\0\0";
        for n in 1..data.len() {
            assert!(boxes(&data[..n], 100).is_err());
        }
        assert_eq!(boxes(data, 100).unwrap().len(), 1);
    }
    #[test]
    fn zero_size_and_overflow() {
        assert_eq!(boxes(b"\0\0\0\0free", 100).unwrap().len(), 1);
        assert_eq!(
            boxes(b"\0\0\0\x01free\xff\xff\xff\xff\xff\xff\xff\xff", 100).unwrap_err(),
            ParseError::Truncated
        );
    }
}
