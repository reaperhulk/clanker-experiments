# Cipher integration checkpoint

The full upstream `nox -e local` command passed on the five recorded native
builds during this integration stage. The report pins each row to its exact
published source tree and patch. It includes Python tests with Wycheproof and
X.509 Limbo vectors, Rust tests/doc tests, formatting, Clippy, and Python checks.

Backend fixes were made between rows. This is an incremental compatibility
record, not a claim that a single final source snapshot passed the acceptance
matrix. The original OpenSSL dependencies and serialization adapters remain.
The failed BoringSSL import check and AWS-LC Blowfish ECB test are retained
alongside the successful reruns. No tests or existing skips were removed.

The XTS API now permits exactly one complete data unit per context; its added
Python regression test accounts for the extra test relative to the RSA stage.
The final replacement still requires the complete upstream configuration matrix
and comparison with unmodified baselines before opening a PR.
