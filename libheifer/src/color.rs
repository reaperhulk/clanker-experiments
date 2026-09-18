// SPDX-License-Identifier: LGPL-3.0-or-later
// Compatibility semantics adapted from libheif, Copyright Dirk Farin and contributors.
use crate::error::Error;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ColorConversionOptions {
    pub version: u8,
    pub preferred_chroma_downsampling_algorithm: i32,
    pub preferred_chroma_upsampling_algorithm: i32,
    pub only_use_preferred_chroma_algorithm: u8,
}
impl Default for ColorConversionOptions {
    fn default() -> Self {
        // Matches the reference without the optional native libsharpyuv backend.
        Self {
            version: 1,
            preferred_chroma_downsampling_algorithm: 2,
            preferred_chroma_upsampling_algorithm: 2,
            only_use_preferred_chroma_algorithm: 1,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ColorConversionOptionsExt {
    pub version: u8,
    pub alpha_composition_mode: i32,
    pub background_red: u16,
    pub background_green: u16,
    pub background_blue: u16,
    pub secondary_background_red: u16,
    pub secondary_background_green: u16,
    pub secondary_background_blue: u16,
    pub checkerboard_square_size: u16,
}
impl Default for ColorConversionOptionsExt {
    fn default() -> Self {
        Self {
            version: 1,
            alpha_composition_mode: 0,
            background_red: 0xffff,
            background_green: 0xffff,
            background_blue: 0xffff,
            secondary_background_red: 0xcccc,
            secondary_background_green: 0xcccc,
            secondary_background_blue: 0xcccc,
            checkerboard_square_size: 16,
        }
    }
}
impl ColorConversionOptionsExt {
    pub fn copy_from_versioned(&mut self, src: Self) {
        if self.version.min(src.version) == 1 {
            let version = self.version;
            *self = src;
            self.version = version;
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NclxProfile {
    pub version: u8,
    pub color_primaries: i32,
    pub transfer_characteristics: i32,
    pub matrix_coefficients: i32,
    pub full_range_flag: u8,
    pub color_primary_red_x: f32,
    pub color_primary_red_y: f32,
    pub color_primary_green_x: f32,
    pub color_primary_green_y: f32,
    pub color_primary_blue_x: f32,
    pub color_primary_blue_y: f32,
    pub color_primary_white_x: f32,
    pub color_primary_white_y: f32,
}

impl Default for NclxProfile {
    fn default() -> Self {
        Self {
            version: 1,
            color_primaries: 1,
            transfer_characteristics: 13,
            matrix_coefficients: 6,
            full_range_flag: 1,
            // Upstream's allocator leaves decoded coordinates unspecified.
            color_primary_red_x: 0.0,
            color_primary_red_y: 0.0,
            color_primary_green_x: 0.0,
            color_primary_green_y: 0.0,
            color_primary_blue_x: 0.0,
            color_primary_blue_y: 0.0,
            color_primary_white_x: 0.0,
            color_primary_white_y: 0.0,
        }
    }
}

pub fn validate_primaries(value: u16) -> Result<(), Error> {
    if matches!(value, 1 | 2 | 4..=12 | 22) {
        Ok(())
    } else {
        Err(Error::new(2, 133, c"Unknown error"))
    }
}
pub fn validate_transfer(value: u16) -> Result<(), Error> {
    if matches!(value, 1 | 2 | 4..=18) {
        Ok(())
    } else {
        Err(Error::new(2, 134, c"Unknown error"))
    }
}
pub fn validate_matrix(value: u16) -> Result<(), Error> {
    if matches!(value, 0..=2 | 4..=14) {
        Ok(())
    } else {
        Err(Error::new(2, 135, c"Unknown error"))
    }
}

/// Stored image metadata is narrowed to 16 bits, matching the container model.
#[derive(Clone, Copy, Debug)]
pub struct Nclx {
    pub primaries: u16,
    pub transfer: u16,
    pub matrix: u16,
    pub full_range: bool,
}

impl Nclx {
    pub fn is_defined(self) -> bool {
        self.primaries != 2 || self.transfer != 2 || self.matrix != 2 || !self.full_range
    }
    pub fn decode(self) -> Result<NclxProfile, Error> {
        validate_primaries(self.primaries)
            .map_err(|_| Error::new(2, 133, c"Invalid input: Unknown NCLX color primaries"))?;
        validate_transfer(self.transfer).map_err(|_| {
            Error::new(
                2,
                134,
                c"Invalid input: Unknown NCLX transfer characteristics",
            )
        })?;
        validate_matrix(self.matrix)
            .map_err(|_| Error::new(2, 135, c"Invalid input: Unknown NCLX matrix coefficients"))?;
        let [gx, gy, bx, by, rx, ry, wx, wy] = match self.primaries {
            1 => [0.300, 0.600, 0.150, 0.060, 0.640, 0.330, 0.3127, 0.3290],
            4 => [0.21, 0.71, 0.14, 0.08, 0.67, 0.33, 0.310, 0.316],
            5 => [0.29, 0.60, 0.15, 0.06, 0.64, 0.33, 0.3127, 0.3290],
            6 | 7 => [0.310, 0.595, 0.155, 0.070, 0.630, 0.340, 0.3127, 0.3290],
            8 => [0.243, 0.692, 0.145, 0.049, 0.681, 0.319, 0.310, 0.316],
            9 => [0.170, 0.797, 0.131, 0.046, 0.708, 0.292, 0.3127, 0.3290],
            10 => [0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.333333, 0.33333],
            11 => [0.265, 0.690, 0.150, 0.060, 0.680, 0.320, 0.314, 0.351],
            12 => [0.265, 0.690, 0.150, 0.060, 0.680, 0.320, 0.3127, 0.3290],
            22 => [0.295, 0.605, 0.155, 0.077, 0.630, 0.340, 0.3127, 0.3290],
            _ => [0.0; 8],
        };
        Ok(NclxProfile {
            version: 1,
            color_primaries: self.primaries.into(),
            transfer_characteristics: self.transfer.into(),
            matrix_coefficients: self.matrix.into(),
            full_range_flag: self.full_range.into(),
            color_primary_red_x: rx,
            color_primary_red_y: ry,
            color_primary_green_x: gx,
            color_primary_green_y: gy,
            color_primary_blue_x: bx,
            color_primary_blue_y: by,
            color_primary_white_x: wx,
            color_primary_white_y: wy,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ContentLightLevel {
    pub max_content_light_level: u16,
    pub max_pic_average_light_level: u16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MasteringDisplayColourVolume {
    pub display_primaries_x: [u16; 3],
    pub display_primaries_y: [u16; 3],
    pub white_point_x: u16,
    pub white_point_y: u16,
    pub max_display_mastering_luminance: u32,
    pub min_display_mastering_luminance: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodedMasteringDisplayColourVolume {
    pub display_primaries_x: [f32; 3],
    pub display_primaries_y: [f32; 3],
    pub white_point_x: f32,
    pub white_point_y: f32,
    pub max_display_mastering_luminance: f64,
    pub min_display_mastering_luminance: f64,
}
impl MasteringDisplayColourVolume {
    pub fn decode(self) -> DecodedMasteringDisplayColourVolume {
        fn coordinate(value: u16, max: u16) -> f32 {
            if (5..=max).contains(&value) {
                (f64::from(value) * 0.00002) as f32
            } else {
                0.0
            }
        }
        fn luminance(value: u32, min: u32, max: u32) -> f64 {
            if (min..=max).contains(&value) {
                f64::from(value) * 0.0001
            } else {
                0.0
            }
        }
        DecodedMasteringDisplayColourVolume {
            display_primaries_x: self.display_primaries_x.map(|v| coordinate(v, 37000)),
            display_primaries_y: self.display_primaries_y.map(|v| coordinate(v, 42000)),
            white_point_x: coordinate(self.white_point_x, 37000),
            white_point_y: coordinate(self.white_point_y, 42000),
            max_display_mastering_luminance: luminance(
                self.max_display_mastering_luminance,
                50000,
                100000000,
            ),
            min_display_mastering_luminance: luminance(
                self.min_display_mastering_luminance,
                1,
                50000,
            ),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AmbientViewingEnvironment {
    pub ambient_illumination: u32,
    pub ambient_light_x: u16,
    pub ambient_light_y: u16,
}

#[derive(Debug)]
pub struct RawProfile {
    pub profile_type: u32,
    pub data: Vec<u8>,
}
#[derive(Debug, Default)]
pub struct ColorMetadata {
    pub raw: Option<RawProfile>,
    pub nclx: Option<Nclx>,
    pub content_light: ContentLightLevel,
    pub mastering: Option<MasteringDisplayColourVolume>,
    pub ambient: Option<AmbientViewingEnvironment>,
    pub diffuse_white: Option<u32>,
}
impl ColorMetadata {
    pub fn profile_type(&self) -> u32 {
        self.raw.as_ref().map_or_else(
            || {
                if self.nclx.is_some_and(Nclx::is_defined) {
                    u32::from_be_bytes(*b"nclx")
                } else {
                    0
                }
            },
            |r| r.profile_type,
        )
    }
    pub fn set_raw(&mut self, profile_type: u32, data: &[u8]) -> Result<(), Error> {
        let mut copy = Vec::new();
        copy.try_reserve_exact(data.len())
            .map_err(|_| Error::ALLOCATION)?;
        copy.extend_from_slice(data);
        self.raw = Some(RawProfile {
            profile_type,
            data: copy,
        });
        Ok(())
    }
    pub fn has_content_light(&self) -> bool {
        self.content_light.max_content_light_level != 0
            || self.content_light.max_pic_average_light_level != 0
    }
}
