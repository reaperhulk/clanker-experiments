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
