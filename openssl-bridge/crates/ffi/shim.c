#include "wrapper.h"

/* Keep macro evaluation in the selected backend's C headers. */
int OB_md_size(const EVP_MD *md) { return EVP_MD_size(md); }
int OB_md_block_size(const EVP_MD *md) { return EVP_MD_block_size(md); }
int OB_md_is_xof(const EVP_MD *md) {
#ifdef EVP_MD_FLAG_XOF
    return (EVP_MD_flags(md) & EVP_MD_FLAG_XOF) != 0;
#else
    (void)md;
    return 0;
#endif
}
int OB_err_lib(unsigned long code) { return ERR_GET_LIB(code); }
int OB_err_reason(unsigned long code) { return ERR_GET_REASON(code); }
int OB_cipher_key_size(const EVP_CIPHER *cipher) { return EVP_CIPHER_key_length(cipher); }
int OB_cipher_iv_size(const EVP_CIPHER *cipher) { return EVP_CIPHER_iv_length(cipher); }
int OB_cipher_block_size(const EVP_CIPHER *cipher) { return EVP_CIPHER_block_size(cipher); }
