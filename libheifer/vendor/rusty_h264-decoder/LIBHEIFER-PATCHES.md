# libheifer compatibility patches

Source: crates.io rusty_h264-decoder 0.16.0 and rusty_h264-common 0.16.0
(https://github.com/remade-with-rust/rusty_h264, BSD-2-Clause). The published
crates omit the license file; `LICENSE` is copied from the upstream repository
at e8de4d2fe3af1a5de9acb61f1fba5272c5071a0d. Both crates are vendored here and
this file records every change to them.

Build configuration:

- Examples, integration tests, benchmarks and development dependencies are
  removed. The manifests keep only the `std`, `libm` and (decoder) `cabac-trace`
  features. `asm` (the rusty_h264-accel SIMD kernels), `global-alloc` (a
  process-wide `#[global_allocator]`), `knobs`, `census` and `profile` are removed;
  `unexpected_cfgs` is allowed for the now-dead cfg references.
- libheifer builds both crates `no_std` with `libm`: no `RS_H264_*` environment
  knobs, no frame or entropy-decoding threads and no global allocator. The core
  remains `forbid(unsafe_code)`.
- Fixed a missing `Vec` import on the `no_std` diagnostic path in `mb16.rs`.

libheif's only AVC decoder is its OpenH264 plugin, so these changes follow
OpenH264 (the pinned test oracle) where it differs from the unmodified crate:

- Monochrome (`chroma_format_idc == 0`) streams are accepted. Chroma syntax is
  absent (`intra_chroma_pred_mode`, chroma CBP bins, the I_16x16 chroma CBP), the
  4:0:0 CAVLC CBP mappings are used, and chroma is reconstructed as flat 128 as
  OpenH264 outputs it. I_16x16 macroblock types carrying chroma CBP are rejected.
- Frame cropping applies all four SPS offsets in two-sample units, as OpenH264
  does for every chroma format; the crop bound is OpenH264's (a window may crop
  to zero samples).
- CAVLC `level_prefix > 15` (the High-profile extended escape) is rejected,
  matching OpenH264's `MAX_LEVEL_PREFIX`.
- The CABAC engine tracks consumed arithmetic-decoder bits and fails a slice
  when they pass the end of the slice data, as OpenH264's engine does
  (`ERR_CABAC_NO_BS_TO_READ`), including its five-byte initialization window and
  two-byte minimum. The unmodified crate zero-fills past the end and decodes.

These changes preserve reconstruction arithmetic for accepted streams.
