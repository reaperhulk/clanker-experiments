// SPDX-License-Identifier: LGPL-3.0-or-later
//! ISO 23001-17 component definitions and frame configuration.
use crate::{context::ContextError, security::Limits};
use std::ffi::CString;
type Result<T> = std::result::Result<T, ContextError>;

#[derive(Clone, Debug)]
pub struct Definition {
    pub kind: u16,
    pub uri: CString,
}
#[derive(Clone, Debug)]
pub struct Component {
    pub index: u32,
    pub bits: u16,
    pub format: u8,
    pub align: u8,
}
#[derive(Clone, Debug)]
pub struct Configuration {
    pub version: u8,
    pub profile: [u8; 4],
    pub components: Vec<Component>,
    pub sampling: u8,
    pub interleave: u8,
    pub block_size: u8,
    pub flags: u8,
    pub pixel_size: u32,
    pub row_align: u32,
    pub tile_align: u32,
    pub columns: u32,
    pub rows: u32,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            version: 0,
            profile: [0; 4],
            components: Vec::new(),
            sampling: 0,
            interleave: 0,
            block_size: 0,
            flags: 0,
            pixel_size: 0,
            row_align: 0,
            tile_align: 0,
            columns: 1,
            rows: 1,
        }
    }
}
struct Reader<'a> {
    data: &'a [u8],
    error: bool,
}
impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, error: false }
    }
    fn number(&mut self, n: usize) -> u32 {
        if self.data.len() < n {
            self.error = true;
            self.data = &[];
            return 0;
        }
        let value = self.data[..n]
            .iter()
            .fold(0, |v, b| (v << 8) | u32::from(*b));
        self.data = &self.data[n..];
        value
    }
    fn string(&mut self) -> CString {
        let end = self
            .data
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(self.data.len().saturating_sub(1));
        let value = CString::new(&self.data[..end]).unwrap();
        self.data = self.data.get(end + 1..).unwrap_or_default();
        value
    }
    fn finish(&self) -> Result<()> {
        if self.error {
            Err(ContextError::invalid(100, "Unexpected end of file"))
        } else {
            Ok(())
        }
    }
}
fn invalid(message: impl Into<String>) -> ContextError {
    ContextError::new(
        2,
        2006,
        format!("Invalid input: Invalid parameter value: {}", message.into()),
    )
}
pub fn definitions(data: &[u8], max_components: u32) -> Result<Vec<Definition>> {
    let mut r = Reader::new(data);
    let count = r.number(4);
    if max_components != 0 && count > max_components {
        return Err(ContextError::new(
            2,
            1000,
            format!(
                "Invalid input: Security limit exceeded: cmpd box should contain {count} components, but security limit is set to {max_components} components"
            ),
        ));
    }
    let mut out = Vec::new();
    for index in 0..count {
        if r.data.is_empty() {
            return Err(ContextError::new(
                2,
                100,
                format!(
                    "Invalid input: Unexpected end of file: cmpd box should contain {count} components, but box only contained {index} components"
                ),
            ));
        }
        let kind = r.number(2) as u16;
        let uri = if kind >= 0x8000 {
            r.string()
        } else {
            CString::default()
        };
        out.push(Definition { kind, uri });
    }
    r.finish()?;
    Ok(out)
}
impl Configuration {
    pub fn parse(data: &[u8], limits: Option<&Limits>) -> Result<Self> {
        let mut r = Reader::new(data);
        let version = (r.number(4) >> 24) as u8;
        let profile = r.number(4).to_be_bytes();
        let mut out = Self {
            version,
            profile,
            ..Self::default()
        };
        if version == 1 {
            if profile_layout(profile).is_none() {
                return Err(invalid("Unknown uncC v1 profile"));
            }
        } else if version == 0 {
            let count = r.number(4);
            if let Some(limits) = limits
                && limits.max_components != 0
                && count > limits.max_components
            {
                return Err(ContextError::new(
                    2,
                    1000,
                    format!(
                        "Invalid input: Security limit exceeded: Number of image components ({count}) exceeds security limit ({})",
                        limits.max_components
                    ),
                ));
            }
            for _ in 0..count {
                if r.error || r.data.is_empty() {
                    break;
                }
                let component = Component {
                    index: r.number(2),
                    bits: r.number(1) as u16 + 1,
                    format: r.number(1) as u8,
                    align: r.number(1) as u8,
                };
                if component.format > 3 {
                    return Err(invalid("Invalid component format"));
                }
                if component.align != 0 && u16::from(component.align) * 8 < component.bits {
                    return Err(invalid(format!(
                        "Component alignment ({} bytes) is too small for component bit depth ({} bits)",
                        component.align, component.bits
                    )));
                }
                out.components.push(component);
            }
            out.sampling = r.number(1) as u8;
            if out.sampling > 3 {
                return Err(invalid("Invalid sampling mode"));
            }
            out.interleave = r.number(1) as u8;
            if out.interleave > 5 {
                return Err(invalid("Invalid interleave mode"));
            }
            out.block_size = r.number(1) as u8;
            out.flags = r.number(1) as u8;
            out.pixel_size = r.number(4);
            if let Some(limits) = limits
                && limits.version >= 4
                && limits.max_iso23001_17_pixel_size_bytes != 0
                && out.pixel_size > limits.max_iso23001_17_pixel_size_bytes
            {
                return Err(ContextError::new(
                    6,
                    1000,
                    format!(
                        "Memory allocation error: Security limit exceeded: uncC pixel_size ({} bytes) exceeds security limit of {} bytes",
                        out.pixel_size, limits.max_iso23001_17_pixel_size_bytes
                    ),
                ));
            }
            out.row_align = r.number(4);
            out.tile_align = r.number(4);
            let columns = r.number(4);
            let rows = r.number(4);
            if columns == u32::MAX || rows == u32::MAX {
                return Err(ContextError::new(
                    4,
                    2006,
                    "Unsupported feature: Invalid parameter value: uncC num_tile_cols/rows_minus_one of 0xFFFFFFFF (2^32 tiles) exceeds the supported range",
                ));
            }
            out.columns = columns + 1;
            out.rows = rows + 1;
            if let Some(limits) = limits
                && limits.max_number_of_tiles != 0
                && u64::from(out.columns) > limits.max_number_of_tiles / u64::from(out.rows)
            {
                return Err(ContextError::new(
                    6,
                    1000,
                    format!(
                        "Memory allocation error: Security limit exceeded: Tiling size {} x {} exceeds the maximum allowed number {} set as security limit",
                        out.columns, out.rows, limits.max_number_of_tiles
                    ),
                ));
            }
        }
        r.finish()?;
        Ok(out)
    }
    /// A supplied cmpd suppresses profile expansion, as in the native helper.
    pub fn expand(&mut self, definitions: &mut Option<Vec<Definition>>) {
        if self.version != 1 || definitions.is_some() {
            return;
        }
        let Some(layout) = profile_layout(self.profile) else {
            return;
        };
        let Profile {
            types,
            indices,
            bits,
            sampling,
            interleave,
            block,
            flags,
        } = layout;
        self.components = indices
            .iter()
            .map(|index| Component {
                index: u32::from(*index),
                bits,
                format: 0,
                align: 0,
            })
            .collect();
        self.sampling = sampling;
        self.interleave = interleave;
        self.block_size = block;
        self.flags = flags;
        *definitions = Some(
            types
                .iter()
                .map(|kind| Definition {
                    kind: *kind,
                    uri: CString::default(),
                })
                .collect(),
        );
    }
}
struct Profile {
    types: &'static [u16],
    indices: &'static [u8],
    bits: u16,
    sampling: u8,
    interleave: u8,
    block: u8,
    flags: u8,
}
fn profile_layout(profile: [u8; 4]) -> Option<Profile> {
    let (types, indices, bits, sampling, interleave, block, flags): (
        &[u16],
        &[u8],
        u16,
        u8,
        u8,
        u8,
        u8,
    ) = match &profile {
        b"rgb3" => (&[4, 5, 6], &[0, 1, 2], 8, 0, 1, 0, 0),
        b"rgba" => (&[4, 5, 6, 7], &[0, 1, 2, 3], 8, 0, 1, 0, 0),
        b"abgr" => (&[7, 6, 5, 4], &[0, 1, 2, 3], 8, 0, 1, 0, 0),
        b"2vuy" => (&[2, 1, 3], &[0, 1, 2, 1], 8, 1, 5, 0, 0),
        b"yuv2" => (&[1, 2, 3], &[0, 1, 0, 2], 8, 1, 5, 0, 0),
        b"yvyu" => (&[1, 3, 2], &[0, 1, 0, 2], 8, 1, 5, 0, 0),
        b"vyuy" => (&[3, 1, 2], &[0, 1, 2, 1], 8, 1, 5, 0, 0),
        b"yuv1" => (&[1, 2, 3], &[0, 0, 1, 0, 0, 2], 8, 3, 5, 0, 0),
        b"v308" => (&[3, 1, 2], &[0, 1, 2], 8, 0, 1, 0, 0),
        b"v408" => (&[2, 1, 3, 7], &[0, 1, 2, 3], 8, 0, 1, 0, 0),
        b"y210" => (&[1, 2, 3], &[0, 1, 0, 2], 10, 1, 5, 2, 0x60),
        b"v410" => (&[2, 1, 3], &[0, 1, 2], 10, 0, 1, 4, 0x70),
        b"v210" => (&[2, 1, 3], &[0, 1, 2, 1], 10, 1, 5, 4, 0x30),
        b"i420" => (&[1, 2, 3], &[0, 1, 2], 8, 2, 0, 0, 0),
        b"nv12" => (&[1, 2, 3], &[0, 1, 2], 8, 2, 2, 0, 0),
        b"nv21" => (&[1, 3, 2], &[0, 1, 2], 8, 2, 2, 0, 0),
        b"yu22" => (&[1, 2, 3], &[0, 1, 2], 8, 1, 0, 0, 0),
        b"yv22" => (&[1, 3, 2], &[0, 1, 2], 8, 1, 0, 0, 0),
        b"yv20" => (&[1, 3, 2], &[0, 1, 2], 8, 2, 0, 0, 0),
        _ => return None,
    };
    Some(Profile {
        types,
        indices,
        bits,
        sampling,
        interleave,
        block,
        flags,
    })
}

impl crate::context::ImageInfo {
    pub fn cmpd_components(&self) -> Vec<Definition> {
        let properties = self
            .retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(p) = properties.iter().find(|p| p.kind == *b"cmpd" && !p.raw) {
            return definitions(&p.data, 0).unwrap_or_default();
        }
        let Some(p) = properties.iter().find(|p| p.kind == *b"uncC" && !p.raw) else {
            return Vec::new();
        };
        let Ok(mut config) = Configuration::parse(&p.data, None) else {
            return Vec::new();
        };
        let mut definitions = None;
        config.expand(&mut definitions);
        definitions.unwrap_or_default()
    }
}

#[path = "uncompressed_decode.rs"]
mod decoder;
pub use decoder::decode;

fn unspecified(message: impl Into<String>) -> ContextError {
    ContextError::invalid(0, &format!("Unspecified: {}", message.into()))
}
fn unsupported(message: impl Into<String>) -> ContextError {
    ContextError::new(
        4,
        3002,
        format!(
            "Unsupported feature: Unsupported data version: {}",
            message.into()
        ),
    )
}
impl Configuration {
    pub(crate) fn load(
        container: &crate::container::Container<'_>,
        id: u32,
    ) -> Result<(Self, Option<Vec<Definition>>)> {
        let data = container
            .property(id, *b"uncC")
            .map_err(|_| unspecified("No 'uncC' box found."))?;
        let mut config = Self::parse(data, None)?;
        let mut defs = container
            .property(id, *b"cmpd")
            .ok()
            .map(|p| definitions(p, 0))
            .transpose()?;
        config.expand(&mut defs);
        Ok((config, defs))
    }
    fn header(&self, defs: Option<&[Definition]>, size: Option<(u32, u32)>) -> Result<()> {
        let defs = defs.ok_or_else(|| {
            unsupported("Missing required cmpd or uncC version 1 box for uncompressed codec")
        })?;
        for c in &self.components {
            let d = defs
                .get(c.index as usize)
                .ok_or_else(|| unspecified("Invalid component index in uncC box"))?;
            if d.kind > 7 && !matches!(d.kind, 11 | 12) {
                return Err(unsupported(format!(
                    "Uncompressed image with component_type {} is not implemented yet",
                    d.kind
                )));
            }
        }
        if let Some((w, h)) = size {
            if self.columns > w || self.rows > h {
                return Err(unspecified("More tiles than pixels in uncC box"));
            }
            if w % self.columns != 0 || h % self.rows != 0 {
                return Err(unspecified(
                    "Invalid tile size (image size not a multiple of the tile size)",
                ));
            }
        }
        Ok(())
    }
    fn color(&self, defs: Option<&[Definition]>) -> Result<(i32, i32, bool)> {
        self.header(defs, None)?;
        if self.version == 1 {
            return match &self.profile {
                b"rgb3" => Ok((1, 3, false)),
                b"rgba" | b"abgr" => Ok((1, 3, true)),
                _ => Err(ContextError::new(
                    4,
                    3001,
                    "Unsupported feature: Unsupported image type: unci image has unsupported profile",
                )),
            };
        }
        let defs = defs.unwrap();
        let mut set = 0u32;
        for c in &self.components {
            let kind = defs[c.index as usize].kind;
            if kind != 12 {
                set |= 1 << kind;
            }
        }
        let alpha = set & (1 << 7) != 0;
        let color = match set & !(1 << 7) {
            0x70 => (1, 3),
            0x0e if !alpha => (
                0,
                match self.sampling {
                    0 => 3,
                    1 => 2,
                    2 => 1,
                    _ => 99,
                },
            ),
            1 | 2 => (2, 0),
            0x800 if !alpha => (4, 0),
            _ => return Err(unsupported("Could not determine colourspace")),
        };
        Ok((color.0, color.1, alpha))
    }
    fn depth(&self, defs: Option<&[Definition]>, chroma: bool) -> i32 {
        if chroma && self.version == 1 {
            return 8;
        }
        let Some(defs) = defs else {
            return -1;
        };
        let (mut primary, mut alternate) = (0, 0);
        for c in &self.components {
            let Some(d) = defs.get(c.index as usize) else {
                return -1;
            };
            if (chroma && matches!(d.kind, 2 | 3)) || (!chroma && d.kind == 1) {
                primary = primary.max(i32::from(c.bits));
            }
            if matches!(d.kind, 0 | 4 | 5 | 6 | 11) {
                alternate = alternate.max(i32::from(c.bits));
            }
        }
        if primary != 0 {
            primary
        } else if alternate != 0 {
            alternate
        } else {
            8
        }
    }
}
pub(crate) fn initialize(
    container: &crate::container::Container<'_>,
    image: &mut crate::context::ImageInfo,
) -> Result<()> {
    let (config, defs) = Configuration::load(container, image.id)?;
    let (cs, ch, alpha) = config.color(defs.as_deref()).unwrap_or((99, 99, false));
    if let Some(defs) = &defs {
        for c in &config.components {
            let Some(d) = defs.get(c.index as usize) else {
                continue;
            };
            let mut desc = crate::components::Description::reference(d.kind);
            desc.datatype = i32::from(c.format);
            desc.bit_depth = c.bits;
            desc.has_data = true;
            desc.width = if matches!(desc.channel, 1 | 2) && matches!(ch, 1 | 2) {
                image.ispe.0.div_ceil(2)
            } else {
                image.ispe.0
            };
            desc.height = if matches!(desc.channel, 1 | 2) && ch == 1 {
                image.ispe.1.div_ceil(2)
            } else {
                image.ispe.1
            };
            image.components.add(desc)?;
        }
    }
    container
        .property(image.id, *b"ispe")
        .map_err(|_| unspecified("No 'ispe' box found for uncompressed image item."))?;
    if let Some(c) = config.components.iter().find(|c| c.bits > 128) {
        return Err(ContextError::new(
            4,
            4000,
            format!(
                "Unsupported feature: Unsupported bit depth: Uncompressed image with {} bits per component is not supported.",
                c.bits
            ),
        ));
    }
    image.luma_bits = config.depth(defs.as_deref(), false);
    image.chroma_bits = config.depth(defs.as_deref(), true);
    image.has_alpha = alpha
        || (config.version != 1
            && config.header(defs.as_deref(), None).is_ok()
            && defs.as_ref().is_some_and(|ds| {
                config
                    .components
                    .iter()
                    .any(|c| ds[c.index as usize].kind == 7)
            }));
    (image.colorspace, image.chroma) = if config.version == 1 {
        (
            1,
            match &config.profile {
                b"rgb3" => 10,
                b"rgba" | b"abgr" => 11,
                _ => 99,
            },
        )
    } else {
        (cs, ch)
    };
    if defs.is_none() {
        image.description_error = Some(unspecified("Missing 'cmpd' box."));
    }
    Ok(())
}

impl crate::context::ImageInfo {
    /// These version-one profiles leave the caller's preferred chroma untouched.
    pub fn leaves_preferred_chroma_untouched(&self) -> bool {
        if self.kind != *b"unci" {
            return false;
        }
        let p = self
            .retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        p.iter()
            .find(|p| p.kind == *b"uncC" && !p.raw)
            .is_some_and(|p| {
                p.data.first() == Some(&1)
                    && !matches!(p.data.get(4..8), Some(b"rgb3" | b"rgba" | b"abgr"))
            })
    }
}

#[path = "uncompressed_compression.rs"]
pub(crate) mod compression;
