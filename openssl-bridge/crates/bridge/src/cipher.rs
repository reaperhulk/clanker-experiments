//! Stateful conventional ciphers and authenticated, one-shot AES-GCM.
use crate::{
    error::{check, pointer},
    ffi, Error, Result,
};
use std::ptr::{self, NonNull};

#[derive(Clone, Copy)]
pub enum Direction {
    Encrypt,
    Decrypt,
}

/// Supported conventional modes. AEAD modes have a separate API so a caller
/// cannot accidentally finalize authenticated decryption without a tag.
#[derive(Clone, Copy)]
pub enum Cipher {
    Aes128Cbc,
    Aes192Cbc,
    Aes256Cbc,
    Aes128Ctr,
    Aes192Ctr,
    Aes256Ctr,
    Aes128Ecb,
    Aes192Ecb,
    Aes256Ecb,
}

impl Cipher {
    fn descriptor(self) -> *const ffi::EVP_CIPHER {
        // SAFETY: Each function returns an immutable, process-lifetime descriptor.
        unsafe {
            match self {
                Self::Aes128Cbc => ffi::EVP_aes_128_cbc(),
                Self::Aes192Cbc => ffi::EVP_aes_192_cbc(),
                Self::Aes256Cbc => ffi::EVP_aes_256_cbc(),
                Self::Aes128Ctr => ffi::EVP_aes_128_ctr(),
                Self::Aes192Ctr => ffi::EVP_aes_192_ctr(),
                Self::Aes256Ctr => ffi::EVP_aes_256_ctr(),
                Self::Aes128Ecb => ffi::EVP_aes_128_ecb(),
                Self::Aes192Ecb => ffi::EVP_aes_192_ecb(),
                Self::Aes256Ecb => ffi::EVP_aes_256_ecb(),
            }
        }
    }
}

struct Context(NonNull<ffi::EVP_CIPHER_CTX>);
// SAFETY: Context is uniquely owned; all operations require exclusive access.
unsafe impl Send for Context {}
impl Context {
    fn new() -> Result<Self> {
        // SAFETY: The allocator has no preconditions.
        pointer(unsafe { ffi::EVP_CIPHER_CTX_new() }).map(Self)
    }
    fn ptr(&mut self) -> *mut ffi::EVP_CIPHER_CTX {
        self.0.as_ptr()
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: This is the sole owner; free accepts partially initialized state.
        unsafe { ffi::EVP_CIPHER_CTX_free(self.0.as_ptr()) };
    }
}

pub struct Stream {
    ctx: Context,
    block_size: usize,
    poisoned: bool,
}

impl Stream {
    pub fn new(
        cipher: Cipher,
        direction: Direction,
        key: &[u8],
        iv: &[u8],
        padding: bool,
    ) -> Result<Self> {
        let descriptor = cipher.descriptor();
        if descriptor.is_null() {
            return Err(Error::Unsupported("cipher is unavailable"));
        }
        // SAFETY: descriptor was returned by a native getter and is non-NULL.
        let (key_size, iv_size, block_size) = unsafe {
            (
                ffi::OB_cipher_key_size(descriptor),
                ffi::OB_cipher_iv_size(descriptor),
                ffi::OB_cipher_block_size(descriptor),
            )
        };
        if key.len() != key_size as usize {
            return Err(Error::InvalidInput("incorrect key length"));
        }
        if iv.len() != iv_size as usize {
            return Err(Error::InvalidInput("incorrect IV length"));
        }
        if block_size < 1 {
            return Err(Error::Unsupported("invalid cipher block size"));
        }
        let mut ctx = Context::new()?;
        let encrypt = matches!(direction, Direction::Encrypt) as i32;
        // SAFETY: ctx is owned; key/IV match the descriptor's required lengths.
        // ECB ignores the empty IV pointer; no engine is selected.
        check(unsafe {
            ffi::EVP_CipherInit_ex(
                ctx.ptr(),
                descriptor,
                ptr::null_mut(),
                key.as_ptr(),
                iv.as_ptr(),
                encrypt,
            )
        })?;
        // SAFETY: The context has a selected conventional cipher.
        check(unsafe { ffi::EVP_CIPHER_CTX_set_padding(ctx.ptr(), padding as i32) })?;
        Ok(Self {
            ctx,
            block_size: block_size as usize,
            poisoned: false,
        })
    }

    pub fn update_capacity(&self, input_length: usize) -> Result<usize> {
        // Padded decryption may copy the entire withheld block before deciding
        // how much output to report (including for a zero-length input). Bounds
        // must cover writes, not just the returned length, especially on LibreSSL.
        let slack = if self.block_size == 1 {
            0
        } else {
            self.block_size
        };
        input_length
            .checked_add(slack)
            .ok_or(Error::InvalidInput("output size overflow"))
    }

    pub fn update_into(&mut self, input: &[u8], output: &mut [u8]) -> Result<usize> {
        if self.poisoned {
            return Err(Error::InvalidState("cipher context is poisoned"));
        }
        let length: i32 = input
            .len()
            .try_into()
            .map_err(|_| Error::InvalidInput("cipher input exceeds INT_MAX"))?;
        if output.len() < self.update_capacity(input.len())? {
            return Err(Error::InvalidInput("output buffer is too small"));
        }
        // EVP may overflow its int output length if input is too close to INT_MAX.
        if self.update_capacity(input.len())? > i32::MAX as usize {
            return Err(Error::InvalidInput("cipher output exceeds INT_MAX"));
        }
        self.poisoned = true;
        let mut written = 0;
        // SAFETY: ctx is initialized and exclusive; input and output are disjoint
        // Rust borrows. Capacity covers a possibly buffered partial block.
        check(unsafe {
            ffi::EVP_CipherUpdate(
                self.ctx.ptr(),
                output.as_mut_ptr(),
                &mut written,
                input.as_ptr(),
                length,
            )
        })?;
        let written = usize::try_from(written)
            .map_err(|_| Error::InvalidState("negative cipher output length"))?;
        if written > output.len() {
            return Err(Error::InvalidState("unexpected cipher output length"));
        }
        self.poisoned = false;
        Ok(written)
    }

    pub fn finish(mut self) -> Result<Vec<u8>> {
        if self.poisoned {
            return Err(Error::InvalidState("cipher context is poisoned"));
        }
        let mut output = vec![0; self.block_size];
        let mut written = 0;
        // SAFETY: output fits a complete final block; context is consumed here.
        check(unsafe {
            ffi::EVP_CipherFinal_ex(self.ctx.ptr(), output.as_mut_ptr(), &mut written)
        })?;
        let written = usize::try_from(written)
            .map_err(|_| Error::InvalidState("negative final output length"))?;
        if written > output.len() {
            return Err(Error::InvalidState("unexpected final output length"));
        }
        output.truncate(written);
        Ok(output)
    }
}

/// AES-GCM with a 128-bit authentication tag. `open` never returns unauthenticated
/// plaintext, even if the native API writes it before detecting a bad tag.
pub struct AesGcm;
impl AesGcm {
    fn context(key: &[u8], nonce: &[u8], direction: Direction) -> Result<Context> {
        // SAFETY: These getters return process-lifetime immutable descriptors.
        let descriptor = unsafe {
            match key.len() {
                16 => ffi::EVP_aes_128_gcm(),
                24 => ffi::EVP_aes_192_gcm(),
                32 => ffi::EVP_aes_256_gcm(),
                _ => {
                    return Err(Error::InvalidInput(
                        "AES-GCM key must be 16, 24, or 32 bytes",
                    ))
                }
            }
        };
        if descriptor.is_null() {
            return Err(Error::Unsupported("AES-GCM unavailable"));
        }
        let nonce_len: i32 = nonce
            .len()
            .try_into()
            .map_err(|_| Error::InvalidInput("nonce is too long"))?;
        if nonce_len == 0 {
            return Err(Error::InvalidInput("nonce cannot be empty"));
        }
        let mut ctx = Context::new()?;
        let encrypt = matches!(direction, Direction::Encrypt) as i32;
        // SAFETY: This owned context selects a cipher before setting its IV length.
        check(unsafe {
            ffi::EVP_CipherInit_ex(
                ctx.ptr(),
                descriptor,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                encrypt,
            )
        })?;
        // SAFETY: This GCM control takes an integer length and no data pointer.
        check(unsafe {
            ffi::EVP_CIPHER_CTX_ctrl(
                ctx.ptr(),
                ffi::EVP_CTRL_GCM_SET_IVLEN as i32,
                nonce_len,
                ptr::null_mut(),
            )
        })?;
        // SAFETY: key matches the selected cipher and nonce matches the configured
        // IV length; both slices remain live for the duration of initialization.
        check(unsafe {
            ffi::EVP_CipherInit_ex(
                ctx.ptr(),
                ptr::null(),
                ptr::null_mut(),
                key.as_ptr(),
                nonce.as_ptr(),
                encrypt,
            )
        })?;
        Ok(ctx)
    }

    fn update(ctx: &mut Context, input: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        let input_len: i32 = input
            .len()
            .try_into()
            .map_err(|_| Error::InvalidInput("input exceeds INT_MAX"))?;
        let aad_len: i32 = aad
            .len()
            .try_into()
            .map_err(|_| Error::InvalidInput("AAD exceeds INT_MAX"))?;
        let mut written = 0;
        // SAFETY: A null output means AAD for an initialized GCM context;
        // aad is readable for aad_len. The context has not processed payload yet.
        check(unsafe {
            ffi::EVP_CipherUpdate(
                ctx.ptr(),
                ptr::null_mut(),
                &mut written,
                aad.as_ptr(),
                aad_len,
            )
        })?;
        let mut output = vec![0; input.len()];
        // SAFETY: GCM writes exactly input_len bytes and has no block buffering.
        let result = check(unsafe {
            ffi::EVP_CipherUpdate(
                ctx.ptr(),
                output.as_mut_ptr(),
                &mut written,
                input.as_ptr(),
                input_len,
            )
        });
        if let Err(error) = result {
            cleanse(&mut output);
            return Err(error);
        }
        if written != input_len {
            cleanse(&mut output);
            return Err(Error::InvalidState("unexpected GCM output length"));
        }
        Ok(output)
    }

    pub fn seal(
        key: &[u8],
        nonce: &[u8],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<(Vec<u8>, [u8; 16])> {
        let mut ctx = Self::context(key, nonce, Direction::Encrypt)?;
        let output = Self::update(&mut ctx, plaintext, aad)?;
        let mut final_buffer = [0; 16];
        let mut written = 0;
        // SAFETY: A full block is available for finalization, which writes zero
        // bytes in GCM. The initialized context has not been finalized before.
        check(unsafe {
            ffi::EVP_CipherFinal_ex(ctx.ptr(), final_buffer.as_mut_ptr(), &mut written)
        })?;
        if written != 0 {
            return Err(Error::InvalidState("unexpected GCM final output"));
        }
        let mut tag = [0; 16];
        // SAFETY: GET_TAG runs after encryption finalization; tag fits 16 bytes.
        check(unsafe {
            ffi::EVP_CIPHER_CTX_ctrl(
                ctx.ptr(),
                ffi::EVP_CTRL_GCM_GET_TAG as i32,
                16,
                tag.as_mut_ptr().cast(),
            )
        })?;
        Ok((output, tag))
    }

    pub fn open(
        key: &[u8],
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
        tag: &[u8; 16],
    ) -> Result<Vec<u8>> {
        let mut ctx = Self::context(key, nonce, Direction::Decrypt)?;
        let mut tag = *tag;
        // SAFETY: SET_TAG copies exactly 16 bytes from a live, writable local copy.
        check(unsafe {
            ffi::EVP_CIPHER_CTX_ctrl(
                ctx.ptr(),
                ffi::EVP_CTRL_GCM_SET_TAG as i32,
                16,
                tag.as_mut_ptr().cast(),
            )
        })?;
        let mut plaintext = Self::update(&mut ctx, ciphertext, aad)?;
        let mut final_buffer = [0; 16];
        let mut written = 0;
        // SAFETY: Context has payload and expected tag; output fits one block.
        let result = check(unsafe {
            ffi::EVP_CipherFinal_ex(ctx.ptr(), final_buffer.as_mut_ptr(), &mut written)
        });
        cleanse(&mut final_buffer);
        if let Err(error) = result {
            cleanse(&mut plaintext);
            return Err(error);
        }
        if written != 0 {
            cleanse(&mut plaintext);
            return Err(Error::InvalidState("unexpected GCM final output"));
        }
        Ok(plaintext)
    }
}

fn cleanse(bytes: &mut [u8]) {
    // SAFETY: bytes is exclusively writable for the declared length.
    unsafe { ffi::OPENSSL_cleanse(bytes.as_mut_ptr().cast(), bytes.len()) };
}
