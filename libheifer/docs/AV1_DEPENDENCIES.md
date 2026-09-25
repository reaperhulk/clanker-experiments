# AV1 implementation dependencies

The AV1 decoder is rav1d 1.1.0, vendored in `vendor/rav1d` from the crates.io
package with its default features, including the dav1d x86-64 (nasm) and
AArch64 assembly. Upstream keeps its Rust API crate-private and exposes only a
dav1d-compatible `extern "C"` interface, so the vendored copy carries two
changes (`vendor/rav1d/LIBHEIFER_CHANGES.md`):

- `api.rs` adds a safe owned Rust API over the crate's internal functions,
  copying complete active sample planes;
- the dav1d C API functions lose `#[no_mangle]`, so the C API library does
  not export `dav1d_*` entry points that could clash with a real libdav1d.
  The data tables the assembly refers to by name stay exported.

libheifer itself forbids `unsafe` code. rav1d contains its own unsafe Rust
and assembly and is not described as entirely safe Rust. Its x86 assembly
addresses those tables with PC-relative relocations, so the C API crate's
build script links the shared library with `-Bsymbolic` on ELF targets. No
foreign decoder is called.

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
scripts. None compiles C or C++. Standard platform libc declarations,
thread parking, Windows system imports, and CPUID intrinsics are retained; they
do not supply codec implementations. The other new dependencies provide Rust
atomics, containers, bit flags, byte conversions, or procedural macros. Source
inspection found no native codec calls or subprocess decoder path.

The Linux release artifact has only libc, libm, libgcc_s, and the ELF loader as dynamic
dependencies; its dynamic symbols contain no dav1d, rav1d, aom, or de265 entry
points (only the dav1d data tables the assembly refers to by name). Native dav1d 1.5.1 is built separately for differential tests, with its assembly like the candidate's rav1d: on some rav1e-encoded 10/12-bit streams dav1d's SSE4.1/AVX2 paths and its C paths decode differently, and rav1d matches dav1d on each path (C against C and assembly against assembly, 36 cases). Native
libaom/FFmpeg generated the committed synthetic fixtures, and is not required
to consume those fixtures. Platform compatibility and complete codec behavior
still require the broader project gates.

## AV1 encoding (rav1e)

The AV1 encoder is rav1e 0.8.1 from crates.io, unmodified, with its
av-scenechange 0.14.1 dependency (default features off, as rav1e depends on
it). The C API crate enables rav1e's `asm`, `capi` and `threading` features.
Building needs nasm; CI installs it on every runner. `tools/audit_dependencies.py`
pins both feature sets and the build scripts by hash. The audited graph is
the one built for the supported Linux, macOS and Windows targets, so rav1e's
`cfg(fuzzing)` harness dependencies (libfuzzer-sys and others) and wasm-only
dependencies are locked but never compiled. The Rust crates are allowlisted
by exact version:

| Package | Build behavior |
|---|---|
| rav1e, rav1d | Assemble the packaged assembly with nasm (x86-64) or cc (AArch64); rav1e also records build information (`built`) and environment variables |
| av-scenechange | Build information and environment variables (its assembly feature is off) |
| libheifer-capi | Links the shared library with `-Bsymbolic` on ELF targets (rav1d's assembly relocations) |
| getrandom | Memory-sanitizer cfg detection (cc/nasm-rs build tooling) |
| anyhow, thiserror | Compile Rust probes with rustc to select cfgs |
| crossbeam-*, num-traits | Rust version/cfg probes |
| rayon-core | Empty; its `links` key only guards against duplicate versions |
| wasm-bindgen, wasm-bindgen-shared | wasm32 target dependencies (version and schema hash); never built natively |

The native oracle builds librav1e from git tag v0.8.1 (whose `src/` is
identical to the crates.io package) with cargo-c, `--no-default-features
--features asm,capi,threading`, after aligning its lockfile to libheifer's
dependency versions; only proc-macro crates differ.

