// SPDX-License-Identifier: LGPL-3.0-or-later
// Metadata insertion semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Metadata item creation, including the reference's Exif offset convention.
use crate::{
    context::{Context, ContextError},
    items::{Item, Reference},
};
use std::ffi::CString;

pub fn exif_payload(data: &[u8]) -> Result<Vec<u8>, ContextError> {
    if data.is_empty() {
        return Err(ContextError::new(
            5,
            2006,
            "Usage error: Invalid parameter value: Could not find location of TIFF header in Exif metadata.",
        ));
    }
    let mut offset = 0;
    // Upstream accepts a nonempty buffer even if the scan reaches its end
    // without a TIFF signature. Keep the strict boundary and resulting offset.
    while offset + 4 < data.len() {
        if matches!(&data[offset..offset + 4], b"MM\0*" | b"II*\0") {
            break;
        }
        offset += 1;
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(data.len().checked_add(4).ok_or_else(allocation)?)
        .map_err(|_| allocation())?;
    result.extend_from_slice(&(offset as u32).to_be_bytes());
    result.extend_from_slice(data);
    Ok(result)
}
fn allocation() -> ContextError {
    ContextError::new(6, 0, "Memory allocation error")
}

impl Context {
    pub fn add_metadata(
        &mut self,
        target: u32,
        kind: [u8; 4],
        content_type: Option<CString>,
        data: &[u8],
        compression: i32,
    ) -> Result<u32, ContextError> {
        // Metadata writers initialize the image property tables, even when
        // the supplied image handle belongs to another context.
        self.properties.has_ipco = true;
        self.properties.has_ipma = true;
        let mut item = Item::new(kind);
        item.content_type = content_type.unwrap_or_default();
        // This API's "deflate" branch writes a zlib wrapper in the reference.
        // Generic MIME item insertion uses raw DEFLATE instead. Do not unify
        // these two observably different entry points.
        let method = if matches!(compression, 3 | 4) { 4 } else { 0 };
        let id = self.items.add_compressed(item, data, method)?;
        if compression == 3 {
            self.items.items.get_mut(&id).unwrap().content_encoding = c"deflate".into();
        }
        self.items.references.push(Reference {
            from: id,
            kind: u32::from_be_bytes(*b"cdsc"),
            to: vec![target],
        });
        Ok(id)
    }
}
