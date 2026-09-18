// SPDX-License-Identifier: LGPL-3.0-or-later
//! Pure Rust HEIF implementation. See PLAN.md for current compatibility scope.
#![forbid(unsafe_code)]

pub mod auxiliary;
pub mod brands;
pub mod color;
pub mod container;
pub mod context;
pub mod conversion;
pub mod decoding;
pub mod derived;
pub mod error;
pub mod error_text;
pub mod geometry;
#[cfg(feature = "hevc")]
pub mod hevc;
pub mod hevc_config;
pub mod image;
pub mod mask;
pub mod overlay;
pub mod security;

/// The API contract version, not the completeness of this implementation.
pub const COMPATIBILITY_VERSION: (u8, u8, u8) = (1, 23, 4);
