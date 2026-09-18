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

## Subsequent stage

Replace the CFFI/TLS surface used by pyOpenSSL, preserving the operation-specific
safe API boundary. Validate the pinned pyOpenSSL suite against each supported
backend and record its integration patch and test evidence separately. This stage
is not implemented or claimed complete by the primary acceptance report.
