# Integration and acceptance

`cryptography.patch` applies to the exact cryptography commit in `sources.json`.
Place the patched cryptography checkout next to `clanker-experiments`, so the
Cargo path dependency resolves to this experiment. It currently migrates hashes,
HMAC/CMAC, PBKDF2/scrypt, random generation, constant-time comparison, and the
Ed25519/X25519/RSA key operations. Conventional and authenticated ciphers,
Poly1305, Fernet, key wrapping, password-based encryption, and ML-DSA message
representative hashing now use the new layer as well. The pure Rust password-derivation helpers now
depend solely on the new abstraction. Ed25519/X25519/RSA serialization still uses a
temporary adapter to the old key parser and serializer. **It does not yet remove
the original openssl or openssl-sys dependencies.**

Use cryptography's canonical `nox -e local` session. Supply both
`--wycheproof-root` and `--x509-limbo-root` pointing at the revisions recorded in
`sources.json`. Set `OPENSSL_DIR` and `OPENSSL_STATIC` consistently for both
binding layers. BoringSSL and AWS-LC also require `bindgen` on PATH while the
original openssl-sys dependency remains. libclang must be discoverable.

Use a separate checkout, nox environment, and Cargo target directory per
backend. This prevents an extension built against one fork from being imported
as evidence for another. The integration needs a Python shared library for the
Rust tests; its real directory must be on the linker and runtime search paths.

The checked-in upstream CI workflow records the required backend/version matrix,
including configuration variants. Passing a few backend versions does not satisfy
the full acceptance requirement. Preserve existing capability skips and compare
test outcomes against an unmodified baseline of the same backend and configuration.

Before a PR may be opened:

- `cargo tree` for cryptography must contain neither `openssl` nor `openssl-sys`.
- All used operations must go through the independent safe abstraction.
- The full cryptography suite, Rust tests, and formatting/lint/type checks must
  pass for every required backend/version/configuration.
- No new skips or xfails may conceal integration regressions.
- The cryptography patch must be regenerated from the tested source tree and
  apply cleanly to the pinned revision.
- The later CFFI/TLS replacement must be validated with pyOpenSSL; record that
  patch and its test evidence separately.

Current code and test results are incremental development evidence, not proof
that the replacement is complete or that the public API has been fully audited.
