# Current status and remaining work

The primary Rust replacement is complete for the pinned cryptography revision.
The integration removes openssl, openssl-sys, and cryptography-openssl. All 17
backend/version/configuration acceptance rows pass the canonical cryptography
session, including both external vector suites and the Rust tests. Standalone
wrapper tests and Clippy pass across the five core installations and on Rust 1.83.

The final evidence and its exact platform scope are in
[validation/acceptance](validation/acceptance/README.md). Earlier reports preserve
incremental development and performance evidence; the final report is the primary
acceptance gate. No existing Python test was removed or given a new skip or xfail.
The unmodified FIPS baseline's MD5 fixture failure is explicitly documented.

## Subsequent stage complete

The CFFI/TLS implementation and both integration patches pass the final CI gate:
17 full cryptography rows, eight standalone crate jobs, and all five core
pyOpenSSL suites without CFFI. See [the separate acceptance report](validation/tls/acceptance/README.md)
for counts, exact source identities, supported scope, intentional API changes,
and the recorded replacement of the deleted private module import test.
