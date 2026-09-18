# Integration and acceptance

`cryptography.patch` applies to the exact cryptography commit in `sources.json`.
Place the patched cryptography checkout next to `clanker-experiments`, so the
Cargo path dependency resolves to this experiment. It currently migrates hashes,
HMAC/CMAC, PBKDF2/scrypt, random generation, constant-time comparison, and the
Ed25519/X25519/Ed448/X448/RSA/EC/DH/DSA key operations. ML-DSA and
ML-KEM also use the independent layer on backends that implement them. Conventional and authenticated ciphers,
Poly1305, Fernet, key wrapping, password-based encryption, and ML-DSA message
representative hashing now use the new layer as well. The pure Rust password-derivation helpers now
depend solely on the new abstraction. Key parsing and serialization now use algorithm-specific owned keys and borrowed
serialization views. Their PKCS#1, SEC1, SPKI, PKCS#8, and PEM code no longer
uses native PKey/BigNum adapters. The key-parsing crate depends on the new sys
crate only for backend build metadata and continues to forbid unsafe code.
The integration now removes the `openssl`, `openssl-sys`, and
`cryptography-openssl` Rust dependencies. Container decoding, Argon2, provider
loading, runtime information, and error records also use the new layer.
The Python crypto backend and buffer adapter now forbid unsafe Rust code.
Mutable inputs are copied through Python's buffer protocol into immutable bytes;
outputs use independent Rust storage and publish only the successfully produced
prefix. Readonly views are also copied because their underlying owner may be
mutable. This permits overlap without aliased Rust references. Python buffer
exporters remain responsible for their own protocol and synchronization rules;
a copy is not a promise of an atomic snapshot under external mutation.
**The primary replacement passes the full pinned acceptance matrix.**
See [the final report](../validation/acceptance/README.md) for per-row counts,
source identities, baseline comparisons, and the scope of validation.
The current patch also removes the CFFI/TLS layer. The historical primary patch
is preserved in `cryptography-primary.patch`. The subsequent migration also
passes; see [its acceptance report](../validation/tls/acceptance/README.md).

Apply `pyopenssl.patch` to the pyOpenSSL revision in `sources.json`. It replaces
CFFI with typed X.509 and TLS adapters provided by the patched cryptography
extension. Run `tools/validate_pyopenssl.py` after the full cryptography nox
check to test the same native extension, physically remove CFFI, and compare
the complete pyOpenSSL suite against the recorded upstream baseline.

Use cryptography's canonical `nox -e local` session. Supply both
`--wycheproof-root` and `--x509-limbo-root` at the revisions in `sources.json`.
Set `OPENSSL_DIR` and `OPENSSL_STATIC` for the selected native build. libclang
must be discoverable; a separate bindgen executable is no longer required.

OpenSSL FIPS properties must be configured before process startup using its
configuration file. The integration's private activation hook now verifies this
configuration instead of calling OpenSSL's non-thread-safe property setter.
The cryptography patch also updates the upstream FIPS CI build configuration.
When using `tools/validate.py --fips`, standalone wrapper vectors run in ordinary
mode on that same native build, since they exercise non-FIPS algorithms too.
The complete cryptography Python and Rust suite retains the startup FIPS
configuration. Both configurations are recorded in the report. Its Rust PEM
derivation test checks MD5 rejection under FIPS and SHA-256 success in both modes.
Argon2 uses one worker, preserving the specified lane count and derived key,
without changing the default context's global thread-pool limit.

Use a separate checkout, nox environment, and Cargo target directory per
backend. This prevents an extension built against one fork from being imported
as evidence for another. The integration needs a Python shared library for the
Rust tests; its real directory must be on the linker and runtime search paths.

The checked-in upstream CI workflow records the required backend/version matrix,
including configuration variants. Passing a few backend versions does not satisfy
the full acceptance requirement. Preserve existing capability skips and compare
test outcomes against an unmodified baseline of the same backend and configuration.

The primary acceptance gate verifies:

- `cargo tree` for cryptography must contain neither `openssl` nor `openssl-sys`.
- All used operations must go through the independent safe abstraction.
- The full cryptography suite, Rust tests, and formatting/lint/type checks must
  pass for every required backend/version/configuration.
- No new skips or xfails may conceal integration regressions.
- The cryptography patch must be regenerated from the tested source tree and
  apply cleanly to the pinned revision.

The CFFI/TLS replacement has its own pyOpenSSL patch and test evidence. Its
acceptance requires full cryptography regressions and all five core pyOpenSSL
backend runs, with no omitted tests or added skips masking regressions.

The final primary report establishes migration of the pinned Rust API surface.
The subsequent CFFI/TLS acceptance report establishes the pyOpenSSL migration and
full cryptography regressions. Both state their platform scope; neither claims
an independent security audit.
