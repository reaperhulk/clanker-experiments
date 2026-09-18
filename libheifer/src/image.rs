// SPDX-License-Identifier: LGPL-3.0-or-later
// Layout/validation semantics adapted from libheif, Copyright Dirk Farin and contributors.
use crate::error::Error;

#[derive(Debug)]
pub struct Plane {
    pub component_ids: Vec<u32>,
    pub datatype: i32,
    pub channel: i32,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub bytes_per_pixel: usize,
    pub stride: usize,
    storage: Vec<u8>,
    offset: usize,
    _reservation: Option<crate::security::Reservation>,
}

impl Plane {
    fn new(
        channel: i32,
        width: u32,
        height: u32,
        depth: i32,
        components: usize,
        budget: Option<&std::sync::Arc<crate::security::Budget>>,
    ) -> Result<Self, Error> {
        if !(1..=128).contains(&depth) {
            return Err(Error::new(
                5,
                0,
                c"Usage error: Unspecified: Invalid bit depth",
            ));
        }
        if width == 0 || height == 0 {
            return Err(Error::new(
                5,
                0,
                c"Usage error: Unspecified: Invalid image size",
            ));
        }
        if width == u32::MAX || height == u32::MAX {
            return Err(Error::new(6, 1000, c"Memory allocation error: Security limit exceeded: Image size too large for memory alignment"));
        }
        let bytes = ((depth as usize).next_power_of_two() / 8).max(1);
        let bytes_per_pixel = bytes * components;
        let mem_width = ((u64::from(width) + 1) & !1).max(64);
        let mem_height = ((u64::from(height) + 1) & !1).max(64);
        let stride =
            usize::try_from((mem_width * bytes_per_pixel as u64 + 15) & !15).map_err(|_| {
                Error::new(
                    6,
                    1000,
                    c"Memory allocation error: Security limit exceeded: Image stride overflow",
                )
            })?;
        let allocation = usize::try_from(mem_height)
            .ok()
            .and_then(|h| h.checked_mul(stride))
            .and_then(|n| n.checked_add(15))
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or(Error::new(
                6,
                1000,
                c"Memory allocation error: Security limit exceeded: Image allocation size overflow",
            ))?;
        if let Some(budget) = budget {
            let maximum = budget
                .limits
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .max_image_size_pixels;
            if maximum != 0 && u64::from(width) * u64::from(height) > maximum {
                return Err(Error::owned(
                    6,
                    1000,
                    format!(
                        "Memory allocation error: Security limit exceeded: Allocating an image of size {width}x{height} exceeds the security limit of {maximum} pixels"
                    ),
                ));
            }
        }
        let reservation = budget
            .map(|b| b.reserve(allocation as u64, "image data"))
            .transpose()?;
        let mut storage = Vec::new();
        storage
            .try_reserve_exact(allocation)
            .map_err(|_| Error::ALLOCATION)?;
        storage.resize(allocation, 0);
        let offset = (16 - (storage.as_ptr() as usize & 15)) & 15;
        Ok(Self {
            _reservation: reservation,
            component_ids: Vec::new(),
            datatype: 0,
            channel,
            width,
            height,
            bit_depth: depth as u8,
            bytes_per_pixel,
            stride,
            storage,
            offset,
        })
    }

    pub fn data(&self) -> &[u8] {
        &self.storage[self.offset..]
    }
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.storage[self.offset..]
    }
    pub fn storage_bits(&self) -> i32 {
        // v1.23.4 narrows to uint8_t, even for 256/512-bit custom layouts.
        let bits = (self.bytes_per_pixel * 8) as u8;
        if bits == 0 { -1 } else { i32::from(bits) }
    }
}

#[derive(Clone, Debug)]
pub struct DecodingWarning {
    pub code: i32,
    pub subcode: i32,
    pub message: std::ffi::CString,
}
impl From<crate::context::ContextError> for DecodingWarning {
    fn from(error: crate::context::ContextError) -> Self {
        let text = error.message.split('\0').next().unwrap_or_default();
        Self {
            code: error.code,
            subcode: error.subcode,
            message: std::ffi::CString::new(text).unwrap(),
        }
    }
}

#[derive(Debug)]
pub struct Image {
    pub sensor: crate::sensor::SensorMetadata,
    pub component_ids: crate::components::ComponentIds,
    pub budget: Option<std::sync::Arc<crate::security::Budget>>,
    pub last_error: std::sync::Mutex<std::ffi::CString>,
    pub color: crate::color::ColorMetadata,
    pub warnings: Vec<DecodingWarning>,
    pub width: u32,
    pub height: u32,
    pub colorspace: i32,
    pub chroma: i32,
    pub premultiplied_alpha: bool,
    pub pixel_aspect_ratio: (u32, u32),
    planes: Vec<Plane>,
}

impl Image {
    pub fn with_budget(mut self, budget: Option<std::sync::Arc<crate::security::Budget>>) -> Self {
        self.budget = budget;
        self
    }
    pub fn new(
        width: u32,
        height: u32,
        mut colorspace: i32,
        mut chroma: i32,
    ) -> Result<Self, Error> {
        if colorspace == 0 && chroma == 0 {
            colorspace = 2;
        }
        let valid = match colorspace {
            0 => matches!(chroma, 1..=3),
            1 => matches!(chroma, 0 | 3 | 10..=15),
            2..=4 => chroma == 0,
            _ => false,
        };
        if !valid {
            return Err(Error::new(
                5,
                2006,
                c"Invalid colorspace/chroma combination.",
            ));
        }
        if colorspace == 1 && chroma == 0 {
            chroma = 3;
        }
        Ok(Self {
            sensor: crate::sensor::SensorMetadata::default(),
            component_ids: crate::components::ComponentIds::default(),
            budget: None,
            last_error: std::sync::Mutex::new(std::ffi::CString::default()),
            width,
            height,
            colorspace,
            chroma,
            color: crate::color::ColorMetadata::default(),
            warnings: Vec::new(),
            premultiplied_alpha: false,
            pixel_aspect_ratio: (1, 1),
            planes: Vec::new(),
        })
    }

    pub fn add_plane(
        &mut self,
        channel: i32,
        width: u32,
        height: u32,
        mut bit_depth: i32,
    ) -> Result<(), Error> {
        if (self.chroma == 10 && bit_depth == 24) || (self.chroma == 11 && bit_depth == 32) {
            bit_depth = 8;
        }
        if matches!(self.chroma, 12..=15) && bit_depth <= 8 {
            return Err(Error::new(5, 0, c"Usage error: Unspecified: Cannot create a 16-bit interleaved channel with a bit depth of 8 or less"));
        }
        let components = match self.chroma {
            10 | 12 | 14 => 3,
            11 | 13 | 15 => 4,
            _ => 1,
        };
        let mut plane = Plane::new(
            channel,
            width,
            height,
            bit_depth,
            components,
            self.budget.as_ref(),
        )?;
        let descriptions = crate::components::types_for_channel(channel, self.chroma)
            .into_iter()
            .map(crate::components::Description::reference)
            .collect::<Vec<_>>();
        self.register_plane(&mut plane, descriptions)?;
        self.planes.push(plane);
        Ok(())
    }
    fn register_plane(
        &mut self,
        plane: &mut Plane,
        descriptions: Vec<crate::components::Description>,
    ) -> Result<(), Error> {
        self.planes.try_reserve(1).map_err(|_| Error::ALLOCATION)?;
        self.component_ids
            .descriptions
            .try_reserve(descriptions.len())
            .map_err(|_| Error::ALLOCATION)?;
        plane
            .component_ids
            .try_reserve(descriptions.len())
            .map_err(|_| Error::ALLOCATION)?;
        for mut description in descriptions {
            description.channel = plane.channel;
            description.datatype = plane.datatype;
            description.bit_depth = u16::from(plane.bit_depth);
            description.width = plane.width;
            description.height = plane.height;
            description.has_data = true;
            plane
                .component_ids
                .push(self.component_ids.add(description)?);
        }
        // Caller owns the buffer until registration succeeds.
        Ok(())
    }
    pub fn add_component(
        &mut self,
        width: u32,
        height: u32,
        kind: u16,
        datatype: i32,
        bit_depth: i32,
    ) -> Result<u32, Error> {
        let mut plane = Plane::new(
            crate::components::channel_for_type(kind),
            width,
            height,
            bit_depth,
            1,
            self.budget.as_ref(),
        )?;
        plane.datatype = datatype;
        self.register_plane(
            &mut plane,
            vec![crate::components::Description::reference(kind)],
        )?;
        let id = plane.component_ids[0];
        self.planes.push(plane);
        Ok(id)
    }
    pub fn component_plane(&self, id: u32) -> Option<&Plane> {
        self.planes.iter().find(|p| p.component_ids.contains(&id))
    }
    pub fn component_plane_mut(&mut self, id: u32) -> Option<&mut Plane> {
        self.planes
            .iter_mut()
            .find(|p| p.component_ids.contains(&id))
    }
    fn add_cloned_plane(
        &mut self,
        source: &Plane,
        descriptions: &crate::components::ComponentIds,
        width: u32,
        height: u32,
    ) -> Result<(), Error> {
        let bytes = (usize::from(source.bit_depth).next_power_of_two() / 8).max(1);
        let mut plane = Plane::new(
            source.channel,
            width,
            height,
            source.bit_depth.into(),
            source.bytes_per_pixel / bytes,
            self.budget.as_ref(),
        )?;
        plane.datatype = source.datatype;
        let descriptions = source
            .component_ids
            .iter()
            .filter_map(|id| descriptions.find(*id).cloned())
            .collect();
        self.register_plane(&mut plane, descriptions)?;
        self.planes.push(plane);
        Ok(())
    }
    pub fn plane(&self, channel: i32) -> Option<&Plane> {
        self.planes.iter().find(|p| p.channel == channel)
    }
    pub fn plane_mut(&mut self, channel: i32) -> Option<&mut Plane> {
        self.planes.iter_mut().find(|p| p.channel == channel)
    }
    pub fn channels(&self) -> impl Iterator<Item = i32> + '_ {
        self.planes.iter().map(|p| p.channel)
    }
    pub fn transfer_plane(&mut self, source: &mut Self, from: i32, to: i32) -> Result<(), Error> {
        let index = source
            .planes
            .iter()
            .position(|p| p.channel == from)
            .ok_or(Error::new(5, 2001, c"No such channel"))?;
        self.planes.try_reserve(1).map_err(|_| Error::ALLOCATION)?;
        let mut plane = source.planes.remove(index);
        let ids = std::mem::take(&mut plane.component_ids);
        for old_id in ids {
            if let Some(index) = source
                .component_ids
                .descriptions
                .iter()
                .position(|d| d.id == old_id)
            {
                let mut desc = source.component_ids.descriptions.remove(index);
                desc.channel = to;
                desc.kind = crate::components::types_for_channel(to, 99)[0];
                plane.component_ids.push(self.component_ids.add(desc)?);
            }
        }
        plane.channel = to;
        self.planes.push(plane);
        Ok(())
    }
    fn subsampling(&self, channel: i32) -> (u32, u32) {
        if matches!(channel, 1 | 2) {
            match self.chroma {
                1 => (2, 2),
                2 => (2, 1),
                _ => (1, 1),
            }
        } else {
            (1, 1)
        }
    }
    fn standard_planes(&self) -> bool {
        self.planes.iter().all(|p| {
            if !matches!(p.channel, 0..=6 | 10..=13) {
                return false;
            }
            let (sx, sy) = self.subsampling(p.channel);
            p.width == self.width.div_ceil(sx) && p.height == self.height.div_ceil(sy)
        })
    }
    pub fn scale(&self, width: u32, height: u32) -> Result<Self, Error> {
        self.scale_with_budget(width, height, self.budget.clone())
    }
    pub fn scale_with_budget(
        &self,
        width: u32,
        height: u32,
        budget: Option<std::sync::Arc<crate::security::Budget>>,
    ) -> Result<Self, Error> {
        if !self.standard_planes() {
            return Err(Error::new(4,0,c"Unsupported feature: Unspecified: Scaling an image with non-standard plane sizes is not supported"));
        }
        let mut out = Self::new(width, height, self.colorspace, self.chroma)?.with_budget(budget);
        let mut channels = if self.plane(10).is_some() {
            vec![10]
        } else {
            match self.colorspace {
                0 => vec![0, 1, 2],
                1 => vec![3, 4, 5],
                2 => vec![0],
                _ => {
                    return Err(Error::new(
                        2,
                        0,
                        c"Invalid input: Unspecified: unknown color configuration",
                    ));
                }
            }
        };
        for &channel in &channels {
            if self.plane(channel).is_none() {
                return Err(match self.colorspace {
                    0 => Error::new(
                        2,
                        0,
                        c"Invalid input: Unspecified: YCbCr image without Y,Cb,Cr planes",
                    ),
                    1 => Error::new(
                        2,
                        0,
                        c"Invalid input: Unspecified: RGB input without R,G,B, planes",
                    ),
                    _ => Error::new(
                        2,
                        0,
                        c"Invalid input: Unspecified: monochrome input with no Y plane",
                    ),
                });
            }
        }
        if channels[0] != 10 && self.plane(6).is_some() {
            channels.push(6);
        }
        for &channel in &channels {
            let source = self.plane(channel).unwrap();
            let (sx, sy) = self.subsampling(channel);
            out.add_plane(
                channel,
                width.div_ceil(sx),
                height.div_ceil(sy),
                source.bit_depth.into(),
            )?;
        }
        if self.planes.len() > out.planes.len() {
            return Err(Error::new(4,0,c"Unsupported feature: Unspecified: Images with extra planes are not supported by scale_nearest_neighbor()."));
        }
        if self.chroma >= 10 && self.plane(10).is_none() {
            return Err(Error::new(2,0,c"Invalid input: Unspecified: Interleaved chroma format without an interleaved plane"));
        }
        for &channel in &channels {
            let source = self.plane(channel).unwrap();
            let dest = out.plane_mut(channel).unwrap();
            let components = if channel == 10 {
                match self.chroma {
                    11 | 13 | 15 => 4,
                    _ => 3,
                }
            } else {
                1
            };
            let bytes = if source.bit_depth <= 8 { 1 } else { 2 } * components;
            for y in 0..dest.height as usize {
                let iy = (y as u64 * u64::from(self.height) / u64::from(height)) as usize;
                for x in 0..dest.width as usize {
                    let ix = (x as u64 * u64::from(self.width) / u64::from(width)) as usize;
                    let from = iy * source.stride + ix * bytes;
                    let to = y * dest.stride + x * bytes;
                    dest.data_mut()[to..to + bytes]
                        .copy_from_slice(&source.data()[from..from + bytes]);
                }
            }
        }
        Ok(out)
    }
    fn as_ycbcr444(&self) -> Result<Self, Error> {
        crate::conversion::convert(
            self.rotate(0)?,
            0,
            3,
            crate::color::Nclx {
                primaries: 2,
                transfer: 2,
                matrix: 2,
                full_range: true,
            },
            0,
            crate::color::ColorConversionOptions::default(),
        )
    }
    /// Allocate a grid canvas using this image's plane layout and metadata.
    pub fn empty_canvas(&self, width: u32, height: u32) -> Result<Self, Error> {
        let mut out = Self::new(width, height, self.colorspace, self.chroma)?
            .with_budget(self.budget.clone());
        out.color = self.color.try_clone()?;
        out.sensor = self.sensor.clone();
        out.pixel_aspect_ratio = self.pixel_aspect_ratio;
        out.premultiplied_alpha = self.premultiplied_alpha;
        for p in &self.planes {
            let (sx, sy) = self.subsampling(p.channel);
            out.add_plane(
                p.channel,
                width.div_ceil(sx),
                height.div_ceil(sy),
                p.bit_depth.into(),
            )?;
            if p.channel == 6 {
                let dest = out.plane_mut(6).unwrap();
                let max = (1u32 << p.bit_depth.min(16)) - 1;
                for y in 0..dest.height as usize {
                    for x in 0..dest.width as usize {
                        let at = y * dest.stride + x * dest.bytes_per_pixel;
                        if p.bit_depth <= 8 {
                            dest.data_mut()[at] = max as u8;
                        } else {
                            dest.data_mut()[at..at + 2]
                                .copy_from_slice(&(max as u16).to_ne_bytes());
                        }
                    }
                }
            }
        }
        Ok(out)
    }
    pub fn paste(&mut self, source: &Self, x: u32, y: u32) -> Result<(), Error> {
        for p in &source.planes {
            let (sx, sy) = self.subsampling(p.channel);
            let (x, y) = (x / sx, y / sy);
            let Some(dest) = self.plane_mut(p.channel) else {
                continue;
            };
            if dest.bytes_per_pixel != p.bytes_per_pixel {
                return Err(Error::new(
                    4,
                    3003,
                    c"Unsupported feature: Unsupported color conversion",
                ));
            }
            if x >= dest.width || y >= dest.height {
                continue;
            }
            let width = p.width.min(dest.width - x);
            let height = p.height.min(dest.height - y);
            for row in 0..height as usize {
                let from = row * p.stride;
                let to = (y as usize + row) * dest.stride + x as usize * p.bytes_per_pixel;
                let len = width as usize * p.bytes_per_pixel;
                dest.data_mut()[to..to + len].copy_from_slice(&p.data()[from..from + len]);
            }
        }
        Ok(())
    }
    pub fn crop(&self, left: u32, right: u32, top: u32, bottom: u32) -> Result<Self, Error> {
        if right < left || bottom < top || right >= self.width || bottom >= self.height {
            return Err(Error::new(
                5,
                2006,
                c"Usage error: Invalid parameter value: Invalid crop region",
            ));
        }
        if !self.standard_planes() {
            return Err(Error::new(4,0,c"Unsupported feature: Unspecified: Cropping an image with non-standard plane sizes is not supported"));
        }
        if (self.chroma == 2 && left % 2 == 1)
            || (self.chroma == 1 && (left % 2 == 1 || top % 2 == 1))
        {
            return self.as_ycbcr444()?.crop(left, right, top, bottom);
        }
        let mut out = Self::new(
            right - left + 1,
            bottom - top + 1,
            self.colorspace,
            self.chroma,
        )?
        .with_budget(self.budget.clone());
        out.warnings = self.warnings.clone();
        out.color = self.color.try_clone()?;
        out.sensor = self.sensor.clone();
        out.pixel_aspect_ratio = self.pixel_aspect_ratio;
        out.premultiplied_alpha = self.premultiplied_alpha;
        for source in &self.planes {
            let (sx, sy) = self.subsampling(source.channel);
            let (x, y, w, h) = (
                left / sx,
                top / sy,
                right / sx - left / sx + 1,
                bottom / sy - top / sy + 1,
            );
            out.add_cloned_plane(source, &self.component_ids, w, h)?;
            let dest = out.planes.last_mut().unwrap();
            for row in 0..h as usize {
                let from = (y as usize + row) * source.stride + x as usize * source.bytes_per_pixel;
                let to = row * dest.stride;
                let bytes = w as usize * source.bytes_per_pixel;
                dest.data_mut()[to..to + bytes].copy_from_slice(&source.data()[from..from + bytes]);
            }
        }
        Ok(out)
    }
    pub fn rotate(&self, quarters: u8) -> Result<Self, Error> {
        let quarters = quarters & 3;
        let need_conversion = match self.chroma {
            2 => quarters == 1 || quarters == 3 || (quarters == 2 && self.height % 2 == 1),
            1 => match quarters {
                1 => self.width % 2 == 1,
                2 => self.width % 2 == 1 || self.height % 2 == 1,
                3 => self.height % 2 == 1,
                _ => false,
            },
            _ => false,
        };
        if need_conversion {
            return self.as_ycbcr444()?.rotate(quarters);
        }
        let odd = quarters & 1 != 0;
        let width = if odd { self.height } else { self.width };
        let height = if odd { self.width } else { self.height };
        let mut out = Self::new(width, height, self.colorspace, self.chroma)?
            .with_budget(self.budget.clone());
        out.warnings = self.warnings.clone();
        out.color = self.color.try_clone()?;
        out.sensor = self.sensor.clone();
        out.pixel_aspect_ratio = self.pixel_aspect_ratio;
        out.premultiplied_alpha = self.premultiplied_alpha;
        for source in &self.planes {
            let (w, h) = if odd {
                (source.height, source.width)
            } else {
                (source.width, source.height)
            };
            out.add_cloned_plane(source, &self.component_ids, w, h)?;
            let dest = out.planes.last_mut().unwrap();
            let bytes = source.bytes_per_pixel;
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let (ix, iy) = match quarters {
                        1 => (source.width as usize - 1 - y, x),
                        2 => (
                            source.width as usize - 1 - x,
                            source.height as usize - 1 - y,
                        ),
                        3 => (y, source.height as usize - 1 - x),
                        _ => (x, y),
                    };
                    let from = iy * source.stride + ix * bytes;
                    let to = y * dest.stride + x * bytes;
                    dest.data_mut()[to..to + bytes]
                        .copy_from_slice(&source.data()[from..from + bytes]);
                }
            }
        }
        if quarters == 0 {
            out.component_ids = self.component_ids.clone();
            for (dest, source) in out.planes.iter_mut().zip(&self.planes) {
                dest.component_ids = source.component_ids.clone();
            }
        }
        Ok(out)
    }
    pub fn mirror(&mut self, horizontal: bool) -> Result<(), Error> {
        if (self.chroma == 2 && horizontal && self.width % 2 == 1)
            || (self.chroma == 1 && (self.width % 2 == 1 || self.height % 2 == 1))
        {
            *self = self.as_ycbcr444()?;
        }
        for p in &mut self.planes {
            let (w, h, bytes, stride) = (
                p.width as usize,
                p.height as usize,
                p.bytes_per_pixel,
                p.stride,
            );
            for y in 0..if horizontal { h } else { h / 2 } {
                for x in 0..if horizontal { w / 2 } else { w } {
                    let (other_x, other_y) = if horizontal {
                        (w - 1 - x, y)
                    } else {
                        (x, h - 1 - y)
                    };
                    for b in 0..bytes {
                        p.data_mut().swap(
                            y * stride + x * bytes + b,
                            other_y * stride + other_x * bytes + b,
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_channel_preserves_first_plane() {
        let mut img = Image::new(7, 5, 1, 11).unwrap();
        img.add_plane(10, 7, 5, 8).unwrap();
        img.plane_mut(10).unwrap().data_mut()[0] = 67;
        img.add_plane(10, 9, 3, 8).unwrap();
        assert_eq!(img.plane(10).unwrap().width, 7);
        assert_eq!(img.plane(10).unwrap().data()[0], 67);
    }
    #[test]
    fn planes_are_aligned_and_padding_is_zero() {
        for width in [1, 63, 64, 65, 127, 129] {
            let mut img = Image::new(width, 3, 1, 14).unwrap();
            img.add_plane(10, width, 3, 10).unwrap();
            let p = img.plane(10).unwrap();
            assert_eq!(p.data().as_ptr() as usize % 16, 0);
            assert_eq!(p.stride % 16, 0);
            assert!(p.data().iter().all(|b| *b == 0));
        }
    }
}
