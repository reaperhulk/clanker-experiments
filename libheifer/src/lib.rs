// SPDX-License-Identifier: LGPL-3.0-or-later
//! Pure Rust HEIF implementation. See PLAN.md for current compatibility scope.
#![forbid(unsafe_code)]

pub mod auxiliary;
#[cfg(feature = "av1")]
pub mod av1;
pub mod av1_config;
pub mod avc_config;
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
#[cfg(feature = "hevc")]
pub mod hevc_encoder;
pub mod image;
pub mod input;
pub mod items;
pub mod jpeg_config;
pub mod mask;
pub mod metadata;
mod mini;
mod mini_write;
pub mod overlay;
pub mod properties;
pub mod security;
pub mod sensor;
pub mod tai;
pub mod text;
pub mod uncompressed;
pub mod vvc_config;

/// The API contract version, not the completeness of this implementation.
pub const COMPATIBILITY_VERSION: (u8, u8, u8) = (1, 23, 4);

pub mod sequence_sample;

pub mod omaf;

pub mod regions;

pub mod entity_groups;

pub mod gimi;

pub mod writing;

pub mod encoding;
mod uncompressed_encode;

pub mod tile_encoding;
pub mod tiling;

pub mod sequences;

pub mod debug;

#[cfg(feature = "jpeg")]
pub mod jpeg;
#[cfg(feature = "jpeg")]
pub mod jpeg_encoder;

#[cfg(feature = "jpeg")]
mod jpeg_header;

#[cfg(feature = "jpeg2000")]
pub mod htj2k_encoder;
#[cfg(feature = "jpeg2000")]
mod htj2k_encoder_tables;
#[cfg(feature = "jpeg2000")]
pub mod jpeg2000;
pub mod jpeg2000_config;
#[cfg(feature = "jpeg2000")]
pub mod jpeg2000_encoder;
mod jpeg2000_properties;

#[cfg(feature = "avc")]
pub mod avc;
#[cfg(feature = "avc")]
pub mod avc_encoder;
#[cfg(feature = "avc")]
pub mod avc_high;

#[cfg(feature = "avc")]
mod avc_openh264;
#[cfg(feature = "vvc")]
pub mod vvc;
