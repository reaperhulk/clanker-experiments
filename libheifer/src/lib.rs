// SPDX-License-Identifier: LGPL-3.0-or-later
//! Pure Rust HEIF implementation. See PLAN.md for current compatibility scope.
#![forbid(unsafe_code)]

pub mod auxiliary;
mod box_probe;
pub mod brands;
pub mod camera;
pub mod color;
pub mod components;
pub mod compression;
pub mod container;
pub mod context;
pub mod conversion;
pub mod decoding;
mod deflate_compat;
pub mod derived;
pub mod error;
pub mod error_text;
pub mod geometry;
pub mod handle_properties;
#[cfg(feature = "hevc")]
pub mod hevc;
pub mod hevc_config;
pub mod image;
pub mod items;
pub mod jpeg_config;
pub mod mask;
pub mod metadata;
pub mod overlay;
pub mod properties;
pub mod security;
pub mod sensor;
pub mod tai;
pub mod text;
pub mod uncompressed;

/// The API contract version, not the completeness of this implementation.
pub const COMPATIBILITY_VERSION: (u8, u8, u8) = (1, 23, 4);

pub mod sequence_sample;

pub mod omaf;

pub mod regions;

pub mod entity_groups;

pub mod gimi;
