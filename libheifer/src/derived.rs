// SPDX-License-Identifier: LGPL-3.0-or-later
// Derived-image semantics adapted from libheif, Copyright Dirk Farin and contributors.
use crate::{container::Container, context::ContextError};
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub rows: u32,
    pub columns: u32,
    pub width: u32,
    pub height: u32,
}
impl Grid {
    pub fn parse(data: &[u8]) -> Result<Self, ContextError> {
        if data.len() < 8 {
            return Err(ContextError::invalid(
                118,
                "Invalid grid data: Less than 8 bytes of data",
            ));
        }
        if data[0] != 0 {
            return Err(ContextError::new(
                4,
                3002,
                format!(
                    "Unsupported feature: Unsupported data version: Grid image version {} is not supported",
                    data[0]
                ),
            ));
        }
        let (width, height) = if data[1] & 1 != 0 {
            if data.len() < 12 {
                return Err(ContextError::invalid(
                    118,
                    "Invalid grid data: Grid image data incomplete",
                ));
            }
            (
                u32::from_be_bytes(data[4..8].try_into().unwrap()),
                u32::from_be_bytes(data[8..12].try_into().unwrap()),
            )
        } else {
            (
                u32::from(u16::from_be_bytes([data[4], data[5]])),
                u32::from(u16::from_be_bytes([data[6], data[7]])),
            )
        };
        Ok(Self {
            rows: u32::from(data[2]) + 1,
            columns: u32::from(data[3]) + 1,
            width,
            height,
        })
    }
    pub fn load(container: &Container<'_>, id: u32) -> Result<Self, ContextError> {
        let grid = Self::parse(&container.payload(id)?)?;
        if !container.has_references {
            return Err(ContextError::invalid(
                113,
                "No 'iref' box: No iref box available, but needed for grid image",
            ));
        }
        let count = container.items[&id]
            .references
            .get(b"dimg")
            .map_or(0, Vec::len);
        if count != (grid.rows * grid.columns) as usize {
            return Err(ContextError::invalid(
                119,
                &format!(
                    "Missing grid images: Tiled image with {}x{}={} tiles, but only {} tile images in file",
                    grid.rows,
                    grid.columns,
                    grid.rows * grid.columns,
                    count
                ),
            ));
        }
        Ok(grid)
    }
}
