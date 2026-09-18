// SPDX-License-Identifier: LGPL-3.0-or-later
// Layout/validation semantics adapted from libheif, Copyright Dirk Farin and contributors.
use crate::error::Error;

#[derive(Debug)]
pub struct Plane {
    pub channel: i32,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub bytes_per_pixel: usize,
    pub stride: usize,
    storage: Vec<u8>,
    offset: usize,
}

impl Plane {
    fn new(
        channel: i32,
        width: u32,
        height: u32,
        depth: i32,
        components: usize,
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
        let mut storage = Vec::new();
        storage
            .try_reserve_exact(allocation)
            .map_err(|_| Error::ALLOCATION)?;
        storage.resize(allocation, 0);
        let offset = (16 - (storage.as_ptr() as usize & 15)) & 15;
        Ok(Self {
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

#[derive(Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub colorspace: i32,
    pub chroma: i32,
    pub premultiplied_alpha: bool,
    pub pixel_aspect_ratio: (u32, u32),
    planes: Vec<Plane>,
}

impl Image {
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
            width,
            height,
            colorspace,
            chroma,
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
        self.planes.try_reserve(1).map_err(|_| Error::ALLOCATION)?;
        self.planes
            .push(Plane::new(channel, width, height, bit_depth, components)?);
        Ok(())
    }
    pub fn plane(&self, channel: i32) -> Option<&Plane> {
        self.planes.iter().find(|p| p.channel == channel)
    }
    pub fn plane_mut(&mut self, channel: i32) -> Option<&mut Plane> {
        self.planes.iter_mut().find(|p| p.channel == channel)
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
