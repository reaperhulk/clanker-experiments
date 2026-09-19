// SPDX-License-Identifier: LGPL-3.0-or-later
// Compatibility semantics adapted from libheif, Copyright Dirk Farin and contributors.
use libheifer::color::{ColorConversionOptions, ColorConversionOptionsExt, NclxProfile};
use std::{
    ffi::{c_char, c_int, c_void},
    ptr,
};

type Progress = Option<unsafe extern "C" fn(c_int, c_int, *mut c_void)>;
type EndProgress = Option<unsafe extern "C" fn(c_int, *mut c_void)>;
type Cancel = Option<unsafe extern "C" fn(*mut c_void) -> c_int>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DecodingOptions {
    pub version: u8,
    pub ignore_transformations: u8,
    pub start_progress: Progress,
    pub on_progress: Progress,
    pub end_progress: EndProgress,
    pub progress_user_data: *mut c_void,
    pub convert_hdr_to_8bit: u8,
    pub strict_decoding: u8,
    pub decoder_id: *const c_char,
    pub color_conversion_options: ColorConversionOptions,
    pub cancel_decoding: Cancel,
    pub color_conversion_options_ext: *mut ColorConversionOptionsExt,
    pub ignore_sequence_editlist: c_int,
    pub output_image_nclx_profile: *mut NclxProfile,
    pub num_library_threads: c_int,
    pub num_codec_threads: c_int,
    pub autocorrect_broken_input: u8,
    pub output_image_nclx_profile_passthrough: u8,
}
impl Default for DecodingOptions {
    fn default() -> Self {
        Self {
            version: 10,
            ignore_transformations: 0,
            start_progress: None,
            on_progress: None,
            end_progress: None,
            progress_user_data: ptr::null_mut(),
            convert_hdr_to_8bit: 0,
            strict_decoding: 0,
            decoder_id: ptr::null(),
            // These defaults intentionally differ from the public defaults
            // helper: the pinned decoding-options allocator permits fallback.
            color_conversion_options: ColorConversionOptions {
                version: 1,
                preferred_chroma_downsampling_algorithm: 2,
                preferred_chroma_upsampling_algorithm: 2,
                only_use_preferred_chroma_algorithm: 0,
            },
            cancel_decoding: None,
            color_conversion_options_ext: ptr::null_mut(),
            ignore_sequence_editlist: 0,
            output_image_nclx_profile: ptr::null_mut(),
            num_library_threads: 0,
            num_codec_threads: 0,
            autocorrect_broken_input: 0,
            output_image_nclx_profile_passthrough: 0,
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_decoding_options_alloc() -> *mut DecodingOptions {
    super::color::allocate(DecodingOptions::default())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_decoding_options_free(options: *mut DecodingOptions) {
    if !options.is_null() {
        unsafe { drop(Box::from_raw(options)) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_decoding_options_copy(
    dst: *mut DecodingOptions,
    src: *const DecodingOptions,
) {
    if src.is_null() || dst.is_null() {
        return;
    }
    // Do not form references to the full struct: an older caller can allocate
    // only the prefix belonging to its version. Access each negotiated field.
    let version = unsafe {
        ptr::addr_of!((*dst).version)
            .read()
            .min(ptr::addr_of!((*src).version).read())
    };
    // Upstream's switch deliberately copies nothing for unknown versions.
    if !(1..=10).contains(&version) {
        return;
    }
    macro_rules! copy {
        ($($field:ident),+ $(,)?) => { $( unsafe {
            ptr::addr_of_mut!((*dst).$field).write(ptr::addr_of!((*src).$field).read());
        } )+ };
    }
    copy!(
        ignore_transformations,
        start_progress,
        on_progress,
        end_progress,
        progress_user_data
    );
    if version >= 2 {
        copy!(convert_hdr_to_8bit);
    }
    if version >= 3 {
        copy!(strict_decoding);
    }
    if version >= 4 {
        copy!(decoder_id);
    }
    if version >= 5 {
        copy!(color_conversion_options);
    }
    if version >= 6 {
        copy!(cancel_decoding);
    }
    if version >= 7 {
        copy!(color_conversion_options_ext);
    }
    if version >= 8 {
        copy!(
            num_library_threads,
            num_codec_threads,
            output_image_nclx_profile,
            ignore_sequence_editlist
        );
    }
    if version >= 9 {
        copy!(autocorrect_broken_input);
    }
    if version >= 10 {
        copy!(output_image_nclx_profile_passthrough);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_decode_image(
    handle: *const super::context::HeifHandle,
    out: *mut *mut libheifer::image::Image,
    colorspace: c_int,
    chroma: c_int,
    input_options: *const DecodingOptions,
) -> super::HeifError {
    use libheifer::{context::ContextError, error::Error};
    if out.is_null() || handle.is_null() {
        return Error::NULL.into();
    }
    unsafe {
        out.write(ptr::null_mut());
    }
    let handle = unsafe { &*handle };
    let mut options = DecodingOptions::default();
    unsafe {
        heif_decoding_options_copy(&mut options, input_options);
    }
    let (document, max_decoding_threads, has_iloc) = {
        let context = super::context::lock(&handle.shared);
        (
            context.decoding_document(),
            context.max_decoding_threads,
            context.items.has_iloc,
        )
    };
    let Some(document) = document else {
        return Error::new(2, 2000, c"Invalid input: Non-existing item ID referenced").into();
    };
    // A failed reload preserves the image model but clears file tables. Data
    // lookup belongs to the current file even when the handle predates it.
    if !has_iloc && document.images.contains_key(&handle.id) {
        let error = if document.images[&handle.id].kind == *b"iden" {
            ContextError::invalid(
                113,
                "No 'iref' box: No iref box available, but needed for iden image",
            )
        } else {
            ContextError::invalid(110, "No 'iloc' box")
        };
        return super::context::report_image(handle.image(), error);
    }
    let callbacks = Callbacks(&options);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        {
            let output_nclx = unsafe { options.output_image_nclx_profile.as_ref() }.map(|p| {
                libheifer::color::Nclx {
                    primaries: p.color_primaries as u16,
                    // The reference does not copy the explicitly requested transfer curve.
                    transfer: 2,
                    matrix: p.matrix_coefficients as u16,
                    full_range: p.full_range_flag != 0,
                }
            });
            libheifer::decoding::decode(
                &document,
                handle.id,
                colorspace,
                chroma,
                libheifer::decoding::DecodeOptions {
                    callbacks: Some(&callbacks),
                    max_decoding_threads,
                    decoder_id: if options.decoder_id.is_null() {
                        None
                    } else {
                        Some(unsafe { std::ffi::CStr::from_ptr(options.decoder_id) }.to_bytes())
                    },
                    ignore_transformations: options.ignore_transformations != 0,
                    strict: options.strict_decoding != 0,
                    output_nclx,
                    profile_passthrough: options.output_image_nclx_profile_passthrough != 0,
                    convert_hdr_to_8bit: options.convert_hdr_to_8bit != 0,
                    color_conversion: options.color_conversion_options,
                },
            )
        }
    }));
    match result {
        Ok(Ok(image)) => {
            let image = super::color::allocate(image);
            if image.is_null() {
                return Error::ALLOCATION.into();
            }
            unsafe {
                out.write(image);
            }
            super::SUCCESS
        }
        Ok(Err(error)) => super::context::report_image(handle.image(), error),
        Err(_) => super::context::report_image(
            handle.image(),
            ContextError::new(
                7,
                0,
                "Decoder plugin generated an error: Unspecified: Rust decoder panicked",
            ),
        ),
    }
}

struct Callbacks<'a>(&'a DecodingOptions);
// SAFETY: libheif progress callbacks can run on decoding workers. The caller's
// C contract keeps options and callback userdata alive and synchronized through
// heif_decode_image; scoped workers join before returning to that caller.
unsafe impl Sync for Callbacks<'_> {}
impl libheifer::decoding::DecodeCallbacks for Callbacks<'_> {
    fn start(&self, step: i32, maximum: i32) {
        if let Some(f) = self.0.start_progress {
            unsafe {
                f(step, maximum, self.0.progress_user_data);
            }
        }
    }
    fn progress(&self, step: i32, value: i32) {
        if let Some(f) = self.0.on_progress {
            unsafe {
                f(step, value, self.0.progress_user_data);
            }
        }
    }
    fn end(&self, step: i32) {
        if let Some(f) = self.0.end_progress {
            unsafe {
                f(step, self.0.progress_user_data);
            }
        }
    }
    fn canceled(&self) -> bool {
        self.0
            .cancel_decoding
            .is_some_and(|f| unsafe { f(self.0.progress_user_data) != 0 })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_max_decoding_threads(
    context: *mut super::context::HeifContext,
    threads: c_int,
) {
    if let Some(context) = unsafe { context.as_ref() } {
        super::context::lock(&context.shared).max_decoding_threads = threads;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_max_decoding_threads(
    context: *const super::context::HeifContext,
) -> c_int {
    unsafe { context.as_ref() }.map_or(4, |context| {
        super::context::lock(&context.shared).max_decoding_threads
    })
}
