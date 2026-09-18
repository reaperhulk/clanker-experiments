# Primary acceptance results

The complete independent Rust replacement passes the pinned cryptography suite
on all rows below. Every candidate row ran `nox -e local`, including formatting,
linting, type checks, the full Python suite with Wycheproof and X.509 Limbo, and
the Rust unit and documentation tests. No test selection filter was used.

Host: Linux x86_64, Python 3.12.14, Rust 1.90.0 unless the row specifies 1.83.0.
This covers the native backend/version/configuration matrix, not every upstream
operating system or Python version. Sources and build flags are pinned in
[sources.json](../../compatibility/sources.json) and [matrix.json](../../compatibility/matrix.json).

| Configuration | Python passed | Python skipped | Rust test passes |
| --- | ---: | ---: | ---: |
| system | 4227 | 486 | 108 |
| openssl4 | 4690 | 23 | 108 |
| libressl | 3972 | 741 | 108 |
| boringssl | 3806 | 907 | 108 |
| awslc | 4321 | 392 | 108 |
| libressl421 | 3972 | 741 | 108 |
| openssl3022 | 4232 | 481 | 108 |
| openssl347 | 4348 | 365 | 108 |
| openssl358 | 4690 | 23 | 108 |
| minimal | 4684 | 29 | 108 |
| no-legacy0 | 4664 | 49 | 108 |
| no-legacy1 | 4664 | 49 | 108 |
| main | 4690 | 23 | 108 |
| fips | 3376 | 1337 | 108 |
| openssl364 | 4690 | 23 | 108 |
| msrv-boringssl | 3806 | 907 | 108 |
| msrv-awslc | 4321 | 392 | 108 |

Each row is compared by Python test identity with an unmodified checkout of the
same cryptography revision and native configuration. No existing test disappeared
or acquired a new skip or xfail. Two integration regressions were added: overlapping
input/output buffers and the XTS single-data-unit contract. The added XTS test is
skipped on BoringSSL because that backend does not implement XTS. The existing
bad-tag test also checks that failed authenticated decryption leaves output unchanged.

All five standalone backend rows and the OpenSSL 4 / Rust 1.83 row pass the wrapper
unit tests, compile-fail documentation tests, and Clippy with warnings denied.
Full cryptography runs on BoringSSL and AWS-LC additionally validate Rust 1.83.

The unmodified startup-FIPS baseline passes its Python suite but its Rust PEM
derivation fixture fails because it assumes MD5 is available. The migration checks
MD5 rejection under FIPS and adds a SHA-256 known-answer vector that passes in both
modes. The candidate startup-FIPS row passes both complete suites. This baseline
failure is preserved in the archived log; it is not suppressed or counted as success.

`report.json` records source hashes, test identities, exact counts, dependency
removal, receipts where available, and SHA-256 hashes of the uncompressed archived
logs. Files ending in `.gz` contain the original logs and JUnit results. The bridge
source hash includes both crate trees and workspace manifests. Integration code
hashes include the complete portable patch except `CHANGELOG.rst`; the full patch
hash is recorded separately. Runs with an older full patch hash differ only by a later changelog
documentation addition. The recorded remote source commit has the same
Git tree as the locally tested source commit.

The Cargo dependency tree contains neither `openssl` nor `openssl-sys`. The new
sys crate retains Cargo's `links = "openssl"` identity; a fixture containing the
actual original `openssl-sys` crate is rejected by Cargo as an intentional native
link conflict. This prevents two owners of unprefixed OpenSSL symbols in one Cargo
dependency graph. The expected rejection log is included.

Previous directories under `validation/` preserve incremental development and
performance evidence. This directory is the final primary acceptance gate.
The safety invariants are documented in [SAFETY.md](../../SAFETY.md). These tests
are not an independent security audit. pyOpenSSL CFFI/TLS replacement is the next
stage and is not claimed by this report.
