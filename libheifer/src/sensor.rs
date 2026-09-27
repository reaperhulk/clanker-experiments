// SPDX-License-Identifier: LGPL-3.0-or-later
//! Owned ISO 23001-17 imaging metadata. Float samples retain their exact bits.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BayerPixel {
    pub component_id: u32,
    pub component_gain: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BadPixel {
    pub row: u32,
    pub column: u32,
}
#[derive(Clone, Debug)]
pub struct BayerPattern {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<BayerPixel>,
}
#[derive(Clone, Debug)]
pub struct PolarizationPattern {
    pub components: Vec<u32>,
    pub width: u16,
    pub height: u16,
    pub angles: Vec<f32>,
}
#[derive(Clone, Debug)]
pub struct BadPixelsMap {
    pub components: Vec<u32>,
    pub applied: bool,
    pub rows: Vec<u32>,
    pub columns: Vec<u32>,
    pub pixels: Vec<BadPixel>,
}
#[derive(Clone, Debug)]
pub struct NonUniformityCorrection {
    pub components: Vec<u32>,
    pub applied: bool,
    pub width: u32,
    pub height: u32,
    pub gains: Vec<f32>,
    pub offsets: Vec<f32>,
}
#[derive(Clone, Debug, Default)]
pub struct SensorMetadata {
    pub bayer: Option<BayerPattern>,
    pub polarization: Vec<PolarizationPattern>,
    pub bad_pixels: Vec<BadPixelsMap>,
    pub nuc: Vec<NonUniformityCorrection>,
    pub chroma_location: Option<u8>,
}
impl SensorMetadata {
    pub fn polarization_for(&self, component: u32) -> Option<usize> {
        self.polarization
            .iter()
            .position(|p| p.components.is_empty() || p.components.contains(&component))
    }
}
