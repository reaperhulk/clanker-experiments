// SPDX-License-Identifier: LGPL-3.0-or-later
//! Owned image encoding and file-model insertion.
use crate::{
    color::Nclx,
    context::{Context, ContextError, Document, ImageInfo},
    handle_properties::Value,
    image::Image,
    properties::Property,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
};
type Result<T> = std::result::Result<T, ContextError>;
pub fn property(kind: [u8; 4], data: Vec<u8>) -> Property {
    Property {
        kind,
        uuid: None,
        data,
        raw: false,
        tai: None,
        write_error: None,
        gimi_components: None,
    }
}
#[derive(Clone, Default)]
pub struct Options {
    pub orientation: i32,
    pub nclx: Option<Nclx>,
    pub two_profiles: bool,
    pub no_nclx: bool,
}
pub fn description_properties(image: &Image) -> Vec<Property> {
    let mut result = Vec::new();
    let (h, v) = image.pixel_aspect_ratio;
    result.extend(Value::PixelAspect(h, v).property());
    result.extend(Value::ContentLight(image.color.content_light).property());
    result.extend(
        image
            .color
            .mastering
            .and_then(|v| Value::Mastering(v).property()),
    );
    result.extend(
        image
            .color
            .ambient
            .and_then(|v| Value::Ambient(v).property()),
    );
    result.extend(
        image
            .color
            .diffuse_white
            .and_then(|v| Value::DiffuseWhite(v).property()),
    );
    if let Some(t) = image.tai_timestamp {
        result.push(crate::tai::TaiProperty::Timestamp(t).property());
    }
    if !image.sample.content_id.is_empty() {
        let mut p = property(*b"uuid", image.sample.content_id.clone());
        p.uuid = Some(crate::gimi::CONTENT_UUID);
        result.push(p);
    }
    if image.projection != crate::omaf::FLAT {
        result.push(property(
            *b"prfr",
            vec![0, 0, 0, 0, image.projection as u8 & 31],
        ));
    }
    let ids: Vec<_> = image
        .component_ids
        .descriptions
        .iter()
        .map(|d| crate::gimi::c_string(&d.content_id))
        .collect();
    if ids.iter().any(|id| !id.is_empty()) {
        let mut p = property(*b"uuid", Vec::new());
        p.uuid = Some(crate::gimi::COMPONENT_UUID);
        p.gimi_components = Some(Arc::new(std::sync::Mutex::new(ids)));
        p.data = p.serialized_data();
        result.push(p);
    }
    result
}
impl Context {
    pub fn decoding_document(&self) -> Option<Arc<Document>> {
        self.document.as_ref().map(|doc| {
            if doc.owned.is_none() {
                return doc.clone();
            }
            let mut current = (**doc).clone();
            current.owned = Some((self.items.clone(), self.properties.clone()));
            Arc::new(current)
        })
    }
    pub fn encode_uncompressed(
        &mut self,
        image: &Image,
        options: &Options,
        compression: i32,
    ) -> Result<Arc<ImageInfo>> {
        let (data, properties) = crate::uncompressed_encode::encode(image, compression)?;
        self.insert_encoded(image, *b"unci", data, properties, options)
    }
    pub fn encode_mask(&mut self, image: &Image, options: &Options) -> Result<Arc<ImageInfo>> {
        if image.colorspace != 2 {
            return Err(ContextError::new(
                4,
                3002,
                "Unsupported feature: Unsupported data version: Unsupported colourspace for mask region",
            ));
        }
        let plane=image.plane(0).filter(|p|p.bit_depth==8).ok_or_else(||ContextError::new(4,3002,"Unsupported feature: Unsupported data version: Unsupported bit depth for mask region"))?;
        let len = (image.width as usize)
            .checked_mul(image.height as usize)
            .ok_or(crate::error::Error::ALLOCATION)?;
        let mut data = Vec::new();
        data.try_reserve_exact(len)
            .map_err(|_| crate::error::Error::ALLOCATION)?;
        for y in 0..image.height as usize {
            let row = plane
                .data()
                .get(y * plane.stride..y * plane.stride + image.width as usize)
                .ok_or(crate::error::Error::ALLOCATION)?;
            data.extend(row);
        }
        self.insert_encoded(
            image,
            *b"mski",
            data,
            vec![(property(*b"mskC", vec![0, 0, 0, 0, 8]), true)],
            options,
        )
    }
    pub(crate) fn insert_encoded(
        &mut self,
        image: &Image,
        kind: [u8; 4],
        data: Vec<u8>,
        properties: Vec<(Property, bool)>,
        options: &Options,
    ) -> Result<Arc<ImageInfo>> {
        self.insert_coded(
            image,
            kind,
            data,
            properties,
            options,
            (image.width, image.height),
        )
    }
    pub fn insert_coded(
        &mut self,
        image: &Image,
        kind: [u8; 4],
        data: Vec<u8>,
        mut properties: Vec<(Property, bool)>,
        options: &Options,
        encoded_size: (u32, u32),
    ) -> Result<Arc<ImageInfo>> {
        if encoded_size.0 < image.width || encoded_size.1 < image.height {
            return Err(ContextError::new(
                5,
                2006,
                "Usage error: Invalid parameter value: Clean aperture is larger than the image",
            ));
        }
        if encoded_size != (image.width, image.height)
            && (image.width > i32::MAX as u32
                || image.height > i32::MAX as u32
                || encoded_size.0 - image.width > (1u32 << 31)
                || encoded_size.1 - image.height > (1u32 << 31))
        {
            return Err(ContextError::new(
                5,
                2006,
                "Usage error: Invalid parameter value: Clean aperture values exceed the supported range",
            ));
        }
        if let Some(raw) = &image.color.raw {
            let mut data = raw.profile_type.to_be_bytes().to_vec();
            data.extend(&raw.data);
            properties.push((property(*b"colr", data), false));
        }
        if let Some(n) = options.nclx
            && !options.no_nclx
            && (image.color.raw.is_none() || options.two_profiles)
        {
            let mut data = b"nclx".to_vec();
            data.extend(n.primaries.to_be_bytes());
            data.extend(n.transfer.to_be_bytes());
            data.extend(n.matrix.to_be_bytes());
            data.push(u8::from(n.full_range) << 7);
            properties.push((property(*b"colr", data), false));
        }
        let mut ispe = vec![0; 4];
        ispe.extend(encoded_size.0.to_be_bytes());
        ispe.extend(encoded_size.1.to_be_bytes());
        properties.push((property(*b"ispe", ispe), matches!(&kind, b"mski" | b"unci")));
        if encoded_size != (image.width, image.height) {
            let mut clap = Vec::new();
            let offset = |delta: u32| {
                let value = -i64::from(delta);
                if delta > 65536 {
                    ((value / 2) as u32, 1)
                } else {
                    (value as u32, 2)
                }
            };
            let horizontal = offset(encoded_size.0 - image.width);
            let vertical = offset(encoded_size.1 - image.height);
            for n in [
                image.width,
                1,
                image.height,
                1,
                horizontal.0,
                horizontal.1,
                vertical.0,
                vertical.1,
            ] {
                clap.extend(n.to_be_bytes());
            }
            properties.push((property(*b"clap", clap), true));
        }
        let channels: &[i32] = match image.colorspace {
            2 => &[0],
            0 => &[0, 1, 2],
            1 if image.chroma == 3 => &[3, 4, 5],
            1 => &[10, 10, 10],
            _ => &[],
        };
        let bits: Vec<_> = channels
            .iter()
            .map(|&ch| image.plane(ch).map_or(0, |p| p.bit_depth))
            .collect();
        if !bits.is_empty() {
            let mut data = vec![0, 0, 0, 0, bits.len() as u8];
            data.extend(bits);
            properties.push((property(*b"pixi", data), false));
        }
        properties.extend(
            description_properties(image)
                .into_iter()
                .map(|p| (p, false)),
        );
        self.items.layout.lock().unwrap().init_image();
        let mut item = crate::items::Item::new(kind);
        item.hidden = false;
        let id = self.items.add(item, data)?;
        for (p, essential) in properties {
            self.properties.add_to_file(id, p, essential)?;
        }
        let (rotation, mirror) = match options.orientation {
            2 => (0, Some(1)),
            3 => (2, None),
            4 => (0, Some(0)),
            5 => (3, Some(1)),
            6 => (3, None),
            7 => (3, Some(0)),
            8 => (1, None),
            _ => (0, None),
        };
        if rotation != 0 {
            self.properties
                .add_to_file(id, property(*b"irot", vec![rotation]), true)?;
        }
        if let Some(mirror) = mirror {
            self.properties
                .add_to_file(id, property(*b"imir", vec![mirror]), true)?;
        }
        let retained = self
            .properties
            .items
            .get(&id)
            .into_iter()
            .flatten()
            .map(|&i| self.properties.boxes[i].clone())
            .collect();
        let mut info = ImageInfo::new(id, kind, encoded_size, retained);
        info.miaf = kind == *b"av01"
            || image.colorspace != 0
            || (!matches!(image.chroma, 1 | 2) || image.width.is_multiple_of(2))
                && (image.chroma != 1 || image.height.is_multiple_of(2));
        info.width = image.width;
        info.height = image.height;
        info.luma_bits = if kind == *b"mski" {
            8
        } else {
            image.plane(0).map_or(-1, |p| i32::from(p.bit_depth))
        };
        info.chroma_bits = 0;
        info.color = image.color.try_clone()?;
        info.pixel_aspect = Some(image.pixel_aspect_ratio);
        info.tai_timestamp = image.tai_timestamp;
        *info.projection.get_mut() = image.projection;
        *info.gimi_content_id.get_mut().unwrap() = image.sample.content_id.clone();
        if kind == *b"unci" {
            let container = crate::container::Container::from_stores(
                &self.items,
                &self.properties,
                id,
                *self.limits.read().unwrap(),
            )?;
            if let Err(error) = crate::uncompressed::initialize(&container, &mut info) {
                info.error = Some(error);
            }
        }
        Ok(self.register_image(info))
    }
    pub fn attach_encoded_alpha(
        &mut self,
        main: &ImageInfo,
        alpha: &ImageInfo,
        premultiplied: bool,
    ) -> Result<()> {
        self.items.add_reference(crate::items::Reference {
            from: alpha.id,
            kind: u32::from_be_bytes(*b"auxl"),
            to: vec![main.id],
        });
        let p = property(
            *b"auxC",
            [
                &[0u8; 4][..],
                if main.kind == *b"hvc1" {
                    b"urn:mpeg:hevc:2015:auxid:1\0".as_slice()
                } else {
                    b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha\0".as_slice()
                },
            ]
            .concat(),
        );
        self.properties.add_to_file(alpha.id, p.clone(), true)?;
        alpha.retained_properties.lock().unwrap().push(Arc::new(p));
        if premultiplied {
            self.items.add_reference(crate::items::Reference {
                from: main.id,
                kind: u32::from_be_bytes(*b"prem"),
                to: vec![alpha.id],
            });
        }
        Ok(())
    }
    pub(crate) fn register_image(&mut self, info: ImageInfo) -> Arc<ImageInfo> {
        let id = info.id;
        let info = Arc::new(info);
        let mut doc = self
            .document
            .as_deref()
            .cloned()
            .unwrap_or_else(|| Document {
                owned: None,
                budget: self.budget.clone(),
                read_limits: *self.limits.read().unwrap(),
                limits: self.limits.clone(),
                input: Arc::new(Vec::<u8>::new()),
                images: BTreeMap::new(),
                primary: 0,
                top_level: Vec::new(),
            });
        doc.images.insert(id, info.clone());
        doc.owned = Some((self.items.clone(), self.properties.clone()));
        self.document = Some(Arc::new(doc));
        info
    }
    pub fn set_primary(&mut self, image: Arc<ImageInfo>) {
        if let Some(doc) = &self.document
            && let Some(old) = doc.images.get(&doc.primary)
        {
            old.primary.store(false, Ordering::Relaxed);
        }
        image.primary.store(true, Ordering::Relaxed);
        self.items.layout.lock().unwrap().primary = image.id;
        if let Some(doc) = &self.document {
            let mut next = (**doc).clone();
            next.primary = image.id;
            next.images.entry(image.id).or_insert(image);
            self.document = Some(Arc::new(next));
        }
    }
}
impl Context {
    pub fn add_overlay(
        &mut self,
        width: u32,
        height: u32,
        ids: &[u32],
        offsets: Vec<(i32, i32)>,
        background: [u16; 4],
    ) -> Result<Arc<ImageInfo>> {
        for id in ids {
            let item = self.items.items.get_mut(id).ok_or_else(|| {
                ContextError::new(5, 2000, "Usage error: Non-existing item ID referenced")
            })?;
            item.hidden = true;
        }
        let wide = width > 65535
            || height > 65535
            || offsets
                .iter()
                .any(|&(x, y)| !(-32768..=32767).contains(&x) || !(-32768..=32767).contains(&y));
        let mut data = vec![0, u8::from(wide)];
        for n in background {
            data.extend(n.to_be_bytes());
        }
        let n = if wide { 4 } else { 2 };
        for v in [width, height] {
            crate::writing::number(&mut data, u64::from(v), n);
        }
        for &(x, y) in &offsets {
            for v in [x, y] {
                crate::writing::number(&mut data, u64::from(v as u32), n);
            }
        }
        self.items.layout.lock().unwrap().init_image();
        let mut item = crate::items::Item::new(*b"iovl");
        item.hidden = false;
        let id = self.items.add_pending(item)?;
        self.items.append_written(id, 1, data)?;
        self.items.add_reference(crate::items::Reference {
            from: id,
            kind: u32::from_be_bytes(*b"dimg"),
            to: ids.to_vec(),
        });
        let mut ispe = vec![0; 4];
        ispe.extend(width.to_be_bytes());
        ispe.extend(height.to_be_bytes());
        self.properties
            .add_to_file(id, property(*b"ispe", ispe), false)?;
        if let Some(pixi) = self
            .properties
            .get(ids[0])?
            .into_iter()
            .find(|p| p.kind == *b"pixi")
            .cloned()
        {
            self.properties.add_to_file(id, pixi, true)?;
        }
        let retained = self
            .properties
            .items
            .get(&id)
            .into_iter()
            .flatten()
            .map(|&i| self.properties.boxes[i].clone())
            .collect();
        let mut info = ImageInfo::new(id, *b"iovl", (width, height), retained);
        info.colorspace = 1;
        info.chroma = 3;
        if let Some(first) = self
            .document
            .as_ref()
            .and_then(|doc| doc.images.get(&ids[0]))
        {
            info.luma_bits = first.luma_bits;
            info.chroma_bits = first.chroma_bits;
        }
        // The object retains zero display dimensions until it is interpreted on read-back.
        // Decode uses the serialized overlay payload, including its signed-field semantics.
        let container = crate::container::Container::from_stores(
            &self.items,
            &self.properties,
            id,
            *self.limits.read().unwrap(),
        )?;
        info.overlay = crate::overlay::Overlay::load(&container, id).ok();
        Ok(self.register_image(info))
    }
}
