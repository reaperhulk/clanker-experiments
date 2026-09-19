// SPDX-License-Identifier: LGPL-3.0-or-later
//! Text objects retain their content independently of the current file tables.
use crate::{
    context::{Context, ContextError},
    items::Item,
};
use std::{ffi::CString, sync::Arc};

pub struct TextItem {
    pub id: u32,
    pub content: Vec<u8>,
}
impl Context {
    pub fn add_text(
        &mut self,
        content_type: CString,
        content: Vec<u8>,
    ) -> Result<Arc<TextItem>, ContextError> {
        self.items.layout.lock().unwrap().init_image();
        self.properties.has_ipco = true;
        self.properties.has_ipma = true;
        let mut item = Item::new(*b"mime");
        item.content_type = content_type;
        let id = self.items.add_pending(item)?;
        let text = Arc::new(TextItem { id, content });
        self.text_items.push(text.clone());
        Ok(text)
    }
}
