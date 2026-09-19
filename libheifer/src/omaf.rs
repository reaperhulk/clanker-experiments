// SPDX-License-Identifier: LGPL-3.0-or-later
//! Projection descriptions and retained ISO 23090-2 prfr properties.
use crate::{
    context::{Context, ContextError, ImageInfo},
    properties::Property,
};
use std::sync::{Arc, atomic::Ordering};
pub const FLAT: i32 = 255;
impl ImageInfo {
    pub fn projection(&self) -> i32 {
        self.projection.load(Ordering::Relaxed)
    }
    pub fn set_projection(&self, context: &mut Context, value: i32) -> Result<(), ContextError> {
        self.projection.store(value, Ordering::Relaxed);
        // The description keeps every C enum integer; only 5-bit values create
        // a property. Clearing the description does not remove old properties.
        if !(0..32).contains(&value) {
            return Ok(());
        }
        let property = Property {
            kind: *b"prfr",
            uuid: None,
            data: vec![0, 0, 0, 0, value as u8],
            raw: false,
            tai: None,
            write_error: None,
            gimi_components: None,
        };
        self.retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::new(property.clone()));
        context.properties.add_to_file(self.id, property, true)?;
        Ok(())
    }
    pub fn decoded_projection(&self) -> Option<i32> {
        self.retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|p| !p.raw && p.kind == *b"prfr")
            .and_then(|p| p.data.get(4))
            .map(|v| i32::from(v & 31))
    }
}
