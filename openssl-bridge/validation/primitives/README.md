# Expanded primitive integration checkpoint

The exact source and patch hashes in `report.json` identify the code exercised.
The complete nox local session passed on every listed build, including Python
external vectors, Rust tests/doctests, formatting, lint, and type checks.
No tests or skips were changed. Observed pass/skip counts match the earlier
hash/HMAC checkpoint on each backend. Only system OpenSSL has an unmodified
baseline comparison so far.

AWS-LC initially failed with conflicting compiled crate metadata in a reused
build directory. With unchanged source and a fresh Cargo target directory, the
entire canonical session passed. Both logs are included.

This integration still has the original openssl dependencies and temporary
serialization adapters. It does not complete the requested replacement. RSA,
other asymmetric algorithms, the remaining cipher and serialization surface,
provider controls, the rest of the upstream matrix, and subsequent pyOpenSSL
work remain outstanding. No PR has been opened.
