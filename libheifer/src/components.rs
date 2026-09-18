// SPDX-License-Identifier: LGPL-3.0-or-later
//! Component descriptions are independent of pixel storage and retain insertion order.
use crate::error::Error;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Complex32 {
    pub real: f32,
    pub imaginary: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Complex64 {
    pub real: f64,
    pub imaginary: f64,
}

#[derive(Clone, Debug)]
pub struct Description {
    pub id: u32,
    pub channel: i32,
    pub kind: u16,
    pub datatype: i32,
    pub bit_depth: u16,
    pub width: u32,
    pub height: u32,
    pub has_data: bool,
    pub content_id: Vec<u8>,
}
impl Description {
    pub fn reference(kind: u16) -> Self {
        Self {
            id: 0,
            channel: channel_for_type(kind),
            kind,
            datatype: 255,
            bit_depth: 0,
            width: 0,
            height: 0,
            has_data: false,
            content_id: Vec::new(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ComponentIds {
    pub(crate) next: u32,
    pub descriptions: Vec<Description>,
}
impl Default for ComponentIds {
    fn default() -> Self {
        Self {
            next: 1,
            descriptions: Vec::new(),
        }
    }
}
impl ComponentIds {
    pub fn mint(&mut self) -> u32 {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        id
    }
    pub fn find(&self, id: u32) -> Option<&Description> {
        self.descriptions.iter().find(|d| d.id == id)
    }
    pub fn find_mut(&mut self, id: u32) -> Option<&mut Description> {
        self.descriptions.iter_mut().find(|d| d.id == id)
    }
    pub fn add(&mut self, mut description: Description) -> Result<u32, Error> {
        self.descriptions
            .try_reserve(1)
            .map_err(|_| Error::ALLOCATION)?;
        description.id = self.mint();
        let id = description.id;
        self.descriptions.push(description);
        Ok(id)
    }
    pub fn add_reference(&mut self, kind: u16) -> Result<u32, Error> {
        self.add(Description::reference(kind))
    }
}
pub fn channel_for_type(kind: u16) -> i32 {
    match kind {
        0 | 1 => 0,
        2..=7 => i32::from(kind - 1),
        8 => 12,
        9 => 13,
        11 => 11,
        _ => 65535,
    }
}
pub fn types_for_channel(channel: i32, chroma: i32) -> Vec<u16> {
    match channel {
        0..=6 => vec![(channel + 1) as u16],
        11 => vec![11],
        12 => vec![8],
        13 => vec![9],
        10 if matches!(chroma, 10 | 12 | 14) => vec![4, 5, 6],
        10 if matches!(chroma, 11 | 13 | 15) => vec![4, 5, 6, 7],
        _ => vec![channel.wrapping_add(1000) as u16],
    }
}

impl ComponentIds {
    pub fn visual(
        width: u32,
        height: u32,
        colorspace: i32,
        chroma: i32,
        luma: i32,
        chroma_depth: i32,
    ) -> Result<Self, Error> {
        let mut out = Self::default();
        let luma = if luma > 0 { luma } else { 8 } as u16;
        let chroma_depth = if chroma_depth > 0 {
            chroma_depth as u16
        } else {
            luma
        };
        let kinds: &[u16] = match colorspace {
            2 => &[0],
            0 => &[1, 2, 3],
            1 => &[4, 5, 6],
            _ => &[],
        };
        for &kind in kinds {
            let mut d = Description::reference(kind);
            d.has_data = true;
            d.datatype = 0;
            d.bit_depth = if matches!(kind, 2 | 3) {
                chroma_depth
            } else {
                luma
            };
            d.width = if matches!(kind, 2 | 3) && matches!(chroma, 1 | 2) {
                width.div_ceil(2)
            } else {
                width
            };
            d.height = if matches!(kind, 2 | 3) && chroma == 1 {
                height.div_ceil(2)
            } else {
                height
            };
            out.add(d)?;
        }
        Ok(out)
    }
    pub fn alpha(&mut self, width: u32, height: u32, depth: i32) -> Result<(), Error> {
        if self.descriptions.iter().any(|d| d.channel == 6) {
            return Ok(());
        }
        let mut d = Description::reference(7);
        d.datatype = 0;
        d.bit_depth = if depth > 0 { depth as u16 } else { 8 };
        d.width = width;
        d.height = height;
        d.has_data = true;
        self.add(d)?;
        Ok(())
    }
}
