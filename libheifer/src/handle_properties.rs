// SPDX-License-Identifier: LGPL-3.0-or-later
//! Image-owned properties are retained independently of the context's file tables.
use crate::{
    color::{AmbientViewingEnvironment, ContentLightLevel, MasteringDisplayColourVolume},
    context::{Context, ContextError, ImageInfo},
    properties::Property,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub enum Value {
    ContentLight(ContentLightLevel),
    Mastering(MasteringDisplayColourVolume),
    Ambient(AmbientViewingEnvironment),
    DiffuseWhite(u32),
    PixelAspect(u32, u32),
}
impl Value {
    pub fn property(self) -> Option<Property> {
        let mut data = Vec::new();
        let kind = match self {
            Self::ContentLight(v) => {
                if v.max_content_light_level == 0 && v.max_pic_average_light_level == 0 {
                    return None;
                }
                data.extend(v.max_content_light_level.to_be_bytes());
                data.extend(v.max_pic_average_light_level.to_be_bytes());
                *b"clli"
            }
            Self::Mastering(v) => {
                for i in 0..3 {
                    data.extend(v.display_primaries_x[i].to_be_bytes());
                    data.extend(v.display_primaries_y[i].to_be_bytes());
                }
                data.extend(v.white_point_x.to_be_bytes());
                data.extend(v.white_point_y.to_be_bytes());
                data.extend(v.max_display_mastering_luminance.to_be_bytes());
                data.extend(v.min_display_mastering_luminance.to_be_bytes());
                *b"mdcv"
            }
            Self::Ambient(v) => {
                data.extend(v.ambient_illumination.to_be_bytes());
                data.extend(v.ambient_light_x.to_be_bytes());
                data.extend(v.ambient_light_y.to_be_bytes());
                *b"amve"
            }
            Self::DiffuseWhite(v) => {
                data.extend([0; 4]);
                data.extend(v.to_be_bytes());
                *b"ndwt"
            }
            Self::PixelAspect(h, v) => {
                if h == v {
                    return None;
                }
                data.extend(h.to_be_bytes());
                data.extend(v.to_be_bytes());
                *b"pasp"
            }
        };
        Some(Property {
            kind,
            uuid: None,
            data,
            raw: false,
            tai: None,
            write_error: None,
            gimi_components: None,
        })
    }
    fn parse(p: &Property) -> Option<Self> {
        if p.raw {
            return None;
        }
        let d = &p.data;
        let u16_at = |i| u16::from_be_bytes([d[i], d[i + 1]]);
        let u32_at = |i| u32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]);
        match &p.kind {
            b"clli" if d.len() >= 4 => Some(Self::ContentLight(ContentLightLevel {
                max_content_light_level: u16_at(0),
                max_pic_average_light_level: u16_at(2),
            })),
            b"mdcv" if d.len() >= 24 => Some(Self::Mastering(MasteringDisplayColourVolume {
                display_primaries_x: std::array::from_fn(|i| u16_at(i * 4)),
                display_primaries_y: std::array::from_fn(|i| u16_at(i * 4 + 2)),
                white_point_x: u16_at(12),
                white_point_y: u16_at(14),
                max_display_mastering_luminance: u32_at(16),
                min_display_mastering_luminance: u32_at(20),
            })),
            b"amve" if d.len() >= 8 => Some(Self::Ambient(AmbientViewingEnvironment {
                ambient_illumination: u32_at(0),
                ambient_light_x: u16_at(4),
                ambient_light_y: u16_at(6),
            })),
            b"ndwt" if d.len() >= 8 && d[0] == 0 => Some(Self::DiffuseWhite(u32_at(4))),
            b"pasp" if d.len() >= 8 => Some(Self::PixelAspect(u32_at(0), u32_at(4))),
            _ => None,
        }
    }
}
impl ImageInfo {
    pub fn handle_property(&self, kind: [u8; 4]) -> Option<Value> {
        self.retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|p| !p.raw && p.kind == kind)
            .and_then(|p| Value::parse(p))
    }
    pub fn attach_handle_property(
        &self,
        context: &mut Context,
        value: Value,
    ) -> Result<(), ContextError> {
        let Some(property) = value.property() else {
            return Ok(());
        };
        // Native ImageItem keeps the new object even if the file deduplicates it.
        self.retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::new(property.clone()));
        context.properties.add_to_file(self.id, property, false)?;
        Ok(())
    }
    pub fn apply_handle_properties(&self, image: &mut crate::image::Image) {
        for kind in [*b"clli", *b"mdcv", *b"amve", *b"ndwt", *b"pasp"] {
            match self.handle_property(kind) {
                Some(Value::ContentLight(v)) => image.color.content_light = v,
                Some(Value::Mastering(v)) => image.color.mastering = Some(v),
                Some(Value::Ambient(v)) => image.color.ambient = Some(v),
                Some(Value::DiffuseWhite(v)) => image.color.diffuse_white = Some(v),
                Some(Value::PixelAspect(h, v)) => image.pixel_aspect_ratio = (h, v),
                None => {}
            }
        }
    }
}
