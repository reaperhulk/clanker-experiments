// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::context::{self, HeifContext};
use std::ffi::{c_int, c_void};
#[cfg(unix)]
unsafe extern "C" {
    fn write(fd: c_int, data: *const c_void, size: usize) -> isize;
}
#[cfg(windows)]
unsafe extern "C" {
    fn _write(fd: c_int, data: *const c_void, size: u32) -> c_int;
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_context_debug_dump_boxes_to_file(ctx: *mut HeifContext, fd: c_int) {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return;
    };
    let dump = context::lock(&ctx.shared).debug_dump_boxes();
    // The public function borrows the descriptor and makes one write, including
    // on a short write/error. Ownership of the descriptor stays with the caller.
    #[cfg(unix)]
    unsafe {
        write(fd, dump.as_ptr().cast(), dump.len());
    }
    #[cfg(windows)]
    unsafe {
        _write(fd, dump.as_ptr().cast(), dump.len() as u32);
    }
}
