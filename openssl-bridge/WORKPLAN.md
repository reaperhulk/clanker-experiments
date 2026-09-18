# Remaining work

The primary replacement is not complete. In particular, an upstream test pass
with the incremental patch must not be described as a full openssl replacement.

## Implemented and integrated

- Independent bindgen bindings against each backend's actual headers.
- Checked hash/XOF and HMAC contexts, fallible copying, consuming finalization.
- Python hash/HMAC paths and their dependent HKDF, KBKDF, HPKE paths now use them.
- CMAC, PBKDF2, scrypt, private random generation, and constant-time comparison.
- The `cryptography-crypto` helper crate no longer depends on rust-openssl.
- Ed25519/X25519 operations use algorithm-specific key types. Their serialization
  boundary temporarily converts raw keys to the original PKey layer.

## Implemented, not yet integrated

- AES CBC/CTR/ECB contexts; one-shot AES-GCM with authenticated plaintext release.

## Primary replacement backlog

1. Unify Python error-stack records
   and preserve native reason text, library/reason codes, and exception behavior.
2. Implement the remaining conventional modes and legacy algorithms used in
   `cipher_registry.rs`, preserving backend capability discovery. Support variable
   key lengths, nonce reset, and streaming GCM through explicit operation APIs.
3. Implement CCM, OCB, SIV, GCM-SIV, ChaCha20-Poly1305 and backend-specific AEAD,
   with correct order of configuration, tag handling, buffer bounds, and context
   copying. Replace the raw `CipherCtx` usage in cryptography completely.
4. Replace asymmetric key operations: RSA (OAEP, PSS, PKCS1), DSA, EC/ECDSA/ECDH,
   DH, Ed448/X448, ML-DSA, and ML-KEM. Constructors and operations must maintain
   native invariants and explicit algorithm/key-role distinctions.
5. Replace key component handling, PKCS8/SPKI/PEM serialization, PKCS12, PKCS7,
   provider/FIPS controls, and the few remaining X509 operations. Preserve the
   existing Rust ASN.1 validation rather than silently changing parser behavior.
6. Remove the original `openssl`, `openssl-sys`, and now-redundant
   `cryptography-openssl` usage. Check the dependency graph, not just imports.
7. Exercise every backend/version/configuration in the pinned upstream CI matrix,
   comparing against unmodified baselines, including Rust 1.83 and external vectors.
8. Review all unsafe blocks and ownership/state contracts, regenerate the patch
   from the tested checkout, and only then submit the primary PR.

## Subsequent stage

Replace the CFFI/TLS surface used by pyOpenSSL after the primary task completes.
Use the pinned pyOpenSSL source, run its complete suite against each supported
backend, and include its integration patch and test evidence in that later work.

## Reproduction notes

The pinned source revisions, original CI matrix, API inventory, and current
integration patch are in `compatibility/`. Native libraries are development
inputs, not vendored runtime dependencies. Test reports must name both the
upstream revision and the exact abstraction source they exercised.
