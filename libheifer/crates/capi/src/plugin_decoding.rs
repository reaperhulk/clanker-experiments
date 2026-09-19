// SPDX-License-Identifier: LGPL-3.0-or-later
// Callback sequencing adapted from libheif, Copyright Dirk Farin and contributors.
use crate::{HeifError, context::SharedContext, plugin_registry::field, plugin_types::*};
use libheifer::{
    context::{ContextError, Document},
    decoding::{DecodeOptions, DecoderProvider, ItemDecoder},
    image::Image,
};
use std::{
    ffi::{CStr, c_void},
    ptr,
    sync::{Arc, Weak},
};
pub(super) struct Provider(pub Weak<SharedContext>);
struct Decoder {
    plugin: *const DecoderPlugin,
    parent: Weak<SharedContext>,
    format: i32,
}
// SAFETY: registered callback records are immutable and caller-owned. Each item
// serializes its decode operations, and no foreign decoder state is shared.
unsafe impl Send for Decoder {}
unsafe impl Sync for Decoder {}
fn loading(message: &str) -> ContextError {
    ContextError::new(
        11,
        6003,
        format!(
            "Error while loading plugin: Support for this compression format has not been built in: {message}"
        ),
    )
}
impl DecoderProvider for Provider {
    fn select(
        &self,
        format: i32,
        requested: Option<&[u8]>,
    ) -> Result<Option<Arc<dyn ItemDecoder>>, ContextError> {
        Ok(
            crate::plugin_registry::select_decoder(format, requested)?.map(|plugin| {
                Arc::new(Decoder {
                    plugin,
                    parent: self.0.clone(),
                    format,
                }) as Arc<dyn ItemDecoder>
            }),
        )
    }
}
fn callback_error(error: HeifError, unpack: bool) -> ContextError {
    let code = libheifer::error_text::code_text(error.code);
    let subcode = libheifer::error_text::subcode_text(error.subcode);
    let text = if error.message.is_null() {
        std::borrow::Cow::Borrowed("")
    } else {
        unsafe { CStr::from_ptr(error.message) }.to_string_lossy()
    };
    let mut detail = text.as_ref();
    if unpack && let Some(rest) = detail.strip_prefix(code) {
        detail = rest.strip_prefix(": ").unwrap_or(rest);
        if let Some(rest) = detail.strip_prefix(subcode) {
            detail = rest.strip_prefix(": ").unwrap_or(rest);
        }
    }
    let mut message = libheifer::error_text::message(error.code, error.subcode);
    if !detail.is_empty() {
        message.push_str(": ");
        message.push_str(detail);
    }
    ContextError::new(error.code, error.subcode, message)
}
struct Instance {
    plugin: *const DecoderPlugin,
    state: *mut c_void,
}
impl Drop for Instance {
    fn drop(&mut self) {
        if !self.state.is_null()
            && let Some(f) = field!(self.plugin, free_decoder)
        {
            unsafe { f(self.state) }
        }
    }
}
impl ItemDecoder for Decoder {
    fn validate(&self) -> Result<(), ContextError> {
        if field!(self.plugin, plugin_api_version) < 5 {
            Err(loading("Decoder plugin needs to be at least version 5."))
        } else {
            Ok(())
        }
    }
    fn decode(
        &self,
        document: &Document,
        id: u32,
        options: &DecodeOptions,
    ) -> Result<Image, ContextError> {
        let parent = self
            .parent
            .upgrade()
            .ok_or_else(|| loading("Decoder context is no longer available."))?;
        let mut limits = unsafe { parent.limits.pointer().read() };
        limits.parent = if limits.version >= 4 && !limits.parent.is_null() {
            limits.parent
        } else {
            parent.limits.pointer()
        };
        limits.version = 4;
        let info = &document.images[&id];
        let padding = match self.format {
            4 | 5 => 128,
            2 | 3 => 16,
            _ => 64,
        };
        if info.ispe.0 != 0
            && info.ispe.1 != 0
            && let Some(pixels) =
                (u64::from(info.ispe.0) + padding).checked_mul(u64::from(info.ispe.1) + padding)
        {
            let maximum = pixels.max(65536);
            if limits.max_image_size_pixels == 0 || maximum < limits.max_image_size_pixels {
                limits.max_image_size_pixels = maximum;
            }
        }
        let container = document.container()?;
        if self.format == 1
            && let Some((w, h)) =
                libheifer::hevc_config::coded_size(container.property(id, *b"hvcC")?)?
        {
            let mut core_limits = document.current_limits();
            core_limits.max_image_size_pixels = limits.max_image_size_pixels;
            core_limits.check_image_size(w, h)?;
        }
        let p = self.plugin;
        let version = field!(p, plugin_api_version);
        let Some(old_new) = field!(p, new_decoder) else {
            return Err(loading("Cannot decode with a dummy decoder plugin."));
        };
        let mut instance = Instance {
            plugin: p,
            state: ptr::null_mut(),
        };
        let err = if version >= 5 {
            let Some(new) = field!(p, new_decoder2) else {
                return Err(loading("Cannot decode with a dummy decoder plugin."));
            };
            let opts = DecoderPluginOptions {
                format: self.format,
                strict_decoding: options.plugin_strict,
                num_threads: options.num_codec_threads,
                limits: &limits,
            };
            unsafe { new(&mut instance.state, &opts) }
        } else {
            unsafe { old_new(&mut instance.state) }
        };
        if err.code != 0 {
            return Err(callback_error(err, false));
        }
        if (2..5).contains(&version)
            && let Some(f) = field!(p, set_strict_decoding)
        {
            unsafe { f(instance.state, options.plugin_strict) };
        }
        let mut data = libheifer::decoding::codec_configuration(&container, id, self.format)?;
        let payload = libheifer::decoding::decoder_payload(document, id)?;
        data.extend_from_slice(&payload);
        if data.is_empty() {
            return Err(ContextError::invalid(
                0,
                "Unspecified: Input with empty data extent.",
            ));
        }
        let err = if version >= 5
            && let Some(f) = field!(p, push_data2)
        {
            unsafe { f(instance.state, data.as_ptr().cast(), data.len(), 0) }
        } else if let Some(f) = field!(p, push_data) {
            unsafe { f(instance.state, data.as_ptr().cast(), data.len()) }
        } else {
            return Err(loading("Cannot decode with a dummy decoder plugin."));
        };
        if err.code != 0 {
            return Err(callback_error(err, false));
        }
        // The still-image API intentionally ignores the flush callback's status.
        if version >= 5
            && let Some(f) = field!(p, flush_data)
        {
            unsafe { f(instance.state) };
        }
        for _ in 0..50 {
            if instance.state.is_null() {
                break;
            }
            let mut image = ptr::null_mut();
            let err = if version >= 5
                && let Some(f) = field!(p, decode_next_image2)
            {
                unsafe { f(instance.state, &mut image, ptr::null_mut(), &limits) }
            } else if version >= 4
                && let Some(f) = field!(p, decode_next_image)
            {
                unsafe { f(instance.state, &mut image, &limits) }
            } else if let Some(f) = field!(p, decode_image) {
                unsafe { f(instance.state, &mut image) }
            } else {
                return Err(loading("Cannot decode with a dummy decoder plugin."));
            };
            if err.code != 0 {
                return Err(callback_error(err, true));
            }
            if !image.is_null() {
                return Ok(*unsafe { Box::from_raw(image) });
            }
        }
        Err(ContextError::new(
            7,
            0,
            "Decoder plugin generated an error: Unspecified: Decoding the input data did not give a decompressed image.",
        ))
    }
}
