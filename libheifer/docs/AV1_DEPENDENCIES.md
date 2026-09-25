# AV1 implementation dependencies

The AV1 decoder is rav1d 1.1.0 from crates.io, unmodified, with only the
`bitdepth_8` and `bitdepth_16` features. Its `asm` features are off, so its
build script does nothing. Its unconditional cc and nasm-rs build-dependencies
are compiled as build tooling and never invoked. The package still ships its
assembly and C sources, but they are not compiled. rav1d's public API is its
dav1d-compatible `extern "C"` interface. `src/rav1d_api.rs` wraps it as an
owned decoder that copies complete active sample planes. It is the only
libheifer module allowed to use `unsafe` (the crate denies `unsafe_code` and
this module opts out). rav1d itself contains unsafe Rust and is not described
as entirely safe Rust. Its `dav1d_*` entry points are exported from the C API
library, as rav1e's `rav1e_*` entry points already were. No foreign decoder is
called.

The resolved graph is allowlisted by exact package/version in
`tools/audit_dependencies.py`. Compiler/platform build scripts are allowed only
with the reviewed hashes in `dependency-build-scripts.json`:

| Package | Build behavior |
|---|---|
| libc | Rust/compiler and platform version probes; ABI cfg selection |
| parking_lot_core | Sanitizer cfg detection |
| paste, quote | Rust compiler version and cfg detection |
| proc-macro2 | Compiles Rust metadata-only API probes; emits cfgs |
| rustversion | Parses rustc version and writes a Rust version expression |

The probe files and rustversion's parser are hash checked alongside the entry
scripts. None compiles C, C++, or assembly. Standard platform libc declarations,
thread parking, Windows system imports, and CPUID intrinsics are retained; they
do not supply codec implementations. The other new dependencies provide Rust
atomics, containers, bit flags, byte conversions, or procedural macros. Source
inspection found no native codec calls or subprocess decoder path.

The Linux release artifact has only libc, libgcc_s, and the ELF loader as dynamic
dependencies; its dynamic symbols contain no dav1d, rav1d, aom, or de265 entry
points. Native dav1d 1.5.1 is built separately for differential tests. Native
libaom/FFmpeg generated the committed synthetic fixtures, and is not required
to consume those fixtures. Platform compatibility and complete codec behavior
still require the broader project gates.

## AV1 encoding (rav1e)

The AV1 encoder is rav1e 0.8.1 from crates.io, unmodified, with its
av-scenechange 0.14.1 dependency (default features off). rav1e's `asm`
default feature is off, and the C API crate enables only `capi` and
`threading`. Both build scripts compile their packaged assembly only under
`asm`. `tools/audit_dependencies.py` pins both feature sets and the build
scripts by hash. The audited graph is the one built for the supported Linux,
macOS and Windows targets, so rav1e's `cfg(fuzzing)` harness dependencies
(libfuzzer-sys and others) and wasm-only dependencies are locked but never
compiled. The pure Rust crates are allowlisted by exact version:

| Package | Build behavior |
|---|---|
| rav1e, av-scenechange | Build information (`built`) and environment variables only; assembly only under the disabled `asm` feature |
| rav1d | Empty without `asm` |
| getrandom | Memory-sanitizer cfg detection (cc/nasm-rs build tooling for rav1d) |
| anyhow, thiserror | Compile Rust probes with rustc to select cfgs |
| crossbeam-*, num-traits | Rust version/cfg probes |
| rayon-core | Empty; its `links` key only guards against duplicate versions |
| wasm-bindgen, wasm-bindgen-shared | wasm32 target dependencies (version and schema hash); never built natively |

The native oracle builds librav1e from git tag v0.8.1 (whose `src/` is
identical to the crates.io package) with cargo-c, `--no-default-features
--features capi,threading`, after aligning its lockfile to libheifer's
dependency versions; only proc-macro crates differ.

