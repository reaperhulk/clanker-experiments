# Remaining work

The primary replacement is not complete. In particular, an upstream test pass
with the incremental patch must not be described as a full openssl replacement.

## Implemented and integrated

- Independent bindgen bindings against each backend's actual headers.
- Checked hash/XOF and HMAC contexts, fallible copying, consuming finalization.
- Python hash/HMAC paths and their dependent HKDF, KBKDF, HPKE paths now use them.
- CMAC, PBKDF2, scrypt, private random generation, and constant-time comparison.
- The `cryptography-crypto` helper crate no longer depends on rust-openssl.
- Ed25519/X25519 and RSA operations use algorithm-specific key types. Their
  serialization boundary temporarily converts keys to the original PKey layer.
- RSA padding is operation-specific, recovery is restricted to PKCS1 v1.5, and
  checked decryption buffers are erased on failure. The structural validation
  mode preserves cryptography's explicit mathematical-validation opt-out.

## Cipher integration under validation

- Conventional CBC/CTR/ECB/CFB/CFB8/OFB/stream ciphers with fixed setup, checked
  variable key lengths, and guarded nonce reset. The registry no longer uses
  rust-openssl cipher descriptors.
- XTS consumes one complete data unit and rejects equal key halves on every fork.
- Streaming GCM separates encryption from explicitly unverified decryption;
  finalization always requires a tag. One-shot AES-GCM withholds plaintext.
- OpenSSL descriptors are fetched and owned; fork lookup omissions are handled
  using their documented algorithm getters.

## Remaining symmetric integration under validation

- AEAD protocols (GCM, CCM, OCB, SIV, GCM-SIV, ChaCha20-Poly1305) use immutable
  keys with per-operation native state. Failed authentication leaves caller
  buffers unchanged; temporary output is erased.
- Native Poly1305 state is held at a stable address and finalized once.
- Fernet uses fallible copies of pristine HMAC and cipher key schedules.
- Password-encrypted keys and the Rust key-wrapping loops use checked ciphers.
- ML-DSA message representative hashing uses checked SHAKE256 contexts.

## Primary replacement backlog

1. Unify Python error-stack records
   and preserve native reason text, library/reason codes, and exception behavior.
2. Finish validating the conventional cipher and streaming GCM integration across
   backends. Compare test identities and existing skips, including provider
   configurations; do not infer capability equivalence from a successful build.
3. Finish the symmetric integration test matrix, including configuration variants,
   FIPS behavior, legacy key-encryption formats, and the minimum Rust version.
4. Replace remaining asymmetric key operations: DSA, EC/ECDSA/ECDH,
   DH, Ed448/X448, ML-DSA, and ML-KEM. Constructors and operations must maintain
   native invariants and explicit algorithm/key-role distinctions.
5. Replace key component handling, PKCS8/SPKI/PEM serialization, PKCS12, PKCS7,
   provider/FIPS controls, and the few remaining X509 operations. Preserve the
   existing Rust ASN.1 validation rather than silently changing parser behavior.
6. Remove the original `openssl`, `openssl-sys`, and now-redundant
   `cryptography-openssl` usage. Check the dependency graph, not just imports.
7. Exercise every backend/version/configuration in the pinned upstream CI matrix,
   comparing against unmodified baselines, including Rust 1.83 and external vectors.
8. Review the Python buffer boundary as well as all native unsafe blocks. The
   upstream `CffiBuf`/`CffiMutBuf` currently assume non-overlapping, non-concurrently
   mutated memory without enforcing it. Valid Rust borrows are a prerequisite of
   the safe abstraction; that existing integration limitation needs explicit review.
9. Review all unsafe blocks and ownership/state contracts, regenerate the patch
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

## EC integration under validation

Named-curve key generation/import, ECDSA, deterministic ECDSA, ECDH, and point
encoding now use typed keys. All five native backend builds pass the wrapper
suite and Clippy. Cryptography integration is being checked and still uses a
component conversion only at its existing shared parser/serializer boundary.

## Ed448/X448 integration under validation

OpenSSL Ed448 and X448 use distinct signing, verification, agreement-private,
and agreement-public key types with exact array sizes and fresh native operation
contexts. RFC 8032/7748 vectors check signatures, shared secrets, and rejection
of low-order agreement peers. These algorithms remain absent on forks that do
not implement them, matching cryptography's existing capability guards.
