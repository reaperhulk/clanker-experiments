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

/// Component IDs are local to an image. Reference-only entries consume IDs just
/// like newly allocated planes, including duplicate plane channels.
#[derive(Clone, Debug)]
pub struct ComponentIds {
    next: u32,
    pub references: Vec<(u32, u16)>,
}
impl Default for ComponentIds {
    fn default() -> Self {
        Self {
            next: 1,
            references: Vec::new(),
        }
    }
}
impl ComponentIds {
    pub fn mint(&mut self) -> u32 {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        id
    }
    pub fn add_reference(&mut self, kind: u16) -> Result<u32, crate::error::Error> {
        self.references
            .try_reserve(1)
            .map_err(|_| crate::error::Error::ALLOCATION)?;
        let id = self.mint();
        self.references.push((id, kind));
        Ok(id)
    }
    pub fn plane(&mut self, channel: i32, chroma: i32) {
        let count = if channel == 10 {
            match chroma {
                10 | 12 | 14 => 3,
                11 | 13 | 15 => 4,
                _ => 1,
            }
        } else {
            1
        };
        for _ in 0..count {
            self.mint();
        }
    }
}
