use crate::{error::check, ffi, Result};

pub fn fill(output: &mut [u8]) -> Result<()> {
    // The common API uses int on OpenSSL and size_t on some forks. Chunking
    // avoids truncation and works for both, including unusually large slices.
    for chunk in output.chunks_mut(i32::MAX as usize) {
        // SAFETY: Each chunk is writable for a length representable by either ABI.
        check(unsafe { ffi::RAND_bytes(chunk.as_mut_ptr(), chunk.len() as _) })?;
    }
    Ok(())
}

/// Fill secret key material using OpenSSL's separate private DRBG where it is
/// available. Forks use their cryptographic RAND_bytes implementation.
pub fn fill_private(output: &mut [u8]) -> Result<()> {
    #[cfg(backend = "openssl")]
    {
        for chunk in output.chunks_mut(i32::MAX as usize) {
            // SAFETY: Each exclusive slice covers the checked native length.
            check(unsafe { ffi::RAND_priv_bytes(chunk.as_mut_ptr(), chunk.len() as i32) })?;
        }
        Ok(())
    }
    #[cfg(not(backend = "openssl"))]
    fill(output)
}
