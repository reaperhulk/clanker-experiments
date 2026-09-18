//! Safe ownership and operation-specific APIs for OpenSSL and its forks.
//!
//! No foreign pointer or raw context is exposed by the safe API. Mutable
//! operations require exclusive access. Failed operations poison their context;
//! finalization consumes it. Fallible allocation and cloning return errors.
pub mod cipher;
pub mod curve25519;
pub mod error;
pub mod hash;
pub mod kdf;
pub mod mac;
pub mod rand;

/// Compare equal-length byte strings without data-dependent early exit.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // SAFETY: Both inputs are readable for a.len(), including the empty case.
    unsafe { ffi::CRYPTO_memcmp(a.as_ptr().cast(), b.as_ptr().cast(), a.len()) == 0 }
}

pub use error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    OpenSsl,
    LibreSsl,
    BoringSsl,
    AwsLc,
}

pub const BACKEND: Backend = {
    #[cfg(backend = "openssl")]
    {
        Backend::OpenSsl
    }
    #[cfg(backend = "libressl")]
    {
        Backend::LibreSsl
    }
    #[cfg(backend = "boringssl")]
    {
        Backend::BoringSsl
    }
    #[cfg(backend = "awslc")]
    {
        Backend::AwsLc
    }
};
use openssl_bridge_sys as ffi;
