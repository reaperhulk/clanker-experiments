# RSA integration checkpoint

The complete `nox -e local` session passed on the five recorded native builds
with the independent RSA operations integrated. The checked patch still contains
legacy key-component and serialization adapters, and the original dependencies
remain. This checkpoint does not satisfy the primary acceptance gate.

`report.json` pins the library source hash, integration patch hash, backend
sources, counts, and log hashes. All Python tests ran with the pinned Wycheproof
and X.509 Limbo vectors. Rust tests, documentation tests, formatting, lint, and
type checks ran through the upstream canonical session. The Rust 1.83 wrapper
suite also passed. Existing Python tests and skips were not modified.

The failed BoringSSL run is retained alongside the successful fresh-directory
rerun. Each backend used its own checkout, native installation, and environment.
Unmodified baselines for the other forks, additional upstream configuration
variants, and the complete API migration are still required before a PR.
