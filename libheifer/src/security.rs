// SPDX-License-Identifier: LGPL-3.0-or-later
//! Context resource limits. Zero disables a limit.
use crate::context::ContextError;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_image_size_pixels: u64,
    pub max_number_of_tiles: u64,
    pub max_bayer_pattern_pixels: u32,
    pub max_items: u32,
    pub max_color_profile_size: u32,
    pub max_memory_block_size: u64,
    pub max_components: u32,
    pub max_iloc_extents_per_item: u32,
    pub max_size_entity_group: u32,
    pub max_children_per_box: u32,
    pub max_total_memory: u64,
    pub max_sample_description_box_entries: u32,
    pub max_sample_group_description_box_entries: u32,
    pub max_sequence_frames: u32,
    pub max_number_of_file_brands: u32,
    pub max_bad_pixels: u32,
    pub max_iso23001_17_pixel_size_bytes: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_image_size_pixels: 1_073_741_824,
            max_number_of_tiles: 16_777_216,
            max_bayer_pattern_pixels: 256,
            max_items: 1000,
            max_color_profile_size: 104_857_600,
            max_memory_block_size: 4_294_967_296,
            max_components: 256,
            max_iloc_extents_per_item: 32,
            max_size_entity_group: 64,
            max_children_per_box: 100,
            max_total_memory: 4_294_967_296,
            max_sample_description_box_entries: 1024,
            max_sample_group_description_box_entries: 1024,
            max_sequence_frames: 18_000_000,
            max_number_of_file_brands: 1000,
            max_bad_pixels: 1000,
            max_iso23001_17_pixel_size_bytes: 256,
        }
    }
}
impl Limits {
    pub fn check_image_size(&self, width: u32, height: u32) -> Result<(), ContextError> {
        let maximum = self.max_image_size_pixels;
        if maximum != 0
            && (width > i32::MAX as u32
                || height > i32::MAX as u32
                || u64::from(width) * u64::from(height) > maximum)
        {
            return Err(ContextError::new(
                6,
                1000,
                format!(
                    "Memory allocation error: Security limit exceeded: Image size {width}x{height} exceeds the maximum image size {maximum}\n"
                ),
            ));
        }
        if width == 0 || height == 0 {
            return Err(ContextError::new(
                6,
                129,
                "Memory allocation error: Invalid image size: zero width or height",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Budget {
    pub limits: std::sync::Arc<std::sync::RwLock<Limits>>,
    used: std::sync::Arc<std::sync::Mutex<u64>>,
}
impl Budget {
    pub fn new(limits: std::sync::Arc<std::sync::RwLock<Limits>>) -> Self {
        Self {
            limits,
            used: std::sync::Arc::new(std::sync::Mutex::new(0)),
        }
    }
    pub fn with_limits(&self, limits: Limits) -> Self {
        Self {
            limits: std::sync::Arc::new(std::sync::RwLock::new(limits)),
            used: self.used.clone(),
        }
    }
    pub fn reserve(
        self: &std::sync::Arc<Self>,
        amount: u64,
        reason: &str,
    ) -> Result<Reservation, crate::error::Error> {
        let limits = self
            .limits
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if limits.max_memory_block_size != 0 && amount > limits.max_memory_block_size {
            return Err(crate::error::Error::owned(
                6,
                1000,
                format!(
                    "Memory allocation error: Security limit exceeded: Allocating {amount} bytes for {reason} exceeds the security limit of {} bytes",
                    limits.max_memory_block_size
                ),
            ));
        }
        let mut used = self
            .used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total = used
            .checked_add(amount)
            .ok_or(crate::error::Error::ALLOCATION)?;
        if limits.max_total_memory != 0 && total > limits.max_total_memory {
            return Err(crate::error::Error::owned(
                6,
                1000,
                format!(
                    "Memory allocation error: Security limit exceeded: Memory usage of {total} bytes for {reason} exceeds the security limit of {} bytes of total memory usage",
                    limits.max_total_memory
                ),
            ));
        }
        *used = total;
        Ok(Reservation {
            budget: self.clone(),
            amount,
        })
    }
}
#[derive(Debug)]
pub struct Reservation {
    budget: std::sync::Arc<Budget>,
    amount: u64,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        *self
            .budget
            .used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) -= self.amount;
    }
}
