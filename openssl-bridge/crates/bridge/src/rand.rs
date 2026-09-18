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
