# AV1 implementation dependencies

The vendored rav1d 1.1.0 source is pinned to
`782dab2135ea64a057c097088a13eb8ed3cc3320`. Its manifest contains only the
`bitdepth_8` and `bitdepth_16` features. The original assembly build script,
assembly sources, `cc`/nasm dependencies, and unmangled dav1d entry points are
absent. The added safe API calls internal Rust functions and owns the decoder,
packets, pictures, and copied pixel buffers. No foreign decoder is called.
The core retains `forbid(unsafe_code)`; upstream rav1d contains its own unsafe
Rust implementation and is not described as entirely safe Rust.

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
