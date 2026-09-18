# CFFI/TLS migration validation

**Final status: complete.** All final CI gates passed; see
[the acceptance report](acceptance/README.md). The notes below preserve the
development history and do not describe the final status.

The primary replacement and its CI are complete in PR #2: all 17 full
cryptography rows and eight standalone library jobs passed. This branch holds
the subsequent CFFI/TLS implementation. Its final acceptance is still pending.

Cryptography's CFFI crate, generated Python binding, and build/runtime CFFI
dependencies have been removed. Its private binding tests now use typed Rust
interfaces. PyOpenSSL uses owned X.509 objects and TLS contexts/connections through
PyO3. Both adapters forbid unsafe Rust. The full portable migrations are
`compatibility/cryptography.patch` and `compatibility/pyopenssl.patch`;
`cryptography-primary.patch` preserves the primary acceptance input.

The first CFFI-free system pyOpenSSL run passed 436 tests with four existing
skips; OpenSSL 4 passed 439 with one existing skip. Fork diagnostics found native
differences in certificate-selection lifetime, TLS 1.3 metadata, and OCSP
observers. Corrections now copy CA names during the forks' permitted certificate
selection callback, retain actual Hello randoms, and require an OCSP request
before invoking client observers. LibreSSL's unsupported TLS 1.3 CA-name hints
are rejected unless the caller explicitly limits the protocol to TLS 1.2.

The updated standalone TLS suite passes 13 tests on OpenSSL 4 and LibreSSL and
12 on BoringSSL and AWS-LC, which lack native DTLS cookie exchange. Full
pyOpenSSL reruns and the final 17-row cryptography regression matrix are pending.
No full secondary acceptance is claimed by these development results.

The first full CI matrix passed the Python/Rust cryptography suites on all 15
current-toolchain rows. The coverage gate correctly flagged the dynamically
collected import check for the deleted `_conditional` CFFI helper. Its explicit
replacement verifies that the helper, `Binding`, and `_rust._openssl` are absent;
the validation report records this single test replacement. Other missing tests
and new skips still fail the gate. The two MSRV rows need a lifetime-lint fix.
Standalone CI also found LibreSSL accepts incomplete certificate serialization;
the wrapper now rejects missing public keys/signatures consistently. Its X.509
tests pass all four native families. A safety-comment placement fix addresses
the Rust 1.83 standalone lint failure. Final CI must pass with these corrections.

The corrected run passed all eight standalone jobs and all 17 complete
cryptography checks. The pyOpenSSL stage found an error-translation mismatch
for incomplete certificates; its public API now keeps `crypto.Error`/`SSL.Error`
while the Rust boundary rejects the invalid state. Four path-encoding tests
retain their upstream parameter IDs after replacing CFFI NULL with Python None.
Custom-prefix native builds have no populated default CA bundle, so CI explicitly
supplies the host's CA file for the upstream external-server verification test.
Verification remains enabled; the test report records the CA file and hash.

The next run passed the system, OpenSSL 4, BoringSSL, and AWS-LC pyOpenSSL
checks. LibreSSL still failed the external-server trust test: its native
default lookup ignores SSL_CERT_FILE, and CMake's default OPENSSLDIR was
relative. CI now builds LibreSSL with an absolute default directory in its
isolated prefix and refreshes the host CA bundle there after cache restore.
The implementation and test expectations remain unchanged by this build fix.

CI applies both portable patches to pinned upstream checkouts. In addition to
the existing matrix, five core integration rows run the full pyOpenSSL suite,
format/lint/types, physical CFFI removal and import checks. They compare every
original test identity and skip against the archived upstream baselines.
LibreSSL's upstream process exited during native DTLS listen before JUnit
emission; its verbose log records the preceding results, and unexecuted tests
are explicitly marked `not_run`. These are never counted as upstream passes.
