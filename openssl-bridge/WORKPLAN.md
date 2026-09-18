# Current status and remaining work

The independent Rust implementation covers the cryptography operations in the
pinned API inventory. Its integration patch removes openssl, openssl-sys, and the
old cryptography-openssl adapter. Primary acceptance remains incomplete until the
final source passes the full backend/version/configuration matrix.

## Implemented

- Bindgen against each backend's actual headers, with a small C compatibility shim.
- Hashes/XOF, MACs, KDFs including Argon2, random generation, and constant-time comparison.
- Conventional and authenticated ciphers, explicit unverified streaming GCM, and Poly1305.
- Typed RSA, EC, DH, DSA, Ed/X25519, Ed/X448, ML-DSA, and ML-KEM keys and operations.
- Typed pure Rust key codecs, PKCS#12 and BER PKCS#7 compatibility decoding, and PKCS#7 verification.
- Owned diagnostic records, provider lifetime management, and startup FIPS configuration.
- Python buffer snapshots and staged outputs, with unsafe code forbidden at that boundary.

## Required before the primary PR

1. Complete the final safety review and minimum-Rust checks.
2. Pass canonical cryptography `nox -e local` on every pinned native backend,
   version, and configuration, with both external vector suites.
3. Compare against unmodified baselines and account for test additions without
   allowing new skips or xfails to conceal regressions.
4. Archive logs and exact source/backend revisions. Verify neither old Rust
   dependency remains in the graph.
5. Regenerate the portable cryptography patch from the tested source, verify it
   applies cleanly, and prepare the primary PR with that patch and evidence.

## Subsequent stage

After the primary task is complete, replace the CFFI/TLS surface used by
pyOpenSSL. Validate the pinned pyOpenSSL suite against each supported backend
and record its integration patch and test evidence separately.

The reports already under validation are incremental evidence. They do not
assert completion of the final matrix or approval to open a PR.
