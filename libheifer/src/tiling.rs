// SPDX-License-Identifier: LGPL-3.0-or-later
//! Coded tile geometry and ordered inverse display transformations.
use crate::{
    context::{ContextError, Document, ImageInfo},
    properties::Property,
};
type Result<T> = std::result::Result<T, ContextError>;
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Tiling {
    pub version: i32,
    pub num_columns: u32,
    pub num_rows: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub top_offset: u32,
    pub left_offset: u32,
    pub number_of_extra_dimensions: u8,
    pub extra_dimension_size: [u32; 8],
}
impl Tiling {
    pub fn for_image(document: &Document, image: &ImageInfo) -> Result<Self> {
        let mut t = Self::default();
        if image.kind == *b"grid" {
            if let Some(g) = image.grid {
                t.num_columns = g.columns;
                t.num_rows = g.rows;
                t.image_width = g.width;
                t.image_height = g.height;
                let ids = image
                    .grid_tiles
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(first) = ids
                    .first()
                    .and_then(|id| document.images.get(id))
                    .filter(|i| i.error.is_none())
                {
                    t.tile_width = first.width;
                    t.tile_height = first.height;
                }
            }
        } else if image.kind == *b"unci" {
            let props = image
                .retained_properties
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(p) = props.iter().find(|p| p.kind == *b"uncC") {
                let config = crate::uncompressed::Configuration::parse(&p.data, None)?;
                t.num_columns = config.columns;
                t.num_rows = config.rows;
                t.image_width = image.ispe.0;
                t.image_height = image.ispe.1;
                t.tile_width = t.image_width / t.num_columns;
                t.tile_height = t.image_height / t.num_rows;
            }
        } else {
            t.version = 1;
            t.num_columns = 1;
            t.num_rows = 1;
            let (w, h) = if image.ispe != (0, 0) {
                image.ispe
            } else {
                (image.width, image.height)
            };
            t.tile_width = w;
            t.tile_height = h;
            t.image_width = w;
            t.image_height = h;
        }
        Ok(t)
    }
    pub fn transform(&mut self, props: &[&Property]) -> Result<()> {
        let (mut left, mut top, mut right, mut bottom) = (0u32, 0u32, 0u32, 0u32);
        if self.tile_width != 0 && self.tile_height != 0 {
            right = self.image_width % self.tile_width;
            bottom = self.image_height % self.tile_height;
        }
        for p in props.iter().filter(|p| !p.raw) {
            match &p.kind {
                b"irot" if !p.data.is_empty() => {
                    let r = p.data[0] & 3;
                    if r & 1 != 0 {
                        std::mem::swap(&mut self.tile_width, &mut self.tile_height);
                        std::mem::swap(&mut self.image_width, &mut self.image_height);
                        std::mem::swap(&mut self.num_columns, &mut self.num_rows);
                    }
                    (left, top, right, bottom) = match r {
                        1 => (top, right, bottom, left),
                        2 => (right, bottom, left, top),
                        3 => (bottom, left, top, right),
                        _ => (left, top, right, bottom),
                    };
                }
                b"imir" if !p.data.is_empty() => {
                    if p.data[0] & 1 != 0 {
                        std::mem::swap(&mut left, &mut right);
                    } else {
                        std::mem::swap(&mut top, &mut bottom);
                    }
                }
                b"clap" => {
                    let (l, r, t, b) = crate::geometry::CleanAperture::parse(&p.data)?
                        .crop(self.image_width, self.image_height)?;
                    left = left.wrapping_add(l);
                    right = right.wrapping_add(r);
                    top = top.wrapping_add(t);
                    bottom = bottom.wrapping_add(b);
                }
                b"iscl" => {
                    return Err(ContextError::new(
                        4,
                        0,
                        "Unsupported feature: Unspecified: Image scaling (iscl) transformative property is not yet supported",
                    ));
                }
                _ => {}
            }
        }
        self.left_offset = left;
        self.top_offset = top;
        Ok(())
    }
    pub fn original_position(
        mut self,
        props: &[&Property],
        mut x: u32,
        mut y: u32,
    ) -> Result<(u32, u32)> {
        self.transform(props)?;
        if x >= self.num_columns || y >= self.num_rows {
            return Err(ContextError::new(
                5,
                0,
                "Usage error: Unspecified: Tile coordinate out of range for displayed image",
            ));
        }
        for p in props.iter().rev().filter(|p| !p.raw) {
            match &p.kind {
                b"irot" if !p.data.is_empty() => {
                    let r = p.data[0] & 3;
                    (x, y) = match r {
                        1 => (self.num_rows - 1 - y, x),
                        2 => (self.num_columns - 1 - x, self.num_rows - 1 - y),
                        3 => (y, self.num_columns - 1 - x),
                        _ => (x, y),
                    };
                    if r & 1 != 0 {
                        std::mem::swap(&mut self.num_columns, &mut self.num_rows);
                    }
                }
                b"imir" if !p.data.is_empty() => {
                    if p.data[0] & 1 != 0 {
                        x = self.num_columns - 1 - x;
                    } else {
                        y = self.num_rows - 1 - y;
                    }
                }
                _ => {}
            }
        }
        Ok((x, y))
    }
}
