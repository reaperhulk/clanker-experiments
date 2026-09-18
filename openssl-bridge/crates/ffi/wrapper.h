#include <openssl/opensslv.h>
#include <openssl/crypto.h>
#include <openssl/err.h>
#include <openssl/evp.h>
#include <openssl/bn.h>
#include <openssl/rsa.h>
#include <openssl/dsa.h>
#include <openssl/dh.h>
#include <openssl/ec.h>
#include <openssl/hmac.h>
#include <openssl/cmac.h>
#include <openssl/rand.h>
#include <openssl/objects.h>
#include <openssl/pem.h>
#include <openssl/pkcs12.h>
#include <openssl/pkcs7.h>
#include <openssl/x509.h>
#include <openssl/x509_vfy.h>
#include <openssl/ssl.h>
#if defined(OPENSSL_IS_AWSLC)
#define OB_BACKEND_CODE 3
#elif defined(OPENSSL_IS_BORINGSSL)
#define OB_BACKEND_CODE 2
#elif defined(LIBRESSL_VERSION_NUMBER)
#define OB_BACKEND_CODE 1
#else
#define OB_BACKEND_CODE 0
#endif
#if defined(OPENSSL_IS_AWSLC)
#include <openssl/experimental/kem_deterministic_api.h>
#elif defined(OPENSSL_IS_BORINGSSL)
#include <openssl/mldsa.h>
#include <openssl/bytestring.h>
void OB_CBS_init(CBS *cbs, const unsigned char *data, size_t length);
#endif
#if defined(LIBRESSL_VERSION_NUMBER)
#include <openssl/poly1305.h>
#endif
#if defined(OPENSSL_IS_BORINGSSL) || defined(OPENSSL_IS_AWSLC)
#include <openssl/aead.h>
#include <openssl/poly1305.h>
#else
#if !defined(LIBRESSL_VERSION_NUMBER)
#include <openssl/provider.h>
#include <openssl/kdf.h>
#include <openssl/params.h>
#include <openssl/core_names.h>
#endif
#endif

int OB_md_size(const EVP_MD *md);
int OB_md_block_size(const EVP_MD *md);
int OB_md_is_xof(const EVP_MD *md);
int OB_err_lib(unsigned long code);
int OB_err_reason(unsigned long code);
int OB_cipher_key_size(const EVP_CIPHER *cipher);
int OB_cipher_iv_size(const EVP_CIPHER *cipher);
int OB_cipher_block_size(const EVP_CIPHER *cipher);
int OB_signature_md(EVP_PKEY_CTX *ctx, const EVP_MD *md);
int OB_rsa_padding(EVP_PKEY_CTX *ctx, int padding);
int OB_rsa_mgf1_md(EVP_PKEY_CTX *ctx, const EVP_MD *md);
int OB_rsa_oaep_md(EVP_PKEY_CTX *ctx, const EVP_MD *md);
int OB_rsa_pss_saltlen(EVP_PKEY_CTX *ctx, int length);
int OB_rsa_oaep_label(EVP_PKEY_CTX *ctx, const unsigned char *label, int length);

int OB_signature_nonce(EVP_PKEY_CTX *ctx, unsigned int nonce_type);

size_t OB_x509_stack_len(const STACK_OF(X509) *stack);
X509 *OB_x509_stack_get(const STACK_OF(X509) *stack, size_t index);
void OB_x509_stack_free(STACK_OF(X509) *stack);
#if OB_BACKEND_CODE == 2 || OB_BACKEND_CODE == 3
int OB_private_key_pkcs8(const EVP_PKEY *key, unsigned char *output, size_t capacity, size_t *length);
#endif
#if OB_BACKEND_CODE == 0 || OB_BACKEND_CODE == 1
int OB_pkcs7_kind(const PKCS7 *p7);
const STACK_OF(X509) *OB_pkcs7_certificates(const PKCS7 *p7);
#endif
