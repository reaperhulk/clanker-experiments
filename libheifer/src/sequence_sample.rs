// SPDX-License-Identifier: LGPL-3.0-or-later
//! Owned sequence sample payloads and metadata, independent of track storage.
use crate::{error::Error, tai::Timestamp};
use std::ffi::CString;

#[derive(Clone, Debug, Default)]
pub struct SampleMetadata {
    pub duration: u32,
    pub content_id: CString,
}

#[derive(Debug, Default)]
pub struct RawSample {
    pub metadata: SampleMetadata,
    pub timestamp: Option<Box<Timestamp>>,
    data: Vec<u8>,
}
impl RawSample {
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    pub fn has_storage(&self) -> bool {
        self.data.capacity() != 0
    }
    pub fn set_data(&mut self, data: &[u8]) -> Result<(), Error> {
        self.data.clear();
        self.data
            .try_reserve(data.len())
            .map_err(|_| Error::ALLOCATION)?;
        self.data.extend_from_slice(data);
        Ok(())
    }
}
