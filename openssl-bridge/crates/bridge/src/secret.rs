//! Owned secret buffers without implicit copying or diagnostic formatting.
use crate::ffi;

pub struct SecretBytes(Vec<u8>);
impl From<Vec<u8>> for SecretBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}
impl AsRef<[u8]> for SecretBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl Drop for SecretBytes {
    fn drop(&mut self) {
        // SAFETY: This uniquely owns the initialized bytes of the allocation.
        unsafe { ffi::OPENSSL_cleanse(self.0.as_mut_ptr().cast(), self.0.len()) };
    }
}
