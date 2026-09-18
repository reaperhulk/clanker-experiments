// SPDX-License-Identifier: LGPL-3.0-or-later
//! Pure Rust HEIF implementation. See PLAN.md for current compatibility scope.
#![forbid(unsafe_code)]

pub mod brands;
pub mod color;
pub mod container;
pub mod context;
pub mod error;
#[cfg(feature = "hevc")]
pub mod hevc;
pub mod image;

/// The API contract version, not the completeness of this implementation.
pub const COMPATIBILITY_VERSION: (u8, u8, u8) = (1, 23, 4);
