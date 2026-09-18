# CFFI/TLS acceptance

The subsequent CFFI/TLS migration passed after the primary Rust replacement.
[The complete CI run](https://github.com/reaperhulk/clanker-experiments/actions/runs/35374605612) passed all eight standalone crate
jobs, all 17 full cryptography jobs, and five complete pyOpenSSL suites.
Each cryptography job ran canonical `nox -e local` without test filters,
including formatting, lint/type checks, Python and Rust tests, Wycheproof,
and X.509 Limbo. PyOpenSSL also passed Ruff formatting/lints and mypy.

Validated commit: `9cf3ada840fe06c8370ef3525934cb7199a04754`.
Native abstraction source SHA-256: `7e5b06299bf82b3916cbf320f5f8a25766f9c958c1e11515900945d2f6b2b380`.

| Portable patch | SHA-256 |
| --- | --- |
| `cryptography.patch` | `d0a931829ce6f80f44e405ea3550be3063883a87e8067e4aa4a783a38ae6a510` |
| `pyopenssl.patch` | `2d7a6e9f9c9af8bc8457c525f1dd7c67826c668ac586c6e0744c1575765e4c10` |

The patches apply to the pinned upstream commits in
`compatibility/sources.json`. They include the API changes, changelogs,
and tests. The archived applied-diff hashes can differ from portable
file hashes because Git regenerates index metadata after application.

## pyOpenSSL

| Native backend | Passed | Existing skips |
| --- | ---: | ---: |
| awslc | 431 | 10 |
| boringssl | 437 | 4 |
| libressl | 438 | 3 |
| openssl4 | 441 | 0 |
| system | 438 | 3 |

Every original test identity is present; there are no new skips or failures.
Six additional tests exercise the safe adapter and explicit unsupported
LibreSSL CA-hint configuration. Private CFFI mock tests now exercise typed
interfaces. Unsupported native features have explicit rejection checks.
CFFI and pycparser were physically uninstalled before these suites. Runtime
checks verify that neither CFFI module can be imported, `_rust._openssl` is
absent, and installed cryptography metadata has no CFFI dependency. Cargo
contains none of openssl, openssl-sys, cryptography-openssl, or cryptography-cffi.

CI supplies the host CA bundle to native builds outside platform prefixes.
External-server certificate verification remains enabled; each report records
the CA file and its hash. The fallback test clears these environment overrides
within its fixture to exercise fallback discovery independently.

LibreSSL ignores SSL_CERT_FILE in its native default lookup. Its CI build
uses an absolute OPENSSLDIR in the isolated native prefix; the action copies
the host CA bundle there after restoring the native cache.

The original LibreSSL process exited during native DTLS listen before writing
JUnit. Its archived verbose baseline records earlier results and marks the
remaining tests `not_run`; those were never counted as upstream passes.

## cryptography regression matrix

| Configuration | Python passed | Existing/capability skips | Rust passed |
| --- | ---: | ---: | ---: |
| awslc | 4321 | 392 | 108 |
| boringssl | 3806 | 907 | 108 |
| fips | 3376 | 1337 | 108 |
| libressl | 3972 | 741 | 108 |
| libressl421 | 3972 | 741 | 108 |
| main | 4690 | 23 | 108 |
| minimal | 4684 | 29 | 108 |
| msrv-awslc | 4321 | 392 | 108 |
| msrv-boringssl | 3806 | 907 | 108 |
| no-legacy0 | 4664 | 49 | 108 |
| no-legacy1 | 4664 | 49 | 108 |
| openssl3022 | 4232 | 481 | 108 |
| openssl347 | 4348 | 365 | 108 |
| openssl358 | 4690 | 23 | 108 |
| openssl364 | 4690 | 23 | 108 |
| openssl4 | 4690 | 23 | 108 |
| system | 4227 | 486 | 108 |

No existing test acquired a new skip. One dynamically collected import test
for the deleted `_conditional` CFFI helper was replaced with an explicit
absence test for that module, `Binding`, and `_rust._openssl`. Every row
records this mapping in `replaced_tests`; the gate requires the replacement
to pass and the old source file to be absent. Other missing tests still fail.
The primary overlap and XTS regressions remain, with the existing BoringSSL
capability skip for its unsupported XTS mode.

## Standalone crates

| Configuration | Rust tests and doctests passed |
| --- | ---: |
| awslc | 59 |
| boringssl | 58 |
| libressl | 57 |
| msrv-awslc | 59 |
| msrv-boringssl | 58 |
| msrv-openssl4 | 67 |
| openssl4 | 67 |
| system | 60 |

All eight rows passed formatting and Clippy with warnings denied. Rust 1.83
is covered for OpenSSL 4, BoringSSL, and AWS-LC. The MSRV integration rows
also run the full cryptography suite on BoringSSL and AWS-LC.

## Scope and intentional API changes

Validation covers Linux x86_64, Python 3.12, Rust 1.90 and the stated 1.83
rows. It does not establish the entire upstream OS/Python matrix or an
independent security audit. Stream socket ownership currently uses Unix
descriptor duplication; Windows sockets and direct DTLS socket transport
are unsupported. Memory TLS and packet-queue DTLS have no OS-handle dependency.

Contexts and trust-store configuration freeze at the first connection.
Connections reject concurrent/reentrant operations and retain write buffers
and sendall progress across WANT retries. Callback exceptions propagate to
the initiating operation. Native callbacks receive copied metadata and typed
actions; no foreign pointer is exposed through the safe API or PyO3 adapters.
Session installation checks configuration and identity. Fatal I/O errors
poison the connection, while already produced alerts remain drainable.

LibreSSL key logging is explicitly unsupported. Its TLS 1.3 CA-name hints
are rejected unless configuration explicitly caps the connection at TLS 1.2.
BoringSSL/AWS-LC lack native DTLS cookie exchange. Incomplete certificates
cannot be serialized as PEM/DER; raw key bits are preserved without requiring
the abstraction to recognize their algorithm. RNG compatibility accepts
additional input with zero caller-supplied entropy credit.

See [SAFETY.md](../../../SAFETY.md) and both patch changelogs for the contracts.
[report.json](report.json) indexes compressed logs, JUnit, per-row reports,
runtime extension identities, hashes, and the successful GitHub job IDs.
