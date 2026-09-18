use crate::{
    error::{check, pointer},
    ffi,
    hash::Algorithm,
    Error, Result,
};
use std::ptr::{self, NonNull};

pub struct Hmac {
    ctx: NonNull<ffi::HMAC_CTX>,
    size: usize,
    poisoned: bool,
}

// SAFETY: Each context is owned exclusively and mutated through &mut self only.
unsafe impl Send for Hmac {}
// SAFETY: Shared methods only copy the native state; they do not modify it.
unsafe impl Sync for Hmac {}

impl Hmac {
    pub fn new(algorithm: Algorithm, key: &[u8]) -> Result<Self> {
        if algorithm.is_xof() {
            return Err(Error::InvalidInput("HMAC requires a fixed-output digest"));
        }
        let size = algorithm.output_size()?;
        #[allow(clippy::useless_conversion)]
        let key_len = key
            .len()
            .try_into()
            .map_err(|_| Error::InvalidInput("HMAC key is too long"))?;
        // SAFETY: The allocator has no preconditions.
        let ctx = pointer(unsafe { ffi::HMAC_CTX_new() })?;
        let result = Self {
            ctx,
            size,
            poisoned: false,
        };
        // SAFETY: ctx is owned, key covers key_len, md is a live descriptor.
        // Even an empty key supplies a non-NULL pointer so it does not mean reuse.
        check(unsafe {
            ffi::HMAC_Init_ex(
                ctx.as_ptr(),
                key.as_ptr().cast(),
                key_len,
                algorithm.as_ptr(),
                ptr::null_mut(),
            )
        })?;
        Ok(result)
    }

    fn ready(&self) -> Result<()> {
        if self.poisoned {
            Err(Error::InvalidState("HMAC context is poisoned"))
        } else {
            Ok(())
        }
    }

    pub fn update(&mut self, data: &[u8]) -> Result<()> {
        self.ready()?;
        self.poisoned = true;
        // SAFETY: The initialized context is exclusive; data covers its length.
        check(unsafe { ffi::HMAC_Update(self.ctx.as_ptr(), data.as_ptr(), data.len()) })?;
        self.poisoned = false;
        Ok(())
    }

    pub fn try_clone(&self) -> Result<Self> {
        self.ready()?;
        // SAFETY: The allocator has no preconditions.
        let ctx = pointer(unsafe { ffi::HMAC_CTX_new() })?;
        let result = Self {
            ctx,
            size: self.size,
            poisoned: false,
        };
        // SAFETY: The source is initialized and the destination is uniquely owned.
        check(unsafe { ffi::HMAC_CTX_copy(ctx.as_ptr(), self.ctx.as_ptr()) })?;
        Ok(result)
    }

    pub fn finish(self) -> Result<Vec<u8>> {
        self.ready()?;
        let mut output = vec![0; self.size];
        let mut written = 0;
        // SAFETY: The output fits the selected digest; this consumes the context.
        check(unsafe { ffi::HMAC_Final(self.ctx.as_ptr(), output.as_mut_ptr(), &mut written) })?;
        if written as usize != output.len() {
            return Err(Error::InvalidState(
                "backend returned an unexpected MAC length",
            ));
        }
        Ok(output)
    }
}

impl Drop for Hmac {
    fn drop(&mut self) {
        // SAFETY: This is the unique owner; free accepts partially initialized state.
        unsafe { ffi::HMAC_CTX_free(self.ctx.as_ptr()) };
    }
}

#[derive(Clone, Copy)]
pub enum CmacCipher {
    Aes128,
    Aes192,
    Aes256,
    TripleDes,
}

pub struct Cmac {
    ctx: NonNull<ffi::CMAC_CTX>,
    size: usize,
    poisoned: bool,
}
// SAFETY: The context is uniquely owned; mutation requires exclusive access.
unsafe impl Send for Cmac {}
// SAFETY: Shared operations only copy state and never mutate the source.
unsafe impl Sync for Cmac {}

impl Cmac {
    pub fn new(cipher: CmacCipher, key: &[u8]) -> Result<Self> {
        // SAFETY: These getters return immutable, process-lifetime descriptors.
        let descriptor = unsafe {
            match cipher {
                CmacCipher::Aes128 => ffi::EVP_aes_128_cbc(),
                CmacCipher::Aes192 => ffi::EVP_aes_192_cbc(),
                CmacCipher::Aes256 => ffi::EVP_aes_256_cbc(),
                CmacCipher::TripleDes => ffi::EVP_des_ede3_cbc(),
            }
        };
        if descriptor.is_null() {
            return Err(Error::Unsupported("CMAC cipher is unavailable"));
        }
        // SAFETY: The descriptor is valid and non-NULL.
        let (key_size, block_size) = unsafe {
            (
                ffi::OB_cipher_key_size(descriptor),
                ffi::OB_cipher_block_size(descriptor),
            )
        };
        if key.len() != key_size as usize {
            return Err(Error::InvalidInput("incorrect CMAC key length"));
        }
        let size = usize::try_from(block_size)
            .ok()
            .filter(|v| *v > 0)
            .ok_or(Error::Unsupported("CMAC cipher has no block size"))?;
        // SAFETY: The allocator has no preconditions.
        let ctx = pointer(unsafe { ffi::CMAC_CTX_new() })?;
        let result = Self {
            ctx,
            size,
            poisoned: false,
        };
        // SAFETY: ctx is owned, key has the selected cipher's required length.
        check(unsafe {
            ffi::CMAC_Init(
                ctx.as_ptr(),
                key.as_ptr().cast(),
                key.len(),
                descriptor,
                ptr::null_mut(),
            )
        })?;
        Ok(result)
    }

    fn ready(&self) -> Result<()> {
        if self.poisoned {
            Err(Error::InvalidState("CMAC context is poisoned"))
        } else {
            Ok(())
        }
    }

    pub fn update(&mut self, data: &[u8]) -> Result<()> {
        self.ready()?;
        self.poisoned = true;
        // SAFETY: The initialized context is exclusive; data covers its length.
        check(unsafe { ffi::CMAC_Update(self.ctx.as_ptr(), data.as_ptr().cast(), data.len()) })?;
        self.poisoned = false;
        Ok(())
    }

    pub fn try_clone(&self) -> Result<Self> {
        self.ready()?;
        // SAFETY: The allocator has no preconditions.
        let ctx = pointer(unsafe { ffi::CMAC_CTX_new() })?;
        let result = Self {
            ctx,
            size: self.size,
            poisoned: false,
        };
        // SAFETY: The initialized source is live and destination is uniquely owned.
        check(unsafe { ffi::CMAC_CTX_copy(ctx.as_ptr(), self.ctx.as_ptr()) })?;
        Ok(result)
    }

    pub fn finish(self) -> Result<Vec<u8>> {
        self.ready()?;
        let mut output = vec![0; self.size];
        let mut written = 0;
        // SAFETY: The output fits the selected cipher's block size. Context is
        // initialized, has not been finalized, and is consumed by this method.
        check(unsafe { ffi::CMAC_Final(self.ctx.as_ptr(), output.as_mut_ptr(), &mut written) })?;
        if written != output.len() {
            return Err(Error::InvalidState("unexpected CMAC output length"));
        }
        Ok(output)
    }
}
impl Drop for Cmac {
    fn drop(&mut self) {
        // SAFETY: This is the sole owner; free accepts partially initialized state.
        unsafe { ffi::CMAC_CTX_free(self.ctx.as_ptr()) };
    }
}
