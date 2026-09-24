// SPDX-License-Identifier: LGPL-3.0-or-later
//! Registry ownership is separate from foreign plugin storage. Client plugins
//! must remain immutable and alive while registered, as in the public C contract.
use crate::{HeifError, SUCCESS, context::HeifContext, plugin_types::*};
use libheifer::error::Error;
use std::{
    ffi::{CStr, c_char, c_int, c_void},
    ptr,
    sync::{Arc, LazyLock, Mutex},
};
macro_rules! field {
    ($p:expr,$f:ident) => {
        unsafe { ptr::addr_of!((*$p).$f).read() }
    };
}
pub(super) use field;
#[derive(Clone, Copy)]
pub(super) enum EncoderSource {
    Builtin(c_int),
    External(*const EncoderPlugin),
}
// SAFETY: registered plugin records have immutable, caller-owned lifetime. The
// library does not transfer ownership of or mutate foreign records.
unsafe impl Send for EncoderSource {}
unsafe impl Sync for EncoderSource {}
pub struct EncoderDescriptor {
    pub(super) source: EncoderSource,
}
#[derive(Clone, Copy)]
pub(super) enum DecoderSource {
    Builtin(c_int),
    External(*const DecoderPlugin),
}
// SAFETY: same immutable registered-record contract as EncoderSource.
unsafe impl Send for DecoderSource {}
unsafe impl Sync for DecoderSource {}
pub struct DecoderDescriptor {
    _private: [u8; 0],
}
struct DecoderRecord {
    source: DecoderSource,
}
struct Registry {
    count: usize,
    defaults: bool,
    encoders: Vec<Arc<EncoderDescriptor>>,
    decoders: Vec<Arc<DecoderRecord>>,
}
impl Registry {
    fn add_defaults(&mut self) {
        for format in [8, 9] {
            self.encoders.push(Arc::new(EncoderDescriptor {
                source: EncoderSource::Builtin(format),
            }));
        }
        #[cfg(feature = "jpeg")]
        self.encoders.push(Arc::new(EncoderDescriptor {
            source: EncoderSource::External(&crate::builtin_jpeg_encoder::JPEG_ENCODER.0),
        }));
        #[cfg(feature = "jpeg2000")]
        self.encoders.push(Arc::new(EncoderDescriptor {
            source: EncoderSource::External(&crate::builtin_jpeg2000_encoder::JPEG2000_ENCODER.0),
        }));
        #[cfg(feature = "jpeg2000")]
        self.encoders.push(Arc::new(EncoderDescriptor {
            source: EncoderSource::External(&crate::builtin_htj2k_encoder::HTJ2K_ENCODER.0),
        }));
        #[cfg(feature = "av1")]
        self.encoders.push(Arc::new(EncoderDescriptor {
            source: EncoderSource::External(&crate::builtin_av1_encoder::AV1_ENCODER.0),
        }));
        self.decoders.push(Arc::new(DecoderRecord {
            source: DecoderSource::Builtin(8),
        }));
        #[cfg(feature = "jpeg2000")]
        self.decoders.push(Arc::new(DecoderRecord {
            source: DecoderSource::Builtin(7),
        }));
        #[cfg(feature = "jpeg")]
        self.decoders.push(Arc::new(DecoderRecord {
            source: DecoderSource::Builtin(3),
        }));
        #[cfg(feature = "avc")]
        self.decoders.push(Arc::new(DecoderRecord {
            source: DecoderSource::Builtin(2),
        }));
        #[cfg(feature = "av1")]
        self.decoders.push(Arc::new(DecoderRecord {
            source: DecoderSource::Builtin(4),
        }));
        #[cfg(feature = "hevc")]
        self.decoders.push(Arc::new(DecoderRecord {
            source: DecoderSource::Builtin(1),
        }));
        #[cfg(feature = "vvc")]
        self.decoders.push(Arc::new(DecoderRecord {
            source: DecoderSource::Builtin(5),
        }));
        self.decoders.sort_by_key(|d| registry_order(&d.source));
        self.encoders
            .sort_by_key(|d| std::cmp::Reverse(d.source.priority()));
        self.defaults = true;
    }
}
static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| {
    let mut r = Registry {
        count: 0,
        defaults: false,
        encoders: Vec::new(),
        decoders: Vec::new(),
    };
    r.add_defaults();
    Mutex::new(r)
});
pub(super) fn ensure_initialized() {
    if REGISTRY.lock().unwrap().count == 0 {
        let _ = heif_init(ptr::null_mut());
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_init(_params: *mut c_void) -> HeifError {
    let first = {
        let mut r = REGISTRY.lock().unwrap();
        if !r.defaults {
            r.add_defaults();
        }
        r.count == 0
    };
    if first {
        for path in crate::dynamic_plugins::paths() {
            let error = unsafe {
                crate::dynamic_plugins::heif_load_plugins(
                    path.as_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    0,
                )
            };
            if error.code != 0 {
                return error;
            }
        }
    }
    REGISTRY.lock().unwrap().count += 1;
    SUCCESS
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_deinit() {
    let (encoders, decoders) = {
        let mut r = REGISTRY.lock().unwrap();
        if r.count == 0 {
            return;
        }
        if r.count > 1 {
            r.count -= 1;
            return;
        }
        (r.encoders.clone(), r.decoders.clone())
    };
    // Callbacks can re-enter discovery; never retain the registry mutex across them.
    for d in decoders {
        if let DecoderSource::External(p) = d.source
            && let Some(f) = field!(p, deinit_plugin)
        {
            unsafe { f() }
        }
    }
    REGISTRY.lock().unwrap().decoders.clear();
    for d in encoders {
        if let EncoderSource::External(p) = d.source
            && let Some(f) = field!(p, cleanup_plugin)
        {
            unsafe { f() }
        }
    }
    REGISTRY.lock().unwrap().encoders.clear();
    crate::dynamic_plugins::unload_all();
    let mut r = REGISTRY.lock().unwrap();
    r.encoders.clear();
    r.decoders.clear();
    r.defaults = false;
    r.count = r.count.saturating_sub(1);
}
fn version_error() -> HeifError {
    Error::new(5, 2003, c"Unsupported plugin version").into()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_register_encoder_plugin(p: *const EncoderPlugin) -> HeifError {
    if p.is_null() {
        return Error::NULL.into();
    }
    if field!(p, plugin_api_version) > 4 {
        return version_error();
    }
    if let Some(f) = field!(p, init_plugin) {
        unsafe { f() }
    }
    let mut r = REGISTRY.lock().unwrap();
    r.encoders.push(Arc::new(EncoderDescriptor {
        source: EncoderSource::External(p),
    }));
    r.encoders
        .sort_by_key(|d| std::cmp::Reverse(d.source.priority()));
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_register_decoder_plugin(p: *const DecoderPlugin) -> HeifError {
    if p.is_null() {
        return Error::NULL.into();
    }
    if field!(p, plugin_api_version) > 6 {
        return version_error();
    }
    if let Some(f) = field!(p, init_plugin) {
        unsafe { f() }
    }
    let mut r = REGISTRY.lock().unwrap();
    if !r
        .decoders
        .iter()
        .any(|d| matches!(d.source,DecoderSource::External(q) if p==q))
    {
        r.decoders.push(Arc::new(DecoderRecord {
            source: DecoderSource::External(p),
        }));
        r.decoders.sort_by_key(|d| registry_order(&d.source));
    }
    SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_register_decoder(
    _ctx: *mut HeifContext,
    p: *const DecoderPlugin,
) -> HeifError {
    unsafe { heif_register_decoder_plugin(p) }
}
impl EncoderSource {
    pub(super) fn priority(self) -> c_int {
        match self {
            Self::Builtin(_) => 60,
            Self::External(p) => field!(p, priority),
        }
    }
    pub(super) fn format(self) -> c_int {
        match self {
            Self::Builtin(v) => v,
            Self::External(p) => field!(p, compression_format),
        }
    }
    pub(super) fn name(self) -> *const c_char {
        match self {
            Self::Builtin(8) => c"builtin".as_ptr(),
            Self::Builtin(_) => c"mask".as_ptr(),
            Self::External(p) => field!(p, get_plugin_name).map_or(ptr::null(), |f| unsafe { f() }),
        }
    }
    pub(super) fn id(self) -> *const c_char {
        match self {
            Self::Builtin(8) => c"uncompressed".as_ptr(),
            Self::Builtin(_) => c"mask".as_ptr(),
            Self::External(p) => field!(p, id_name),
        }
    }
}
pub(super) unsafe fn encoders(format: c_int, name: *const c_char) -> Vec<Arc<EncoderDescriptor>> {
    ensure_initialized();
    REGISTRY
        .lock()
        .unwrap()
        .encoders
        .iter()
        .filter(|d| {
            (format == 0 || d.source.format() == format)
                && (name.is_null()
                    || unsafe { CStr::from_ptr(name) == CStr::from_ptr(d.source.id()) })
        })
        .cloned()
        .collect()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_get_encoder_descriptors(
    format: c_int,
    name: *const c_char,
    out: *mut *const EncoderDescriptor,
    count: c_int,
) -> c_int {
    if !out.is_null() && count <= 0 {
        return 0;
    }
    let descriptors = unsafe { encoders(format, name) };
    if out.is_null() {
        return descriptors.len() as c_int;
    }
    let n = (count as usize).min(descriptors.len());
    for (i, d) in descriptors.iter().take(n).enumerate() {
        unsafe { out.add(i).write(Arc::as_ptr(d)) }
    }
    n as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_get_encoder_descriptors(
    _ctx: *mut HeifContext,
    format: c_int,
    name: *const c_char,
    out: *mut *const EncoderDescriptor,
    count: c_int,
) -> c_int {
    unsafe { heif_get_encoder_descriptors(format, name, out, count) }
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_have_encoder_for_format(format: c_int) -> c_int {
    i32::from(!unsafe { encoders(format, ptr::null()) }.is_empty())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_descriptor_get_name(
    d: *const EncoderDescriptor,
) -> *const c_char {
    unsafe { (*d).source.name() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_descriptor_get_id_name(
    d: *const EncoderDescriptor,
) -> *const c_char {
    unsafe { (*d).source.id() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_encoder_descriptor_get_compression_format(
    d: *const EncoderDescriptor,
) -> c_int {
    unsafe { (*d).source.format() }
}
macro_rules! supports {
    ($name:ident,$f:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(d: *const EncoderDescriptor) -> c_int {
            match unsafe { (*d).source } {
                EncoderSource::Builtin(_) => 1,
                EncoderSource::External(p) => field!(p, $f),
            }
        }
    };
}
supports!(
    heif_encoder_descriptor_supports_lossy_compression,
    supports_lossy_compression
);
supports!(
    heif_encoder_descriptor_supports_lossless_compression,
    supports_lossless_compression
);
supports!(
    heif_encoder_descriptor_supportes_lossy_compression,
    supports_lossy_compression
);
supports!(
    heif_encoder_descriptor_supportes_lossless_compression,
    supports_lossless_compression
);
impl DecoderSource {
    fn priority(self, format: c_int) -> c_int {
        match self {
            // libheif's OpenH264 plugin reports priority 70 for AVC.
            Self::Builtin(2) => {
                if format == 2 {
                    70
                } else {
                    0
                }
            }
            Self::Builtin(v) => {
                if v == format {
                    100
                } else {
                    0
                }
            }
            Self::External(p) => field!(p, does_support_format).map_or(0, |f| unsafe { f(format) }),
        }
    }
    fn name(self) -> *const c_char {
        match self {
            Self::Builtin(8) => c"builtin".as_ptr(),
            Self::Builtin(7) => c"hayro-jpeg2000".as_ptr(),
            Self::Builtin(2) => c"rusty_h264".as_ptr(),
            Self::Builtin(3) => c"jpeg-decoder".as_ptr(),
            Self::Builtin(4) => c"rav1d".as_ptr(),
            Self::Builtin(5) => c"libheifer VVC decoder".as_ptr(),
            Self::Builtin(_) => c"rusty_h265".as_ptr(),
            Self::External(p) => field!(p, get_plugin_name).map_or(ptr::null(), |f| unsafe { f() }),
        }
    }
    fn id(self) -> *const c_char {
        match self {
            Self::Builtin(8) => c"uncompressed".as_ptr(),
            Self::Builtin(7) => c"hayro-jpeg2000".as_ptr(),
            Self::Builtin(2) => c"rusty_h264".as_ptr(),
            Self::Builtin(3) => c"jpeg-decoder".as_ptr(),
            Self::Builtin(4) => c"rav1d".as_ptr(),
            Self::Builtin(5) => c"libheifer-vvc".as_ptr(),
            Self::Builtin(_) => c"rusty_h265".as_ptr(),
            Self::External(p) => {
                if field!(p, plugin_api_version) < 3 {
                    ptr::null()
                } else {
                    field!(p, id_name)
                }
            }
        }
    }
}
fn decoders() -> Vec<Arc<DecoderRecord>> {
    ensure_initialized();
    REGISTRY.lock().unwrap().decoders.clone()
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_have_decoder_for_format(format: c_int) -> c_int {
    let mut best = 0;
    for d in decoders() {
        best = best.max(d.source.priority(format));
    }
    i32::from(best > 0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_get_decoder_descriptors(
    format: c_int,
    out: *mut *const DecoderDescriptor,
    count: c_int,
) -> c_int {
    let formats: Vec<_> = if format == 0 {
        vec![1, 4, 3, 7, 10, 5]
    } else {
        vec![format]
    };
    let mut found = Vec::new();
    for d in decoders() {
        for &f in &formats {
            let priority = d.source.priority(f);
            if priority != 0 {
                found.push((d, priority));
                break;
            }
        }
    }
    if out.is_null() {
        return found.len() as c_int;
    }
    found.sort_by_key(|(_, p)| std::cmp::Reverse(*p));
    let n = count.min(found.len() as c_int);
    for (i, (d, _)) in found.iter().take(n.max(0) as usize).enumerate() {
        unsafe {
            out.add(i).write(match d.source {
                DecoderSource::External(p) => p.cast(),
                DecoderSource::Builtin(format) => builtin_decoder(format).cast(),
            })
        }
    }
    n
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_decoder_descriptor_get_name(
    d: *const DecoderDescriptor,
) -> *const c_char {
    decoder_source(d).name()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_decoder_descriptor_get_id_name(
    d: *const DecoderDescriptor,
) -> *const c_char {
    decoder_source(d).id()
}

fn decoder_source(d: *const DecoderDescriptor) -> DecoderSource {
    DecoderSource::External(d.cast())
}
struct StaticDecoder(DecoderPlugin);
// SAFETY: these immutable records contain static strings and function pointers.
unsafe impl Sync for StaticDecoder {}
unsafe extern "C" fn builtin_name_8() -> *const c_char {
    DecoderSource::Builtin(8).name()
}
unsafe extern "C" fn builtin_name_7() -> *const c_char {
    DecoderSource::Builtin(7).name()
}
unsafe extern "C" fn builtin_name_3() -> *const c_char {
    DecoderSource::Builtin(3).name()
}
unsafe extern "C" fn builtin_name_2() -> *const c_char {
    DecoderSource::Builtin(2).name()
}
unsafe extern "C" fn builtin_name_4() -> *const c_char {
    DecoderSource::Builtin(4).name()
}
unsafe extern "C" fn builtin_name_5() -> *const c_char {
    DecoderSource::Builtin(5).name()
}
unsafe extern "C" fn builtin_priority_5(format: c_int) -> c_int {
    DecoderSource::Builtin(5).priority(format)
}
unsafe extern "C" fn builtin_name_1() -> *const c_char {
    DecoderSource::Builtin(1).name()
}
unsafe extern "C" fn builtin_priority_8(format: c_int) -> c_int {
    DecoderSource::Builtin(8).priority(format)
}
unsafe extern "C" fn builtin_priority_7(format: c_int) -> c_int {
    DecoderSource::Builtin(7).priority(format)
}
unsafe extern "C" fn builtin_priority_3(format: c_int) -> c_int {
    DecoderSource::Builtin(3).priority(format)
}
unsafe extern "C" fn builtin_priority_2(format: c_int) -> c_int {
    DecoderSource::Builtin(2).priority(format)
}
unsafe extern "C" fn builtin_priority_4(format: c_int) -> c_int {
    DecoderSource::Builtin(4).priority(format)
}
unsafe extern "C" fn builtin_priority_1(format: c_int) -> c_int {
    DecoderSource::Builtin(1).priority(format)
}
type NameFn = unsafe extern "C" fn() -> *const c_char;
type PriorityFn = unsafe extern "C" fn(c_int) -> c_int;
/// Descriptor record of a built-in decoder. Each built-in has its own static
/// record: descriptors, names and the registry order all derive from it.
const fn builtin_record(name: NameFn, priority: PriorityFn, id: &'static CStr) -> DecoderPlugin {
    DecoderPlugin {
        plugin_api_version: 5,
        get_plugin_name: Some(name),
        init_plugin: None,
        deinit_plugin: None,
        does_support_format: Some(priority),
        new_decoder: None,
        free_decoder: None,
        push_data: None,
        decode_image: None,
        set_strict_decoding: None,
        id_name: id.as_ptr(),
        decode_next_image: None,
        minimum_required_libheif_version: 0,
        does_support_format2: None,
        new_decoder2: None,
        push_data2: None,
        flush_data: None,
        decode_next_image2: None,
    }
}
static UNCOMPRESSED_DECODER: StaticDecoder = StaticDecoder(builtin_record(
    builtin_name_8,
    builtin_priority_8,
    c"uncompressed",
));
static JPEG2000_DECODER: StaticDecoder = StaticDecoder(builtin_record(
    builtin_name_7,
    builtin_priority_7,
    c"hayro-jpeg2000",
));
static JPEG_DECODER: StaticDecoder = StaticDecoder(builtin_record(
    builtin_name_3,
    builtin_priority_3,
    c"jpeg-decoder",
));
static AVC_DECODER: StaticDecoder = StaticDecoder(builtin_record(
    builtin_name_2,
    builtin_priority_2,
    c"rusty_h264",
));
static AV1_DECODER: StaticDecoder =
    StaticDecoder(builtin_record(builtin_name_4, builtin_priority_4, c"rav1d"));
static HEVC_DECODER: StaticDecoder = StaticDecoder(builtin_record(
    builtin_name_1,
    builtin_priority_1,
    c"rusty_h265",
));
static VVC_DECODER: StaticDecoder = StaticDecoder(builtin_record(
    builtin_name_5,
    builtin_priority_5,
    c"libheifer-vvc",
));
fn builtin_decoder(format: c_int) -> *const DecoderPlugin {
    match format {
        5 => &VVC_DECODER.0,
        8 => &UNCOMPRESSED_DECODER.0,
        7 => &JPEG2000_DECODER.0,
        3 => &JPEG_DECODER.0,
        2 => &AVC_DECODER.0,
        4 => &AV1_DECODER.0,
        _ => &HEVC_DECODER.0,
    }
}
/// libheif keeps decoder plugins in a `std::set` ordered by record address and
/// selects the first of equal priority, so a tie goes to the lower address (in
/// practice a heap-allocated registered plugin before a static built-in).
fn registry_order(source: &DecoderSource) -> usize {
    match *source {
        DecoderSource::External(p) => p as usize,
        DecoderSource::Builtin(format) => builtin_decoder(format) as usize,
    }
}

pub(super) unsafe fn unregister_encoder(p: *const EncoderPlugin) {
    if let Some(f) = field!(p, cleanup_plugin) {
        unsafe { f() }
    }
    let mut r = REGISTRY.lock().unwrap();
    if let Some(i) = r
        .encoders
        .iter()
        .position(|d| matches!(d.source,EncoderSource::External(q) if p==q))
    {
        r.encoders.remove(i);
    }
}

pub(super) fn decoder_registered(p: *const DecoderPlugin) -> bool {
    REGISTRY
        .lock()
        .unwrap()
        .decoders
        .iter()
        .any(|d| matches!(d.source,DecoderSource::External(q) if p==q))
}

// Snapshot registration before calling plugins: support queries may re-enter discovery.
pub(super) fn select_decoder(
    format: i32,
    requested: Option<&[u8]>,
) -> Result<Option<*const DecoderPlugin>, libheifer::context::ContextError> {
    let records = decoders();
    if let Some(id) = requested {
        let found = records.iter().any(|r| {
            let priority = r.source.priority(format);
            let name = r.source.id();
            priority > 0 && !name.is_null() && unsafe { CStr::from_ptr(name) }.to_bytes() == id
        });
        if !found {
            return Err(libheifer::context::ContextError::new(
                11,
                0,
                "Error while loading plugin: Unspecified: No decoder with that ID found.",
            ));
        }
    }
    let mut highest = 0;
    let mut selected = None;
    for record in records {
        let priority = record.source.priority(format);
        if priority > 0
            && let Some(requested) = requested
        {
            let id = record.source.id();
            if !id.is_null() && unsafe { CStr::from_ptr(id) }.to_bytes() == requested {
                selected = Some(record.source);
                break;
            }
        }
        if priority > highest {
            highest = priority;
            selected = Some(record.source);
        }
    }
    Ok(match selected {
        Some(DecoderSource::External(p)) => Some(p),
        Some(DecoderSource::Builtin(_)) => None,
        None => {
            let detail = match format {
                1 => "HEVC (a suitable decoder plugin is libde265)",
                2 => "AVC (a suitable decoder plugin is openh264)",
                3 => "JPEG (a suitable decoder plugin is libjpeg)",
                4 => "AV1 (a suitable decoder plugin is dav1d)",
                5 => "VVC (a suitable decoder plugin is vvdec)",
                6 => "EVC",
                7 => "JPEG 2000 (a suitable decoder plugin is openjpeg)",
                8 => "ISO/IEC 23001-17 uncompressed",
                9 => "mask image",
                10 => "HT-J2K (a suitable decoder plugin is openjpeg)",
                _ => "",
            };
            return Err(libheifer::context::ContextError::new(
                11,
                6003,
                format!(
                    "Error while loading plugin: Support for this compression format has not been built in{}{}",
                    if detail.is_empty() { "" } else { ": " },
                    detail
                ),
            ));
        }
    })
}
