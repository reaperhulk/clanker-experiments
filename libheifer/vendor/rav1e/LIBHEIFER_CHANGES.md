# Rust-only rav1e integration

Upstream: xiph/rav1e v0.8.1 (crates.io package; its `src/` is identical to git
tag v0.8.1, commit `1fe82de02510767539e89b2ee6fa846920ae2686`), BSD-2-Clause
with the AOMedia patent license in `PATENTS`.

Changes for libheifer:

- The assembly and C sources (`src/x86`, `src/arm`, `src/ext`) are deleted.
- `build.rs` keeps only upstream's build-information and environment steps;
  the nasm/cc assembly build is removed, as are the `asm`, `cc` and `nasm-rs`
  features and build dependencies.
- The command-line binaries, benchmarks, tests, the native aom/dav1d decoder
  test features and the optional serialization, tracing, channel and wasm
  features are removed from the manifest. `capi` and `threading` remain.

No Rust source is changed. The native rav1e library built for the libheif
oracle comes from the same git revision with `--no-default-features --features
capi,threading` (no assembly), so both sides run the same encoder code.
