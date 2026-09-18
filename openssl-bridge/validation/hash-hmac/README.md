# Hash/HMAC integration checkpoint

These logs test the exact abstraction commit and integration patch identified in
`report.json`. All five complete `nox -e local` runs passed: formatting, lint,
type checks, Python tests with external vectors, and Rust tests and doctests.
The independent wrapper tests also passed on the five builds and on Rust 1.83.

This patch still uses the original openssl crates for most operations. These
results do **not** satisfy the requested replacement or PR acceptance gate.
Only the system OpenSSL row has an unmodified baseline comparison so far.
No tests or capability skips were changed by this patch. The rest of the pinned
upstream version/configuration matrix, including FIPS, remains untested.

Logs are gzip-compressed UTF-8. The manifest includes the uncompressed integration
log hashes. Local paths in logs identify the isolated test worktrees. Build
configuration and pinned native source revisions are recorded in compatibility/.
