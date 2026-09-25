# libheifer changes to rav1d

Upstream: rav1d 1.1.0 from crates.io (memorysafety/rav1d, BSD-2-Clause),
published package unmodified apart from the following. Default features are
used, including the x86-64 (nasm) and AArch64 assembly.

- `api.rs` (declared at the end of `lib.rs`) adds a safe, owned Rust API
  over the crate's internal `rav1d_open`, `rav1d_send_data`,
  `rav1d_get_picture` and `rav1d_close`. Upstream keeps these, and its Rust
  types, crate-private; its only public interface is the dav1d-compatible
  C ABI. The API copies complete active sample planes out of each picture.
- `#[no_mangle]` is removed from the dav1d C API functions (`src/lib.rs` and
  `dav1d_set_cpu_flags_mask` in `src/cpu.rs`), so libheifer's shared library
  does not export `dav1d_*` entry points that could clash with a real
  libdav1d in the same process. The `#[no_mangle]` data tables in
  `src/tables.rs` are kept: the assembly refers to them by name.

No other source is changed.
