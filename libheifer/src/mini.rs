// SPDX-License-Identifier: LGPL-3.0-or-later
//! Expand minimized image metadata while retaining physical media offsets.
use crate::{
    context::{ContextError, Input, header},
    security::{Budget, Limits},
    writing::{boxed, full, number},
};
use std::{borrow::Cow, sync::Arc};
type Result<T> = std::result::Result<T, ContextError>;

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn get(&mut self, count: usize) -> u32 {
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1)
                | u32::from(
                    self.data.get(self.at / 8).copied().unwrap_or(0) >> (7 - self.at % 8) & 1,
                );
            self.at += 1;
        }
        value
    }
    fn flag(&mut self) -> bool {
        self.get(1) != 0
    }
    fn depth(&mut self, float: bool, gain: bool) -> Result<u8> {
        if float {
            let log = self.get(2) + 4;
            if log == 7 {
                return Err(invalid(&format!(
                    "Reserved float {}bit_depth_log2 value 7 in MinimizedImageBox",
                    if gain { "gainmap " } else { "" }
                )));
            }
            Ok(1 << log)
        } else if self.flag() {
            Ok(self.get(3) as u8 + 9)
        } else {
            Ok(8)
        }
    }
    fn field(&mut self, out: &mut Vec<u8>, bytes: usize) {
        number(out, u64::from(self.get(bytes * 8)), bytes);
    }
    fn hdr(&mut self) -> Vec<Vec<u8>> {
        let flags: Vec<_> = (0..6).map(|_| self.flag()).collect();
        let mut properties = Vec::new();
        for (index, kind) in [*b"clli", *b"mdcv", *b"cclv", *b"amve", *b"reve", *b"ndwt"]
            .into_iter()
            .enumerate()
        {
            if !flags[index] {
                continue;
            }
            let mut data = Vec::new();
            match index {
                0 => {
                    self.field(&mut data, 2);
                    self.field(&mut data, 2);
                }
                1 => {
                    for _ in 0..8 {
                        self.field(&mut data, 2);
                    }
                    self.field(&mut data, 4);
                    self.field(&mut data, 4);
                }
                2 => {
                    let flags = self.get(8) as u8 & 0x3c;
                    data.push(flags);
                    if flags & 32 != 0 {
                        for _ in 0..6 {
                            self.field(&mut data, 4);
                        }
                    }
                    for bit in [16, 8, 4] {
                        if flags & bit != 0 {
                            self.field(&mut data, 4);
                        }
                    }
                }
                3 => {
                    self.field(&mut data, 4);
                    self.field(&mut data, 2);
                    self.field(&mut data, 2);
                }
                4 => {
                    for _ in 0..4 {
                        self.get(32);
                    }
                }
                5 => {
                    data.extend([0; 4]); // ndwt is a version-zero FullBox.
                    self.field(&mut data, 4);
                }
                _ => unreachable!(),
            }
            if index != 4 {
                properties.push(boxed(kind, &data));
            }
        }
        properties
    }
}
fn invalid(message: &str) -> ContextError {
    ContextError::invalid(
        149,
        &format!("Unsupported or invalid 'mini' box: {message}"),
    )
}

struct ExpandedInput {
    metadata: Vec<u8>,
    source: Arc<dyn Input>,
}
impl Input for ExpandedInput {
    fn is_minimized(&self) -> bool {
        true
    }
    fn metadata_limits(&self, limits: Limits) -> Limits {
        // The native mini parser uses global limits. Its expanded tables are
        // installed directly, without reapplying per-context table limits.
        Limits {
            max_items: 0,
            max_children_per_box: 0,
            max_components: 0,
            max_iloc_extents_per_item: 0,
            max_color_profile_size: Limits::default().max_color_profile_size,
            ..limits
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.metadata
    }
    fn length(&self) -> u64 {
        self.source.length()
    }
    fn read_range(&self, offset: u64, size: u64) -> Result<Cow<'_, [u8]>> {
        self.source.read_range(offset, size)
    }
}

struct Mini<'a> {
    width: u32,
    height: u32,
    depth: u8,
    orientation: u32,
    nclx: Vec<u8>,
    hdr: Vec<Vec<u8>>,
    config: &'a [u8],
    alpha_config: &'a [u8],
    icc: &'a [u8],
    compressed: bool,
    extents: Vec<(u16, u64, u32)>,
}
impl<'a> Mini<'a> {
    fn parse(data: &'a [u8], offset: u64, limits: &Limits) -> Result<Self> {
        let mut bits = Bits { data, at: 0 };
        bits.get(2); // The native reader retains, but does not reject, versions.
        let explicit_codec = bits.flag();
        let float = bits.flag();
        let full_range = bits.flag();
        let alpha = bits.flag();
        let cicp = bits.flag();
        let hdr = bits.flag();
        let icc = bits.flag();
        let exif = bits.flag();
        let xmp = bits.flag();
        let chroma = bits.get(2);
        let orientation = bits.get(3) + 1;
        let dimension_bits = if bits.flag() { 15 } else { 7 };
        let width = bits.get(dimension_bits) + 1;
        let height = bits.get(dimension_bits) + 1;
        if matches!(chroma, 1 | 2) {
            bits.flag();
        }
        if chroma == 1 {
            bits.flag();
        }
        let depth = bits.depth(float, false)?;
        if alpha {
            bits.flag();
        } // Native expansion does not install prem.
        let (cp, tc, mc) = if cicp {
            (bits.get(8), bits.get(8), bits.get(8))
        } else {
            (
                if icc { 2 } else { 1 },
                if icc { 2 } else { 13 },
                if chroma == 0 { 2 } else { 6 },
            )
        };
        let mut nclx = b"nclx".to_vec();
        for value in [cp, tc, mc] {
            number(&mut nclx, u64::from(value), 2);
        }
        nclx.push(u8::from(full_range) << 7);
        if explicit_codec {
            bits.get(32);
            bits.get(32);
        }
        let gain = hdr && bits.flag();
        let mut tmap_icc = false;
        if gain {
            if !bits.flag() {
                bits.get(dimension_bits);
                bits.get(dimension_bits);
            }
            bits.get(8);
            bits.flag();
            let gain_chroma = bits.get(2);
            if matches!(gain_chroma, 1 | 2) {
                bits.flag();
            }
            if gain_chroma == 1 {
                bits.flag();
            }
            let gain_float = bits.flag();
            bits.depth(gain_float, true)?;
            tmap_icc = bits.flag();
            if bits.flag() {
                bits.get(24);
                bits.flag();
            }
        }
        let hdr = if hdr { bits.hdr() } else { Vec::new() };
        if gain {
            bits.hdr();
        }
        let meta_bits = if (icc || exif || xmp || gain) && bits.flag() {
            20
        } else {
            10
        };
        let config_bits = if bits.flag() { 12 } else { 3 };
        let item_bits = if bits.flag() { 28 } else { 15 };
        let icc_size = if icc { bits.get(meta_bits) + 1 } else { 0 };
        let tmap_icc_size = if tmap_icc { bits.get(meta_bits) + 1 } else { 0 };
        let gain_metadata_size = if gain { bits.get(meta_bits) } else { 0 };
        let gain_data_size = if gain { bits.get(item_bits) } else { 0 };
        let gain_config_size = if gain_data_size != 0 {
            bits.get(config_bits)
        } else {
            0
        };
        let config_size = bits.get(config_bits);
        let data_size = bits.get(item_bits) + 1;
        let alpha_data_size = if alpha { bits.get(item_bits) } else { 0 };
        let alpha_config_size = if alpha_data_size != 0 {
            bits.get(config_bits)
        } else {
            0
        };
        let compressed = (exif || xmp) && bits.flag();
        let exif_size = if exif { bits.get(meta_bits) + 1 } else { 0 };
        let xmp_size = if xmp { bits.get(meta_bits) + 1 } else { 0 };
        let mut at = bits.at.div_ceil(8);
        let required: u64 = [
            icc_size,
            tmap_icc_size,
            gain_metadata_size,
            gain_data_size,
            gain_config_size,
            config_size,
            data_size,
            alpha_data_size,
            alpha_config_size,
            exif_size,
            xmp_size,
        ]
        .into_iter()
        .map(u64::from)
        .sum();
        if at > data.len() || required > (data.len() - at) as u64 {
            return Err(invalid(
                "Declared chunk sizes in MinimizedImageBox exceed available payload.",
            ));
        }
        for (size, name) in [(icc_size, "ICC"), (tmap_icc_size, "Tone-map ICC")] {
            if limits.max_color_profile_size != 0 && size > limits.max_color_profile_size {
                return Err(ContextError::invalid(
                    1000,
                    &format!(
                        "Security limit exceeded: {name} color profile in MinimizedImageBox exceeds maximum supported size"
                    ),
                ));
            }
        }
        let mut take = |size: u32| {
            let value = &data[at..at + size as usize];
            at += size as usize;
            value
        };
        let config = take(config_size);
        let alpha_config = if alpha_data_size == 0 {
            &[]
        } else if alpha_config_size == 0 {
            config
        } else {
            take(alpha_config_size)
        };
        take(gain_config_size);
        let icc = take(icc_size);
        take(tmap_icc_size);
        take(gain_metadata_size);
        let mut extents = Vec::new();
        for (id, size) in [
            (2, alpha_data_size),
            (4, gain_data_size),
            (1, data_size),
            (6, exif_size),
            (7, xmp_size),
        ] {
            if size != 0 {
                if id != 4 {
                    extents.push((id, offset + at as u64, size));
                }
                at += size as usize;
            }
        }
        extents.sort_by_key(|&(id, _, _)| id);
        Ok(Self {
            width,
            height,
            depth,
            orientation,
            nclx,
            hdr,
            config,
            alpha_config,
            icc,
            compressed,
            extents,
        })
    }
    fn metadata(self, brand: &[u8]) -> Result<Vec<u8>> {
        let (kind, config_kind) = match brand {
            b"avif" => (*b"av01", *b"av1C"),
            b"heic" => (*b"hvc1", *b"hvcC"),
            _ => {
                return Err(ContextError::new(
                    3,
                    0,
                    format!(
                        "Unsupported file-type: Unspecified: Minimised file requires brand {} but this is not yet supported.",
                        String::from_utf8_lossy(brand)
                    ),
                ));
            }
        };
        let alpha = self.extents.iter().any(|e| e.0 == 2);
        let config = |data: &[u8]| {
            if data.is_empty() {
                boxed(*b"free", &[])
            } else {
                boxed(config_kind, data)
            }
        };
        let mut ispe = Vec::new();
        number(&mut ispe, u64::from(self.width), 4);
        number(&mut ispe, u64::from(self.height), 4);
        let mut properties = vec![
            config(self.config),
            full(*b"ispe", 0, 0, &ispe),
            full(*b"pixi", 0, 0, &[3, self.depth, self.depth, self.depth]),
            boxed(*b"colr", &self.nclx),
            if self.icc.is_empty() {
                boxed(*b"free", &[])
            } else {
                boxed(*b"colr", &[b"prof".as_slice(), self.icc].concat())
            },
            config(self.alpha_config),
            if alpha {
                full(
                    *b"auxC",
                    0,
                    0,
                    b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha\0",
                )
            } else {
                boxed(*b"free", &[])
            },
            boxed(*b"free", &[]),
        ];
        let mut associations = vec![(1u16, vec![0x81, 2, 3, 0x84, 0x85])];
        if alpha {
            associations.push((2, vec![0x86, 2, 0x87, 8]));
        }
        for property in self.hdr {
            properties.push(property);
            associations[0].1.push(properties.len() as u8);
        }
        let (rotation, mirror) = match self.orientation {
            2 => (0, Some(1)),
            3 => (2, None),
            4 => (0, Some(0)),
            5 => (3, Some(1)),
            6 => (3, None),
            7 => (3, Some(0)),
            8 => (1, None),
            _ => (0, None),
        };
        for (_, assoc) in &mut associations {
            for (kind, value) in [
                (*b"irot", (rotation != 0).then_some(rotation)),
                (*b"imir", mirror),
            ] {
                if let Some(value) = value {
                    properties.push(boxed(kind, &[value]));
                    assoc.push(0x80 | properties.len() as u8);
                }
            }
        }
        let mut ipma = Vec::new();
        number(&mut ipma, associations.len() as u64, 4);
        for (id, assoc) in associations {
            number(&mut ipma, u64::from(id), 2);
            ipma.push(assoc.len() as u8);
            ipma.extend(assoc);
        }
        let mut iprp = boxed(*b"ipco", &properties.concat());
        iprp.extend(full(*b"ipma", 0, 0, &ipma));
        let mut iinf = Vec::new();
        number(&mut iinf, self.extents.len() as u64, 2);
        let mut iloc = vec![0x88, 0];
        number(&mut iloc, self.extents.len() as u64, 2);
        let mut iref = Vec::new();
        for (id, offset, size) in self.extents {
            let mut infe = Vec::new();
            number(&mut infe, u64::from(id), 2);
            number(&mut infe, 0, 2);
            infe.extend(if id == 6 {
                *b"Exif"
            } else if id == 7 {
                *b"mime"
            } else {
                kind
            });
            infe.push(0);
            if id == 7 {
                infe.extend(b"application/rdf+xml\0");
                if self.compressed {
                    infe.extend(b"deflate");
                }
                infe.push(0);
            }
            iinf.extend(full(*b"infe", 2, u32::from(id != 1), &infe));
            number(&mut iloc, u64::from(id), 2);
            number(&mut iloc, 0, 2);
            number(&mut iloc, 1, 2);
            number(&mut iloc, offset, 8);
            number(&mut iloc, u64::from(size), 8);
            if id != 1 {
                let mut refs = Vec::new();
                number(&mut refs, u64::from(id), 2);
                number(&mut refs, 1, 2);
                number(&mut refs, 1, 2);
                iref.extend(boxed(if id == 2 { *b"auxl" } else { *b"cdsc" }, &refs));
            }
        }
        let mut hdlr = vec![0; 4];
        hdlr.extend(b"pict");
        hdlr.extend([0; 13]);
        let mut meta = full(*b"hdlr", 0, 0, &hdlr);
        meta.extend(full(*b"pitm", 0, 0, &[0, 1]));
        meta.extend(full(*b"iinf", 0, 0, &iinf));
        meta.extend(full(*b"iloc", 0, 0, &iloc));
        meta.extend(full(*b"iref", 0, 0, &iref));
        meta.extend(boxed(*b"iprp", &iprp));
        Ok(full(*b"meta", 0, 0, &meta))
    }
}

pub(crate) fn payload_budget(size: u64) -> Result<crate::security::Reservation> {
    let limits = Limits {
        max_total_memory: 0,
        ..Limits::default()
    };
    let budget = Arc::new(Budget::new(Arc::new(std::sync::RwLock::new(limits))));
    Ok(budget.reserve(size, "MinimizedImageBox payload")?)
}
pub(crate) fn expand(input: Arc<dyn Input>) -> Result<Arc<dyn Input>> {
    let bytes = input.bytes();
    // Leave the ordinary reader responsible for malformed initial headers.
    if bytes.len() < 32 {
        return Ok(input);
    }
    let Ok(first) = header(bytes) else {
        return Ok(input);
    };
    if first.kind != *b"ftyp"
        || first.size < (first.header + 8) as u64
        || first.size > bytes.len() as u64
    {
        return Ok(input);
    }
    let mut at = first.size as usize;
    let mut found = None;
    while at.checked_add(32).is_some_and(|end| end <= bytes.len()) {
        let h = header(&bytes[at..])?;
        let size = if h.size == 0 {
            bytes.len() - at
        } else {
            usize::try_from(h.size)
                .map_err(|_| ContextError::invalid(100, "Unexpected end of file"))?
        };
        if size < h.header {
            return Ok(input);
        }
        if h.kind == *b"mini" {
            let data = bytes
                .get(at + h.header..at.saturating_add(size))
                .ok_or_else(|| {
                    ContextError::invalid(
                        149,
                        "Unsupported or invalid 'mini' box: Cannot read full mini box",
                    )
                })?;
            // Global limits are unregistered: only their block limit applies
            // to this temporary buffer, not the context's live memory budget.
            let limits = Limits {
                max_total_memory: 0,
                ..Limits::default()
            };
            let _reservation = payload_budget(data.len() as u64)?;
            found = Some(Mini::parse(data, input.original_offset(data), &limits)?);
        }
        if h.size == 0 {
            break;
        }
        at = at
            .checked_add(size)
            .ok_or_else(|| ContextError::invalid(100, "Unexpected end of file"))?;
    }
    let Some(mini) = found else {
        return Ok(input);
    };
    let mut metadata = bytes[..first.size as usize].to_vec();
    metadata.extend(mini.metadata(&bytes[first.header + 4..first.header + 8])?);
    Ok(Arc::new(ExpandedInput {
        metadata,
        source: input,
    }))
}
