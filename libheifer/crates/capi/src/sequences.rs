// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::{
    HeifError, SUCCESS,
    context::{HeifContext, SharedContext, lock, report},
};
use libheifer::{
    error::Error,
    sequence_sample::RawSample,
    sequences::{AuxType, SharedTrack, TrackOptions},
    tai::ClockInfo,
};
use std::{
    ffi::{CStr, c_char},
    ptr,
    sync::Arc,
};
pub struct HeifTrack {
    shared: Arc<SharedContext>,
    track: SharedTrack,
}
fn owned(data: &[u8]) -> *const c_char {
    let mut bytes = data.to_vec();
    bytes.push(0);
    Box::into_raw(bytes.into_boxed_slice()).cast::<u8>().cast()
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_track_options_alloc() -> *mut TrackOptions {
    Box::into_raw(Box::new(TrackOptions::default()))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_options_release(p: *mut TrackOptions) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(p) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_options_set_timescale(p: *mut TrackOptions, v: u32) {
    if let Some(p) = unsafe { p.as_mut() } {
        p.timescale = v;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_options_set_interleaved_sample_aux_infos(
    p: *mut TrackOptions,
    v: i32,
) {
    if let Some(p) = unsafe { p.as_mut() } {
        p.interleaved = v != 0;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_options_enable_sample_gimi_content_ids(
    p: *mut TrackOptions,
    v: i32,
) {
    if let Some(p) = unsafe { p.as_mut() } {
        p.content_presence = v;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_options_set_gimi_track_id(
    p: *mut TrackOptions,
    v: *const c_char,
) {
    if let Some(p) = unsafe { p.as_mut() } {
        p.content_id = if v.is_null() {
            Vec::new()
        } else {
            unsafe { CStr::from_ptr(v) }.to_bytes().to_vec()
        };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_options_enable_sample_tai_timestamps(
    p: *mut TrackOptions,
    v: *const ClockInfo,
    presence: i32,
) -> HeifError {
    if presence != 0 && v.is_null() {
        return Error::new(
            5,
            0,
            c"NULL tai clock info passed for track with TAI timestamps",
        )
        .into();
    }
    let Some(p) = (unsafe { p.as_mut() }) else {
        return Error::NULL.into();
    };
    p.tai_presence = presence;
    p.clock = if v.is_null() {
        None
    } else {
        let mut c = Box::<ClockInfo>::default();
        unsafe { crate::tai::heif_tai_clock_info_copy(&mut *c, v) };
        Some(c)
    };
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_release(p: *mut HeifTrack) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(p) });
    }
}
macro_rules! context_query {
    ($name:ident,$ty:ty,$body:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(ctx: *const HeifContext) -> $ty {
            let Some(ctx) = (unsafe { ctx.as_ref() }) else {
                return 0;
            };
            let state = lock(&ctx.shared);
            ($body)(&state.sequences)
        }
    };
}
context_query!(
    heif_context_has_sequence,
    i32,
    |s: &libheifer::sequences::Sequences| i32::from(!s.tracks.is_empty())
);
context_query!(
    heif_context_number_of_sequence_tracks,
    i32,
    |s: &libheifer::sequences::Sequences| s.tracks.len() as i32
);
context_query!(
    heif_context_get_sequence_timescale,
    u32,
    |s: &libheifer::sequences::Sequences| s.timescale
);
context_query!(
    heif_context_get_sequence_duration,
    u64,
    |s: &libheifer::sequences::Sequences| s.duration
);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_sequence_timescale(ctx: *mut HeifContext, v: u32) {
    if let Some(ctx) = unsafe { ctx.as_ref() } {
        let mut s = lock(&ctx.shared);
        s.init_sequence();
        s.sequences.timescale = v;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_set_number_of_sequence_repetitions(
    ctx: *mut HeifContext,
    v: u32,
) {
    if let Some(ctx) = unsafe { ctx.as_ref() } {
        lock(&ctx.shared).sequences.repetitions = v;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_track_ids(ctx: *const HeifContext, out: *mut u32) {
    if ctx.is_null() || out.is_null() {
        return;
    }
    for (i, id) in lock(&unsafe { &*ctx }.shared)
        .sequences
        .tracks
        .keys()
        .enumerate()
    {
        unsafe { out.add(i).write(*id) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_track(
    ctx: *const HeifContext,
    id: u32,
) -> *mut HeifTrack {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return ptr::null_mut();
    };
    let track = lock(&ctx.shared).sequences.get(id);
    track.map_or(ptr::null_mut(), |track| {
        Box::into_raw(Box::new(HeifTrack {
            shared: ctx.shared.clone(),
            track,
        }))
    })
}
unsafe fn add(
    ctx: *mut HeifContext,
    handler: u32,
    dimensions: (u16, u16),
    uri: Option<Vec<u8>>,
    options: *const TrackOptions,
    out: *mut *mut HeifTrack,
) -> HeifError {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    let opts = unsafe { options.as_ref() }.cloned().unwrap_or_default();
    let mut state = lock(&ctx.shared);
    match state.add_track(handler, dimensions, uri, opts) {
        Err(e) => report(&mut state, e),
        Ok(track) => {
            if !out.is_null() {
                unsafe {
                    out.write(Box::into_raw(Box::new(HeifTrack {
                        shared: ctx.shared.clone(),
                        track,
                    })))
                };
            }
            SUCCESS
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_visual_sequence_track(
    ctx: *mut HeifContext,
    w: u16,
    h: u16,
    kind: u32,
    opts: *const TrackOptions,
    _encoding: *const crate::encoding_options::SequenceEncodingOptions,
    out: *mut *mut HeifTrack,
) -> HeifError {
    if kind != u32::from_be_bytes(*b"pict") && kind != u32::from_be_bytes(*b"vide") {
        return Error::new(
            5,
            2006,
            c"visual track has to be of type video or image sequence",
        )
        .into();
    }
    unsafe { add(ctx, kind, (w, h), None, opts, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_add_uri_metadata_sequence_track(
    ctx: *mut HeifContext,
    uri: *const c_char,
    opts: *const TrackOptions,
    out: *mut *mut HeifTrack,
) -> HeifError {
    if uri.is_null() {
        return Error::NULL.into();
    }
    unsafe {
        add(
            ctx,
            u32::from_be_bytes(*b"meta"),
            (0, 0),
            Some(CStr::from_ptr(uri).to_bytes().to_vec()),
            opts,
            out,
        )
    }
}
macro_rules! track_query {
    ($name:ident,$ty:ty,$default:expr,$body:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(track: *const HeifTrack) -> $ty {
            let Some(track) = (unsafe { track.as_ref() }) else {
                return $default;
            };
            let t = track.track.lock().unwrap();
            ($body)(&*t)
        }
    };
}
track_query!(
    heif_track_get_id,
    u32,
    0,
    |t: &libheifer::sequences::Track| t.id
);
track_query!(
    heif_track_get_track_handler_type,
    u32,
    0,
    |t: &libheifer::sequences::Track| t.reported_handler
);
track_query!(
    heif_track_get_timescale,
    u32,
    0,
    |t: &libheifer::sequences::Track| t.options.timescale
);
track_query!(
    heif_track_get_number_of_repetitions,
    u32,
    0,
    |t: &libheifer::sequences::Track| t.repetitions
);
track_query!(
    heif_track_get_sample_entry_type_of_first_cluster,
    u32,
    0,
    |t: &libheifer::sequences::Track| t.entry_kind
);
track_query!(
    heif_track_get_number_of_sample_aux_infos,
    i32,
    0,
    |t: &libheifer::sequences::Track| t.aux_types.len() as i32
);
track_query!(
    heif_track_has_alpha_channel,
    i32,
    0,
    |t: &libheifer::sequences::Track| i32::from(t.alpha)
);
track_query!(
    heif_track_get_auxiliary_info_type,
    i32,
    0,
    |t: &libheifer::sequences::Track| i32::from(
        [
            b"urn:mpeg:hevc:2015:auxid:1".as_slice(),
            b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha",
            b"urn:mpeg:avc:2015:auxid:1"
        ]
        .contains(&t.auxiliary_urn.as_slice())
    )
);
track_query!(
    heif_track_get_auxiliary_info_type_urn,
    *const c_char,
    ptr::null(),
    |t: &libheifer::sequences::Track| if t.auxiliary_urn.is_empty() {
        ptr::null()
    } else {
        owned(&t.auxiliary_urn)
    }
);
track_query!(
    heif_track_get_gimi_track_content_id,
    *const c_char,
    ptr::null(),
    |t: &libheifer::sequences::Track| if t.options.content_id.is_empty() {
        ptr::null()
    } else {
        owned(&t.options.content_id)
    }
);
track_query!(
    heif_track_get_tai_clock_info_of_first_cluster,
    *const ClockInfo,
    ptr::null(),
    |t: &libheifer::sequences::Track| t
        .first_clock
        .as_deref()
        .map_or(ptr::null(), |x| x as *const _)
);
track_query!(
    heif_track_get_number_of_track_reference_types,
    usize,
    0,
    |t: &libheifer::sequences::Track| t.references.len()
);
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_get_image_resolution(
    track: *const HeifTrack,
    w: *mut u16,
    h: *mut u16,
) -> HeifError {
    let Some(track) = (unsafe { track.as_ref() }) else {
        return Error::NULL.into();
    };
    let t = track.track.lock().unwrap();
    if !t.visual() {
        return Error::new(5, 2006, c"Cannot get resolution of non-visual track.").into();
    }
    if !w.is_null() {
        unsafe { w.write(t.dimensions.0) };
    }
    if !h.is_null() {
        unsafe { h.write(t.dimensions.1) };
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_get_urim_sample_entry_uri_of_first_cluster(
    track: *const HeifTrack,
    out: *mut *const c_char,
) -> HeifError {
    let Some(track) = (unsafe { track.as_ref() }) else {
        return Error::NULL.into();
    };
    let result = track.track.lock().unwrap().first_uri().map(|s| s.to_vec());
    match result {
        Err(e) => report(&mut lock(&track.shared), e),
        Ok(uri) => {
            if !out.is_null() {
                unsafe { out.write(owned(&uri)) };
            }
            SUCCESS
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_add_raw_sequence_sample(
    track: *mut HeifTrack,
    sample: *const RawSample,
) -> HeifError {
    let Some(track) = (unsafe { track.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut t = track.track.lock().unwrap();
    if t.visual() {
        return Error::new(5, 2006, c"Cannot save metadata in a non-metadata track.").into();
    }
    let Some(sample) = (unsafe { sample.as_ref() }) else {
        return Error::NULL.into();
    };
    let result = t.add_raw(sample);
    drop(t);
    match result {
        Err(e) => report(&mut lock(&track.shared), e),
        Ok(()) => SUCCESS,
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_get_next_raw_sequence_sample(
    track: *mut HeifTrack,
    out: *mut *mut RawSample,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    let Some(track) = (unsafe { track.as_ref() }) else {
        return Error::NULL.into();
    };
    let result = track.track.lock().unwrap().next_raw();
    match result {
        Err(e) => report(&mut lock(&track.shared), e),
        Ok(s) => {
            unsafe { out.write(Box::into_raw(Box::new(s))) };
            SUCCESS
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_get_sample_aux_info_types(
    track: *const HeifTrack,
    out: *mut AuxType,
) {
    if track.is_null() || out.is_null() {
        return;
    }
    for (i, t) in unsafe { &*track }
        .track
        .lock()
        .unwrap()
        .aux_types
        .iter()
        .enumerate()
    {
        unsafe { out.add(i).write(*t) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_add_reference_to_track(
    track: *mut HeifTrack,
    kind: u32,
    to: *const HeifTrack,
) {
    if track.is_null() || to.is_null() {
        return;
    }
    let id = unsafe { &*to }.track.lock().unwrap().id;
    unsafe { &*track }
        .track
        .lock()
        .unwrap()
        .add_reference(kind, id);
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_get_track_reference_types(
    track: *const HeifTrack,
    out: *mut u32,
) {
    if track.is_null() || out.is_null() {
        return;
    }
    for (i, (kind, _)) in unsafe { &*track }
        .track
        .lock()
        .unwrap()
        .references
        .iter()
        .enumerate()
    {
        unsafe { out.add(i).write(*kind) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_get_number_of_track_reference_of_type(
    track: *const HeifTrack,
    kind: u32,
) -> usize {
    if track.is_null() {
        return 0;
    }
    unsafe { &*track }
        .track
        .lock()
        .unwrap()
        .references(kind)
        .len()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_get_references_from_track(
    track: *const HeifTrack,
    kind: u32,
    out: *mut u32,
) -> usize {
    if track.is_null() {
        return 0;
    }
    let t = unsafe { &*track }.track.lock().unwrap();
    let refs = t.references(kind);
    if !out.is_null() {
        for (i, id) in refs.iter().enumerate() {
            unsafe { out.add(i).write(*id) };
        }
    }
    refs.len()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_find_referring_tracks(
    track: *const HeifTrack,
    kind: u32,
    out: *mut u32,
    capacity: usize,
) -> usize {
    if track.is_null() || out.is_null() {
        return 0;
    }
    let track = unsafe { &*track };
    let id = track.track.lock().unwrap().id;
    let state = lock(&track.shared);
    let mut count = 0;
    for (&other, t) in &state.sequences.tracks {
        if count == capacity {
            break;
        }
        if other != id && t.lock().unwrap().references(kind).contains(&id) {
            unsafe { out.add(count).write(other) };
            count += 1;
        }
    }
    count
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_encode_sequence_image(
    track: *mut HeifTrack,
    image: *const libheifer::image::Image,
    encoder: *mut crate::encoder::Encoder,
    options: *const crate::encoding_options::SequenceEncodingOptions,
) -> HeifError {
    let Some(track) = (unsafe { track.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut t = track.track.lock().unwrap();
    if !t.visual() {
        return Error::new(5, 2006, c"Cannot encode image for non-visual track.").into();
    }
    let (Some(image), Some(enc)) = (unsafe { image.as_ref() }, unsafe { encoder.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut copied = crate::encoding_options::SequenceEncodingOptions::default();
    unsafe { crate::encoding_options::heif_sequence_encoding_options_copy(&mut copied, options) };
    let result = (|| {
        use libheifer::context::ContextError;
        if image.width > 65535 || image.height > 65535 {
            return Err(ContextError::invalid(
                0,
                "Unspecified: Input image resolution too high",
            ));
        }
        if t.active_encoder == 0 {
            t.active_encoder = encoder as usize;
        } else if t.active_encoder != encoder as usize {
            return Err(ContextError::new(
                5,
                0,
                "Usage error: Unspecified: You may not switch the heif_encoder while encoding a sequence.",
            ));
        }
        let version = match enc.source {
            crate::plugin_registry::EncoderSource::Builtin(8) => 4,
            crate::plugin_registry::EncoderSource::Builtin(_) => 3,
            crate::plugin_registry::EncoderSource::External(p) => {
                crate::plugin_registry::field!(p, plugin_api_version)
            }
        };
        if version < 4 {
            return Err(ContextError::new(
                11,
                6003,
                "Plugin loading error: Support for this compression format has not been built in: Encoder plugin needs to be at least version 4.",
            ));
        }
        if enc.source.format() == 8 {
            t.encode_uncompressed(image)
        } else {
            Err(ContextError::new(
                4,
                6003,
                "Unsupported feature: Support for this compression format has not been built in",
            ))
        }
    })();
    drop(t);
    match result {
        Err(e) => report(&mut lock(&track.shared), e),
        Ok(()) => SUCCESS,
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_encode_end_of_sequence(
    track: *mut HeifTrack,
    _encoder: *mut crate::encoder::Encoder,
) -> HeifError {
    let Some(track) = (unsafe { track.as_ref() }) else {
        return Error::NULL.into();
    };
    let t = track.track.lock().unwrap();
    if !t.visual() {
        return Error::new(5, 2006, c"Cannot encode image for non-visual track.").into();
    }
    // The uncompressed encoder emits each complete frame synchronously.
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_track_decode_next_image(
    track: *mut HeifTrack,
    out: *mut *mut libheifer::image::Image,
    colorspace: i32,
    chroma: i32,
    options: *const crate::decoding::DecodingOptions,
) -> HeifError {
    if out.is_null() {
        return Error::NULL.into();
    }
    let Some(track) = (unsafe { track.as_ref() }) else {
        return Error::NULL.into();
    };
    let mut t = track.track.lock().unwrap();
    if !t.decode_failed && u64::from(t.next) >= t.output_count {
        unsafe { out.write(ptr::null_mut()) };
        return Error::new(13, 0, c"End of sequence").into();
    }
    if !t.visual() {
        return Error::new(5, 2006, c"Cannot get image from non-visual track.").into();
    }
    let mut opts = crate::decoding::DecodingOptions::default();
    unsafe { crate::decoding::heif_decoding_options_copy(&mut opts, options) };
    let state = lock(&track.shared);
    // Coded tracks select their decoder plugin as libheif's track decoding does.
    let provider = crate::plugin_decoding::Provider(Arc::downgrade(&track.shared));
    let coded = matches!(
        &t.entry_kind.to_be_bytes(),
        b"avc1" | b"hvc1" | b"hev1" | b"av01"
    );
    let core = libheifer::decoding::DecodeOptions {
        decoder_provider: coded.then_some(&provider as &dyn libheifer::decoding::DecoderProvider),
        decoder_id: if opts.decoder_id.is_null() {
            None
        } else {
            Some(unsafe { std::ffi::CStr::from_ptr(opts.decoder_id) }.to_bytes())
        },
        plugin_strict: i32::from(opts.strict_decoding),
        num_codec_threads: opts.num_codec_threads,
        ignore_transformations: opts.ignore_transformations != 0,
        strict: opts.strict_decoding != 0,
        profile_passthrough: opts.output_image_nclx_profile_passthrough != 0,
        output_nclx: unsafe { opts.output_image_nclx_profile.as_ref() }.map(|p| {
            libheifer::color::Nclx {
                primaries: p.color_primaries as u16,
                transfer: 2,
                matrix: p.matrix_coefficients as u16,
                full_range: p.full_range_flag != 0,
            }
        }),
        convert_hdr_to_8bit: opts.convert_hdr_to_8bit != 0,
        color_conversion: opts.color_conversion_options,
        ..Default::default()
    };
    let result = t.decode_next(
        &state,
        colorspace,
        chroma,
        core,
        opts.ignore_sequence_editlist != 0,
    );
    drop(t);
    drop(state);
    match result {
        Err(e) => report(&mut lock(&track.shared), e),
        Ok(img) => {
            unsafe { out.write(Box::into_raw(Box::new(img))) };
            HeifError {
                code: 0,
                subcode: 0,
                message: ptr::null(),
            }
        }
    }
}
