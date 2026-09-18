// SPDX-License-Identifier: LGPL-3.0-or-later
//! Pure Rust HEIF implementation. See PLAN.md for current compatibility scope.
#![forbid(unsafe_code)]

pub mod brands;
pub mod color;
pub mod container;
pub mod context;
pub mod conversion;
#[cfg(feature = "hevc")]
pub mod decoding;
pub mod derived;
pub mod error;
pub mod error_text;
pub mod geometry;
#[cfg(feature = "hevc")]
pub mod hevc;
pub mod image;

/// The API contract version, not the completeness of this implementation.
pub const COMPATIBILITY_VERSION: (u8, u8, u8) = (1, 23, 4);
