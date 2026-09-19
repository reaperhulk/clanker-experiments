// SPDX-License-Identifier: LGPL-3.0-or-later
//! File and callback input adapters.
use crate::{
    HeifError, SUCCESS,
    context::{self, HeifContext},
};
use libheifer::{context::ContextError, error::Error};
use std::{
    ffi::{CStr, c_char, c_void},
    sync::Arc,
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_read_from_file(
    ctx: *mut HeifContext,
    filename: *const c_char,
    _options: *const c_void,
) -> HeifError {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    if filename.is_null() {
        return Error::NULL.into();
    }
    let bytes = unsafe { CStr::from_ptr(filename) }.to_bytes();
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    };
    #[cfg(not(unix))]
    let path = std::path::PathBuf::from(String::from_utf8_lossy(bytes).as_ref());
    let mut state = context::lock(&ctx.shared);
    // An opening failure replaces the file model but retains old image handles.
    let _ = state.read(Arc::new(Vec::<u8>::new()));
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            let code = error.raw_os_error().unwrap_or(0);
            let description = error.to_string();
            let suffix = format!(" (os error {code})");
            let description = description.strip_suffix(&suffix).unwrap_or(&description);
            return context::report(
                &mut state,
                ContextError::new(
                    1,
                    0,
                    format!(
                        "Input file does not exist: Unspecified: Error opening file: {description} ({code})\n"
                    ),
                ),
            );
        }
    };
    let input = match libheifer::input::FileInput::new(file) {
        Ok(input) => input,
        Err(error) => return context::report(&mut state, error),
    };
    match state.read(Arc::new(input)) {
        Ok(()) => SUCCESS,
        Err(error) => context::report(&mut state, error),
    }
}

#[repr(C)]
pub struct ReaderRangeResult {
    pub status: i32,
    pub range_end: u64,
    pub reader_error_code: i32,
    pub reader_error_msg: *const c_char,
}
#[repr(C)]
pub struct Reader {
    pub reader_api_version: i32,
    pub get_position: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    pub read: Option<unsafe extern "C" fn(*mut c_void, usize, *mut c_void) -> i32>,
    pub seek: Option<unsafe extern "C" fn(i64, *mut c_void) -> i32>,
    pub wait_for_file_size: Option<unsafe extern "C" fn(i64, *mut c_void) -> i32>,
    pub request_range: Option<unsafe extern "C" fn(u64, u64, *mut c_void) -> ReaderRangeResult>,
    pub preload_range_hint: Option<unsafe extern "C" fn(u64, u64, *mut c_void)>,
    pub release_file_range: Option<unsafe extern "C" fn(u64, u64, *mut c_void)>,
    pub release_error_msg: Option<unsafe extern "C" fn(*const c_char)>,
}
use crate::plugin_registry::field;
use libheifer::{
    context::Input,
    input::{RangeSource, Snapshot},
};
use std::{
    borrow::Cow,
    io::{self, Read, Seek, SeekFrom},
    ptr,
    sync::Mutex,
};
struct ReaderSource {
    table: *const Reader,
    userdata: *mut c_void,
    position: u64,
    last_error: Option<ContextError>,
}
// SAFETY: the caller keeps the table and userdata alive for every retained handle.
// Access to this source and all callbacks is serialized by CallbackInput's mutex.
unsafe impl Send for ReaderSource {}
impl Read for ReaderSource {
    fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        let callback =
            field!(self.table, read).ok_or_else(|| io::Error::other("missing read callback"))?;
        if unsafe { callback(data.as_mut_ptr().cast(), data.len(), self.userdata) } != 0 {
            return Err(io::Error::other("reader failed"));
        }
        self.position = self.position.saturating_add(data.len() as u64);
        Ok(data.len())
    }
}
impl Seek for ReaderSource {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::Current(by) => self.position.checked_add_signed(by),
            SeekFrom::End(by) => self.request_range(0, u64::MAX).checked_add_signed(by),
        }
        .filter(|at| *at <= i64::MAX as u64)
        .ok_or_else(|| io::Error::other("invalid seek"))?;
        let callback =
            field!(self.table, seek).ok_or_else(|| io::Error::other("missing seek callback"))?;
        if unsafe { callback(position as i64, self.userdata) } != 0 {
            return Err(io::Error::other("reader seek failed"));
        }
        self.position = position;
        Ok(position)
    }
}
impl RangeSource for ReaderSource {
    fn request_range(&mut self, start: u64, end: u64) -> u64 {
        if field!(self.table, reader_api_version) >= 2
            && let Some(callback) = field!(self.table, request_range)
        {
            let result = unsafe { callback(start, end, self.userdata) };
            let message = if result.reader_error_msg.is_null() {
                None
            } else {
                let message = unsafe { CStr::from_ptr(result.reader_error_msg) }
                    .to_string_lossy()
                    .into_owned();
                if let Some(release) = field!(self.table, release_error_msg) {
                    unsafe { release(result.reader_error_msg) };
                }
                Some(message)
            };
            return match result.status {
                0 => end,
                1 => 0,
                2 => {
                    self.last_error = Some(ContextError::invalid(
                        100,
                        "Unexpected end of file: Read beyond file size",
                    ));
                    result.range_end
                }
                3 => {
                    let suffix = message.map(|s| format!(" : {s}")).unwrap_or_default();
                    self.last_error = Some(ContextError::invalid(
                        0,
                        &format!(
                            "Unspecified: Input error ({}){suffix}",
                            result.reader_error_code
                        ),
                    ));
                    0
                }
                _ => {
                    self.last_error = Some(ContextError::invalid(
                        0,
                        "Unspecified: Invalid input reader return value",
                    ));
                    0
                }
            };
        }
        let Some(wait) = field!(self.table, wait_for_file_size) else {
            return 0;
        };
        let mut hi = end.min(i64::MAX as u64);
        if unsafe { wait(hi as i64, self.userdata) } == 0 {
            return hi;
        }
        let Some(position) = field!(self.table, get_position) else {
            return 0;
        };
        let mut lo = unsafe { position(self.userdata) }.max(0) as u64;
        if lo >= hi {
            return hi;
        }
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if unsafe { wait(mid as i64, self.userdata) } == 0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }
}
impl ReaderSource {
    fn release_range(&mut self, start: u64, end: u64) {
        if field!(self.table, reader_api_version) >= 2
            && let Some(callback) = field!(self.table, release_file_range)
        {
            unsafe { callback(start, end, self.userdata) };
        }
    }
}
struct CallbackInput {
    snapshot: Snapshot,
    source: Mutex<ReaderSource>,
}
impl Input for CallbackInput {
    fn bytes(&self) -> &[u8] {
        &self.snapshot.bytes
    }
    fn length(&self) -> u64 {
        self.snapshot.length
    }
    fn original_offset(&self, data: &[u8]) -> u64 {
        self.snapshot.original_offset(data)
    }
    fn read_idat(&self, offset: u64, size: u64) -> Result<Cow<'_, [u8]>, ContextError> {
        let eof = || ContextError::invalid(100, "Unexpected end of file");
        let end = offset
            .checked_add(size)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(eof)?;
        let mut source = self.source.lock().unwrap();
        let wait = field!(source.table, wait_for_file_size).ok_or_else(eof)?;
        let status = unsafe { wait(end as i64, source.userdata) };
        if status == 1 || status == 2 {
            return Err(eof());
        }
        let size = usize::try_from(size).map_err(|_| ContextError::from(Error::ALLOCATION))?;
        let mut data = Vec::new();
        data.try_reserve_exact(size)
            .map_err(|_| ContextError::from(Error::ALLOCATION))?;
        data.resize(size, 0);
        source
            .seek(SeekFrom::Start(offset))
            .and_then(|_| source.read_exact(&mut data))
            .map_err(|_| eof())?;
        Ok(Cow::Owned(data))
    }
    fn read_range(&self, offset: u64, size: u64) -> Result<Cow<'_, [u8]>, ContextError> {
        let eof = || ContextError::invalid(100, "Unexpected end of file");
        let end = offset.checked_add(size).ok_or_else(eof)?;
        let mut source = self.source.lock().unwrap();
        if source.request_range(offset, end) < end {
            return Err(source.last_error.clone().unwrap_or_else(eof));
        }
        let size = usize::try_from(size).map_err(|_| ContextError::from(Error::ALLOCATION))?;
        let mut data = Vec::new();
        data.try_reserve_exact(size)
            .map_err(|_| ContextError::from(Error::ALLOCATION))?;
        data.resize(size, 0);
        source
            .seek(SeekFrom::Start(offset))
            .and_then(|_| source.read_exact(&mut data))
            .map_err(|_| eof())?;
        source.release_range(offset, end);
        Ok(Cow::Owned(data))
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_read_from_reader(
    ctx: *mut HeifContext,
    reader: *const Reader,
    userdata: *mut c_void,
    _options: *const c_void,
) -> HeifError {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return Error::NULL.into();
    };
    if reader.is_null() {
        return Error::NULL.into();
    }
    let mut state = context::lock(&ctx.shared);
    let empty: Vec<u8> = Vec::new();
    let _ = state.read(Arc::new(empty));
    let mut source = ReaderSource {
        table: reader,
        userdata,
        position: 0,
        last_error: None,
    };
    let snapshot = match Snapshot::read_ranges(&mut source) {
        Ok(snapshot) => snapshot,
        Err(error) => return context::report(&mut state, error),
    };
    match state.read(Arc::new(CallbackInput {
        snapshot,
        source: Mutex::new(source),
    })) {
        Ok(()) => SUCCESS,
        Err(error) => context::report(&mut state, error),
    }
}
