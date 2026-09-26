// SPDX-License-Identifier: LGPL-3.0-or-later
use std::{
    borrow::Cow,
    ffi::{CStr, CString},
};

/// Numeric values and stable diagnostic text follow the pinned libheif contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub code: i32,
    pub subcode: i32,
    pub message: Cow<'static, CStr>,
}

impl Error {
    pub const fn new(code: i32, subcode: i32, message: &'static CStr) -> Self {
        Self {
            code,
            subcode,
            message: Cow::Borrowed(message),
        }
    }
    pub fn owned(code: i32, subcode: i32, message: String) -> Self {
        Self {
            code,
            subcode,
            message: Cow::Owned(CString::new(message).expect("diagnostic has no NUL")),
        }
    }
    pub const NULL: Self = Self::new(5, 2001, c"NULL argument passed");
    pub const ALLOCATION: Self = Self::new(6, 0, c"Out of memory");
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}:{})",
            self.message.to_string_lossy(),
            self.code,
            self.subcode
        )
    }
}

impl std::error::Error for Error {}
