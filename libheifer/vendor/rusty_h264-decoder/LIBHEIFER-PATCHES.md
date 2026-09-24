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
- `rust-version` is 1.89, fearless_simd's minimum.
- A `simd-detect` feature (both crates) enables fearless_simd's `std` feature for
  runtime CPU detection only; the crates themselves stay `no_std`.
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
- CAVLC I slices end exactly at the RBSP stop bit and fail when a macroblock
  reads past it (`WelsDecodeMbCavlcISlice`), instead of `more_rbsp_data()`.
- Invalid `coeff_token` patterns decode as TotalCoeff 0 consuming 8 bits
  (nC < 8) or 6 bits (nC >= 8), as OpenH264's VLC tables map them, rather than
  failing. The mapping was extracted from OpenH264's own tables for every 16-bit
  pattern; chroma DC tokens already agreed.
- Intra 4x4/8x8, 16x16 and chroma prediction modes that need an unavailable
  left, top or top-left neighbour (and chroma modes above 3) fail the slice
  (`CheckIntra{NxN,16x16,Chroma}PredMode`). Top-left availability is derived
  from slice membership rather than assumed from top and left.
- Separate Cb and Cr chroma QP offsets (`second_chroma_qp_index_offset`) in
  reconstruction and deblocking; the upstream crate applied the Cb offset to
  both planes. The (unbuilt) accel deblock arm keeps the Cb offset.
- A slice may start a picture at any `first_mb_in_slice`, and a picture is
  complete only when the macroblocks decoded across its slices equal the
  picture size (`iTotalNumMbRec`).
- Scaling-list dequantization uses OpenH264's factors: the 4x4 factor
  `weight * normAdjust << qp/6` is stored as `uint16_t` (wrapping), and QP 51,
  whose table row OpenH264 never initializes, has zero factors for 4x4 and 8x8.
  Coefficients are stored as `int16_t` after dequantization, in the luma/chroma
  DC transforms, the 4x4 inverse transform's row pass and every 8x8 inverse
  transform temporary, as in OpenH264's C implementation.

Performance changes (no C or assembly; Rust SIMD through fearless_simd 1.0):

- `simd_deblock.rs` replaces the scalar per-line loop filter with fearless_simd
  kernels that filter a whole 16-sample luma edge, or the Cb and Cr edges of a
  macroblock packed into one 16-lane vector, per call (vertical edges through
  an in-register zip transpose). Each lane carries its own alpha, beta, tc0 and
  boundary strength, so the separate Cb/Cr offsets are kept. The scalar line
  filters are retained as the test oracle (`simd_matches_scalar`, 20,000
  randomized edges at the detected and the baseline SIMD level).
- `Decoder::decode_units` / `decode_units_still` accept already split and
  unescaped NAL units, so libheifer's OpenH264 syntax layer hands over its RBSP
  instead of re-escaping it for a second unescape pass. `decode_units_still`
  (a decoder dropped after one call) skips the padded reference copy of a
  picture completed by the final slice unit; no later slice can reference it,
  and an incomplete picture is an error rather than a reference.
- The CABAC input-exhaustion rule is evaluated where the decoder queries it
  instead of after every bin. Consumed bits only grow, so the result is the
  same; an I_PCM re-initialization carries the state across.

For streams whose intermediate values stay in the 16-bit range and whose
factors do not wrap, these changes preserve the standard reconstruction.
