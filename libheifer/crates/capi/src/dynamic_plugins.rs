// SPDX-License-Identifier: LGPL-3.0-or-later
//! Operating-system module management. Codec implementations are supplied by the
//! caller; no native codec or libheif implementation is a dependency.
use crate::{
    HeifError, SUCCESS,
    plugin_registry::{self, field},
    plugin_types::*,
};
use libheifer::error::Error;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr,
    sync::{LazyLock, Mutex},
};
struct Loaded {
    handle: usize,
    info: usize,
    count: usize,
    matchable: bool,
}
// Native unload leaves decoder records registered. Retain their module storage
// until registry teardown so subsequent discovery/cleanup cannot read freed code.
static RETIRED_DECODERS: LazyLock<Mutex<Vec<usize>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static LOADED: LazyLock<Mutex<Vec<Loaded>>> = LazyLock::new(|| Mutex::new(Vec::new()));
#[cfg(unix)]
mod platform {
    use super::*;
    #[cfg_attr(not(target_os = "macos"), link(name = "dl"))]
    unsafe extern "C" {
        fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> c_int;
        fn dlerror() -> *const c_char;
    }
    pub unsafe fn open(name: *const c_char) -> *mut c_void {
        let handle = unsafe { dlopen(name, 1) };
        if handle.is_null() {
            let error = unsafe { dlerror() };
            if !error.is_null() {
                eprintln!(
                    "dlopen: {}",
                    unsafe { CStr::from_ptr(error) }.to_string_lossy()
                );
            }
        }
        handle
    }
    pub unsafe fn info(h: *mut c_void) -> *const PluginInfo {
        let info: *const PluginInfo = unsafe { dlsym(h, c"plugin_info".as_ptr()).cast() };
        if info.is_null() {
            let error = unsafe { dlerror() };
            if !error.is_null() {
                eprintln!(
                    "dlsym: {}",
                    unsafe { CStr::from_ptr(error) }.to_string_lossy()
                );
            }
        }
        info
    }
    pub unsafe fn close(h: *mut c_void) {
        unsafe { dlclose(h) };
    }
}
#[cfg(windows)]
mod platform {
    use super::*;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn FreeLibrary(handle: *mut c_void) -> c_int;
    }
    pub unsafe fn open(name: *const c_char) -> *mut c_void {
        unsafe { LoadLibraryA(name) }
    }
    pub unsafe fn info(h: *mut c_void) -> *const PluginInfo {
        unsafe { GetProcAddress(h, c"plugin_info".as_ptr()).cast() }
    }
    pub unsafe fn close(h: *mut c_void) {
        unsafe { FreeLibrary(h) };
    }
}
#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;
    pub unsafe fn open(_: *const c_char) -> *mut c_void {
        ptr::null_mut()
    }
    pub unsafe fn info(_: *mut c_void) -> *const PluginInfo {
        ptr::null()
    }
    pub unsafe fn close(_: *mut c_void) {}
}
fn load_error() -> HeifError {
    Error::new(11, 6000, c"Cannot open plugin (dlopen).").into()
}
fn version_error(text: &'static CStr) -> HeifError {
    Error::new(11, 2003, text).into()
}
pub(super) fn paths() -> Vec<CString> {
    let value = std::env::var_os("LIBHEIF_PLUGIN_PATH");
    #[cfg(unix)]
    let bytes = value.map(|v| {
        use std::os::unix::ffi::OsStringExt;
        v.into_vec()
    });
    #[cfg(not(unix))]
    let bytes = value.map(|v| v.to_string_lossy().as_bytes().to_vec());
    let sep = if cfg!(windows) { b';' } else { b':' };
    let mut paths = Vec::new();
    if let Some(bytes) = bytes {
        for part in bytes.split_terminator_byte(sep) {
            if let Ok(p) = CString::new(part) {
                paths.push(p)
            }
        }
    }
    if paths.is_empty() {
        paths.push(CString::default());
    }
    paths
}
// std slice::split includes the final empty segment, unlike C++ getline.
trait SplitTerminatorByte {
    fn split_terminator_byte(&self, sep: u8) -> Vec<&[u8]>;
}
impl SplitTerminatorByte for Vec<u8> {
    fn split_terminator_byte(&self, sep: u8) -> Vec<&[u8]> {
        if self.is_empty() {
            return Vec::new();
        }
        let mut v: Vec<_> = self.split(|b| *b == sep).collect();
        if self.last() == Some(&sep) {
            v.pop();
        }
        v
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn heif_get_plugin_directories() -> *mut *mut c_char {
    let mut result: Vec<_> = paths().into_iter().map(CString::into_raw).collect();
    result.push(ptr::null_mut());
    Box::into_raw(result.into_boxed_slice()).cast()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_free_plugin_directories(dirs: *mut *mut c_char) {
    if dirs.is_null() {
        return;
    }
    let mut count = 0;
    unsafe {
        while !(*dirs.add(count)).is_null() {
            drop(CString::from_raw(*dirs.add(count)));
            count += 1;
        }
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
            dirs,
            count + 1,
        )));
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_load_plugin(
    filename: *const c_char,
    out: *mut *const PluginInfo,
) -> HeifError {
    if filename.is_null() || out.is_null() {
        return Error::NULL.into();
    }
    let handle = unsafe { platform::open(filename) };
    if handle.is_null() {
        return load_error();
    }
    let info = unsafe { platform::info(handle) };
    if info.is_null() {
        unsafe { platform::close(handle) };
        return load_error();
    }
    {
        let mut loaded = LOADED.lock().unwrap();
        if let Some(p) = loaded
            .iter_mut()
            .find(|p| p.matchable && p.handle == handle as usize)
        {
            p.count += 1;
            unsafe { out.write(p.info as *const PluginInfo) };
            return SUCCESS;
        }
        loaded.push(Loaded {
            handle: handle as usize,
            info: info as usize,
            count: 1,
            matchable: true,
        });
    }
    unsafe { out.write(info) };
    match field!(info, kind) {
        0 => {
            let p = field!(info, plugin).cast::<EncoderPlugin>();
            if p.is_null() {
                return Error::NULL.into();
            }
            if field!(p, plugin_api_version) < 4 {
                return version_error(c"Encoder plugin needs to be at least version 4");
            }
            if field!(p, minimum_required_libheif_version) > 0x01170400 {
                return version_error(c"Encoder plugin requires at least libheif version 1.23.4");
            }
            unsafe { plugin_registry::heif_register_encoder_plugin(p) }
        }
        1 => {
            let p = field!(info, plugin).cast::<DecoderPlugin>();
            if p.is_null() {
                return Error::NULL.into();
            }
            if field!(p, plugin_api_version) < 5 {
                return version_error(c"Decoder plugin needs to be at least version 6");
            }
            if field!(p, minimum_required_libheif_version) > 0x01170400 {
                return version_error(c"Decoder plugin requires at least libheif version 1.23.4");
            }
            unsafe { plugin_registry::heif_register_decoder_plugin(p) }
        }
        _ => SUCCESS,
    }
}
unsafe fn unregister(info: *const PluginInfo) {
    if field!(info, kind) == 0 {
        let plugin = field!(info, plugin).cast();
        unsafe { plugin_registry::unregister_encoder(plugin) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_unload_plugin(info: *const PluginInfo) -> HeifError {
    let (handle, remove) = {
        let mut loaded = LOADED.lock().unwrap();
        let Some(index) = loaded.iter().position(|p| p.info == info as usize) else {
            return Error::new(11, 6001, c"Trying to remove a plugin that is not loaded.").into();
        };
        let p = &mut loaded[index];
        p.count -= 1;
        p.matchable = false;
        let result = (p.handle, p.count == 0);
        if result.1 {
            loaded.swap_remove(index);
        }
        result
    };
    // Unregister before closing: upstream accesses module data after dlclose, which
    // is undefined for an unpinned last reference. Preserve all defined callbacks.
    if remove {
        unsafe { unregister(info) }
    }
    if remove
        && field!(info, kind) == 1
        && plugin_registry::decoder_registered(field!(info, plugin).cast())
    {
        RETIRED_DECODERS.lock().unwrap().push(handle);
    } else {
        unsafe { platform::close(handle as *mut c_void) };
    }
    SUCCESS
}
pub(super) fn unload_all() {
    let loaded = std::mem::take(&mut *LOADED.lock().unwrap());
    for p in loaded {
        unsafe {
            unregister(p.info as *const PluginInfo);
            for _ in 0..p.count {
                platform::close(p.handle as *mut c_void);
            }
        }
    }
    for handle in std::mem::take(&mut *RETIRED_DECODERS.lock().unwrap()) {
        unsafe { platform::close(handle as *mut c_void) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heif_load_plugins(
    directory: *const c_char,
    out: *mut *const PluginInfo,
    count: *mut c_int,
    capacity: c_int,
) -> HeifError {
    let mut n = 0;
    let mut result = SUCCESS;
    if !directory.is_null() {
        let bytes = unsafe { CStr::from_ptr(directory) }.to_bytes();
        #[cfg(unix)]
        let path = {
            use std::os::unix::ffi::OsStrExt;
            std::path::PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
        };
        #[cfg(not(unix))]
        let path = std::path::PathBuf::from(String::from_utf8_lossy(bytes).as_ref());
        if !bytes.is_empty()
            && let Ok(entries) = std::fs::read_dir(path)
        {
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if !kind.is_file() && !kind.is_symlink() {
                    continue;
                }
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let suffix = if cfg!(windows) { ".dll" } else { ".so" };
                if name.len() <= suffix.len() || !name.ends_with(suffix) {
                    continue;
                }
                #[cfg(unix)]
                let filename = {
                    use std::os::unix::ffi::OsStrExt;
                    CString::new(entry.path().as_os_str().as_bytes()).unwrap()
                };
                #[cfg(not(unix))]
                let filename = CString::new(entry.path().to_string_lossy().as_bytes()).unwrap();
                let mut info = ptr::null();
                let error = unsafe { heif_load_plugin(filename.as_ptr(), &mut info) };
                if error.code == 0 {
                    if !out.is_null() {
                        if n == capacity {
                            break;
                        }
                        unsafe { out.offset(n as isize).write(info) }
                    }
                    n += 1;
                } else {
                    result = error
                }
            }
        }
    }
    if !out.is_null() && n < capacity {
        unsafe { out.offset(n as isize).write(ptr::null()) }
    }
    if !count.is_null() {
        unsafe { count.write(n) }
    }
    result
}
