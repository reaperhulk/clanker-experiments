#include "wrapper.h"
#include <string.h>

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
int OB_cipher_iv_size(const EVP_CIPHER *cipher) {
#if OB_BACKEND_CODE == 3
    /* AWS-LC's legacy Blowfish ECB descriptor reports an eight-byte IV,
     * although ECB does not use one. Pass NULL to native initialization. */
    if (cipher == EVP_bf_ecb()) return 0;
#endif
    return EVP_CIPHER_iv_length(cipher);
}
int OB_cipher_block_size(const EVP_CIPHER *cipher) { return EVP_CIPHER_block_size(cipher); }
int OB_signature_md(EVP_PKEY_CTX *ctx, const EVP_MD *md) { return EVP_PKEY_CTX_set_signature_md(ctx, md); }
int OB_rsa_padding(EVP_PKEY_CTX *ctx, int padding) { return EVP_PKEY_CTX_set_rsa_padding(ctx, padding); }
int OB_rsa_mgf1_md(EVP_PKEY_CTX *ctx, const EVP_MD *md) { return EVP_PKEY_CTX_set_rsa_mgf1_md(ctx, md); }
int OB_rsa_oaep_md(EVP_PKEY_CTX *ctx, const EVP_MD *md) { return EVP_PKEY_CTX_set_rsa_oaep_md(ctx, md); }
int OB_rsa_pss_saltlen(EVP_PKEY_CTX *ctx, int length) { return EVP_PKEY_CTX_set_rsa_pss_saltlen(ctx, length); }
int OB_rsa_oaep_label(EVP_PKEY_CTX *ctx, const unsigned char *label, int length) {
    unsigned char *copy;
    int result;
    if (length < 0) return 0;
    /* LibreSSL does not retain a non-NULL allocation when length is zero. */
    if (length == 0) return EVP_PKEY_CTX_set0_rsa_oaep_label(ctx, NULL, 0);
    /* The successful set0 call owns this exact OpenSSL allocation;
     * failure retains ownership. */
    copy = OPENSSL_malloc((size_t)length);
    if (copy == NULL) return 0;
    if (length != 0) memcpy(copy, label, (size_t)length);
    result = EVP_PKEY_CTX_set0_rsa_oaep_label(ctx, copy, length);
    if (result <= 0) OPENSSL_free(copy);
    return result;
}

int OB_signature_nonce(EVP_PKEY_CTX *ctx, unsigned int nonce_type) {
#if OB_BACKEND_CODE == 0 && OPENSSL_VERSION_NUMBER >= 0x30200000L
    OSSL_PARAM params[2];
    params[0] = OSSL_PARAM_construct_uint("nonce-type", &nonce_type);
    params[1] = OSSL_PARAM_construct_end();
    return EVP_PKEY_CTX_set_params(ctx, params);
#else
    (void)ctx;
    (void)nonce_type;
    return 0;
#endif
}
