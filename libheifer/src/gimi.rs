// SPDX-License-Identifier: LGPL-3.0-or-later
//! GIMI strings and shared component properties retain their file-object identity.
use crate::{
    context::{Context, ContextError, ImageInfo},
    properties::Property,
};
use std::{
    ffi::CString,
    sync::{Arc, Mutex},
};
pub const CONTENT_UUID: [u8; 16] = [
    0x26, 0x1e, 0xf3, 0x74, 0x1d, 0x97, 0x5b, 0xba, 0xac, 0xbd, 0x9d, 0x2c, 0x8e, 0xa7, 0x35, 0x22,
];
pub const COMPONENT_UUID: [u8; 16] = [
    0x9d, 0xb9, 0xdd, 0x6e, 0x37, 0x3c, 0x5a, 0x4e, 0x81, 0x10, 0x21, 0xfc, 0x83, 0xa9, 0x11, 0xfd,
];
pub fn c_string(value: &[u8]) -> CString {
    CString::new(value.split(|b| *b == 0).next().unwrap_or_default()).unwrap()
}
pub fn parse_components(data: &[u8], maximum: u32) -> Result<Vec<CString>, ContextError> {
    let Some(count) = data
        .get(..4)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
    else {
        return Err(ContextError::invalid(100, "Unexpected end of file"));
    };
    if maximum != 0 && count > maximum {
        return Err(ContextError::invalid(
            1000,
            &format!(
                "Security limit exceeded: GIMI component content IDs box contains {count} components, but security limit is set to {maximum} components"
            ),
        ));
    }
    let mut data = &data[4..];
    let mut ids = Vec::new();
    for _ in 0..count {
        if data.is_empty() {
            return Err(ContextError::invalid(
                100,
                "Unexpected end of file: Not enough data for all component content IDs",
            ));
        }
        let end = data.iter().position(|b| *b == 0).unwrap_or(data.len() - 1);
        ids.push(CString::new(&data[..end]).unwrap());
        data = &data[end + 1..];
    }
    Ok(ids)
}
impl ImageInfo {
    pub fn decoded_content_id(&self) -> Option<Vec<u8>> {
        self.retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|p| !p.raw && p.kind == *b"uuid" && p.uuid == Some(CONTENT_UUID))
            .map(|p| p.data.clone())
    }
    pub fn component_content_ids(&self) -> Option<Arc<Mutex<Vec<CString>>>> {
        self.retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find_map(|p| p.gimi_components.clone())
    }
    fn add_gimi_property(&self, ctx: &mut Context, property: Property) -> Result<(), ContextError> {
        ctx.properties.add(self.id, property.clone(), false)?;
        self.retained_properties
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::new(property));
        Ok(())
    }
    pub fn set_content_id(&self, ctx: &mut Context, value: Vec<u8>) {
        let _ = self.add_gimi_property(
            ctx,
            Property {
                kind: *b"uuid",
                uuid: Some(CONTENT_UUID),
                data: value.clone(),
                raw: false,
                tai: None,
                gimi_components: None,
            },
        );
        *self
            .gimi_content_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
    }
    pub fn set_component_content_id(&self, ctx: &mut Context, index: u32, value: CString) {
        if let Some(ids) = self.component_content_ids() {
            let mut ids = ids
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let size = ids.len().max(index as usize + 1);
            ids.resize(size, CString::default());
            ids[index as usize] = value;
        } else {
            let mut ids = vec![CString::default(); index as usize + 1];
            ids[index as usize] = value;
            let property = Property {
                kind: *b"uuid",
                uuid: Some(COMPONENT_UUID),
                data: Vec::new(),
                raw: false,
                tai: None,
                gimi_components: Some(Arc::new(Mutex::new(ids))),
            };
            let _ = self.add_gimi_property(ctx, property);
        }
    }
}
