// SPDX-License-Identifier: LGPL-3.0-or-later
//! Item decoding and container metadata orchestration.
use crate::{
    color::Nclx,
    context::{ContextError, Document},
    image::Image,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub struct DecodeOptions {
    pub ignore_transformations: bool,
    pub strict: bool,
    pub output_nclx: Option<Nclx>,
    pub profile_passthrough: bool,
    pub convert_hdr_to_8bit: bool,
    pub color_conversion: crate::color::ColorConversionOptions,
}
impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            ignore_transformations: false,
            strict: false,
            output_nclx: None,
            profile_passthrough: false,
            convert_hdr_to_8bit: false,
            color_conversion: crate::color::ColorConversionOptions {
                only_use_preferred_chroma_algorithm: 0,
                ..Default::default()
            },
        }
    }
}
impl From<crate::error::Error> for ContextError {
    fn from(e: crate::error::Error) -> Self {
        Self::new(e.code, e.subcode, e.message.to_string_lossy())
    }
}
pub fn unsupported_conversion() -> ContextError {
    ContextError::new(4, 3003, "Unsupported feature: Unsupported color conversion")
}

pub fn decode(
    document: &Document,
    id: u32,
    colorspace: i32,
    chroma: i32,
    options: DecodeOptions,
) -> Result<Image, ContextError> {
    let mut visiting = BTreeSet::new();
    let image = decode_native(document, id, &options, &mut visiting)?;
    let target_cs = if colorspace == 99 {
        image.colorspace
    } else {
        colorspace
    };
    let target_chroma = if chroma == 99 { image.chroma } else { chroma };
    let requested = options.output_nclx.or({
        if options.profile_passthrough {
            image.color.nclx
        } else {
            Some(Nclx {
                primaries: 1,
                transfer: 13,
                matrix: 6,
                full_range: true,
            })
        }
    });
    if target_cs != image.colorspace
        || target_chroma != image.chroma
        || (options.convert_hdr_to_8bit && image.plane(0).is_some_and(|p| p.bit_depth > 8))
        || requested.zip(image.color.nclx).is_some_and(|(a, b)| a != b)
    {
        return crate::conversion::convert(
            image,
            target_cs,
            target_chroma,
            requested.unwrap(),
            if options.convert_hdr_to_8bit { 8 } else { 0 },
            options.color_conversion,
        )
        .map_err(Into::into);
    }
    Ok(image)
}

fn decode_native(
    document: &Document,
    id: u32,
    options: &DecodeOptions,
    visiting: &mut BTreeSet<u32>,
) -> Result<Image, ContextError> {
    if !visiting.insert(id) {
        return Err(ContextError::invalid(
            0,
            "Unspecified: 'iref' has cyclic references",
        ));
    }
    let info = document
        .images
        .get(&id)
        .ok_or_else(|| ContextError::invalid(2000, "Non-existing item ID referenced"))?;
    if let Some(error) = &info.error {
        return Err(error.clone());
    }
    let container = document.container()?;
    let mut image = crate::hevc::decode_item(&container, id).map_err(|e| match e {
        crate::hevc::DecodeError::Container(e) => e.into(),
        crate::hevc::DecodeError::Image(e) => e.into(),
        e => ContextError::new(
            7,
            0,
            format!("Decoder plugin generated an error: Unspecified: {e}"),
        ),
    })?;
    if !options.ignore_transformations {
        for (kind, p) in container.properties(id)? {
            match &kind {
                b"irot" if p.first().is_some_and(|a| a & 3 != 0) => {
                    image = image.rotate(p[0] & 3)?;
                }
                b"imir" if !p.is_empty() => {
                    image.mirror(p[0] & 1 == 0)?;
                }
                b"clap" | b"iscl" => {
                    return Err(ContextError::new(
                        4,
                        0,
                        "Unsupported feature: Unspecified: Transform not implemented",
                    ));
                }
                _ => {}
            }
        }
    }
    if info.has_alpha {
        for item in container.items.values() {
            if !item
                .references
                .get(b"auxl")
                .is_some_and(|ids| ids.contains(&id))
            {
                continue;
            }
            let Ok(aux) = container.property(item.id, *b"auxC") else {
                continue;
            };
            let aux_type = aux
                .get(4..)
                .unwrap_or_default()
                .split(|b| *b == 0)
                .next()
                .unwrap_or_default();
            if !matches!(
                aux_type,
                b"urn:mpeg:avc:2015:auxid:1"
                    | b"urn:mpeg:hevc:2015:auxid:1"
                    | b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha"
            ) {
                continue;
            }
            let mut alpha = decode_native(document, item.id, options, visiting)?;
            if alpha.width != image.width || alpha.height != image.height {
                alpha = alpha.scale(image.width, image.height)?;
            }
            image.transfer_plane(&mut alpha, 0, 6)?;
            break;
        }
    }
    let bitstream_nclx = image.color.nclx;
    image.color = info.color.try_clone()?;
    if image.color.nclx.is_none_or(|n| !n.is_defined()) {
        image.color.nclx = bitstream_nclx;
    }
    image.pixel_aspect_ratio = info.pixel_aspect.unwrap_or((1, 1));
    image.premultiplied_alpha = info.premultiplied_alpha;
    visiting.remove(&id);
    Ok(image)
}
