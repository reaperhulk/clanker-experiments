# AVC decoding dependencies

The optional `avc` feature (enabled by the C adapter's default features) decodes
H.264 with vendored rusty_h264-decoder/rusty_h264-common 0.16.0, built `no_std`
with `libm`, and Rust SIMD kernels through fearless_simd (no C or assembly; the
upstream OpenH264-assembly `asm` feature is removed). See
`vendor/rusty_h264-decoder/LIBHEIFER-PATCHES.md` for every change.

Resolved implementation graph added by the feature:

| Crate | Version | Notes |
|---|---|---|
| rusty_h264-decoder | 0.16.0 | Vendored, BSD-2-Clause, `forbid(unsafe_code)`; features `libm` only. |
| rusty_h264-common | 0.16.0 | Vendored; build script only sets accel cfgs, which stay off without `asm`. |
| wide / safe_arch | 0.7.33 / 0.7.4 | Portable Rust SIMD wrappers over `core::arch`. |
| bytemuck | 1.25.2 | Safe transmutes for the SIMD wrappers. |
| once_cell | 1.21.4 | `race`/`alloc` cells for `no_std`. |
| fearless_simd | 1.0.0 | Apache-2.0 OR MIT; safe `core::arch` wrappers with runtime level dispatch (SSE2/SSE4.2/AVX2/AVX-512, NEON, WASM, scalar fallback); features `libm` and `std` (CPU detection). No build script. |
| libm | 0.2.16 | Rust float math for `no_std`; build script emits cfgs only. |
| portable-atomic | 1.15.0 | Resolved only for targets without 64-bit atomics; build script emits cfgs only. |

`tools/audit_dependencies.py` rejects any other feature set and native source files
in the vendored trees, and pins every reviewed build script by hash.

## Test oracle and fixtures

- Oracle: libheif 1.23.4 with its OpenH264 decoder plugin and OpenH264 v2.6.0
  (`652bdb7719f30b52b08e506645a7322ff1b2cc6f`), built scalar (`USE_ASM=No`) by
  `tools/build_reference.py --avc`. It is test-only and never linked into the candidate.
- Fixtures: `tools/generate_avc_fixtures.py` encodes deterministic inputs with a
  pinned test-only x264 (`b35605ace3ddf7c1a5d67a2eb553f034aef41d55`, `--disable-asm`)
  and stores every Annex B stream, command and input hash in
  `tests/fixtures/avc-generated.json`. Profiles, sizes and cropping, QP sweeps,
  scaling matrices, deblocking parameters, slices, presets, encoder options and
  monochrome streams are covered. Streams OpenH264 rejects (4:2:2, 4:4:4, 10-bit,
  lossless, interlaced, extended CAVLC escapes) are kept for error parity. I_PCM
  streams (x264 chooses PCM under rate-distortion analysis at low QP without
  psy-RD) cover CABAC re-initialization and I_PCM neighbours in both entropy
  modes, including monochrome.

## Adapter behavior

`src/avc.rs` reproduces libheif's OpenH264 plugin: the length-prefixed to Annex B
conversion with its start-code emulation handling, OpenH264's silent rejection of
unsupported SPS profiles, I420 8-bit output, the plugin's error codes and messages,
libheif's ispe-tightened (+16) pixel limit with its SPS coded-size check, and the
plugin's plane allocation limit. `avc1` handles report `avcC` chroma and bit depths
and libheif's missing-`avcC` error.

## OpenH264 syntax layer

`src/avc_openh264.rs` models OpenH264's decisions ahead of reconstruction: its
Annex B splitting and unescaping (`WelsDecodeBs`), its 32-bit-cache bit reader with
bounded over-reads into zeroed padding, `ParseNalHeader` (trailing-zero stripping,
forbidden bit, parameter-set existence), SPS parsing with level limits, VUI and the
HRD parsing quirks of the pinned build, PPS parsing with scaling lists, and slice
headers with reordering, weighted prediction and reference marking. Only accepted
units reach the Rust decoder, as already-unescaped RBSP. An incomplete picture is an
error only when an SEI or delimiter completes its access unit in OpenH264's data
call; otherwise the flush call yields no image and no error.

## SIMD and performance

The upstream crates' fast path is OpenH264's assembly (`rusty_h264-accel`), which
is not used. Deblocking (`vendor/rusty_h264-common/src/simd_deblock.rs`) is Rust
SIMD through fearless_simd with runtime level dispatch, and has no `unsafe`. Each
call filters a 16-sample luma edge, or Cb and Cr packed into 16 lanes with
per-lane thresholds; vertical edges use an in-register transpose. The scalar
line filters stay as the test oracle, and every differential suite checks the
output. libheifer also passes the syntax layer's RBSP straight to the decoder
and skips the padded reference copy of the final picture of a still image.

`tools/bench_avc.py` times memory parse, primary-item decode and teardown
through one C client against each library. It generates 1920x1080 x264 streams
and refuses timings unless every sample's full decoded-plane digest matches the
reference. The table shows median milliseconds over 7 interleaved samples of 5
decodes each, on the 4-core x86-64 (AVX2) development container, rustc 1.94.
`docs/results/avc-decode-benchmark-openh264-*.json` has the raw samples:

| Stream | OpenH264 asm | OpenH264 scalar | libheifer | Before SIMD |
|---|---|---|---|---|
| High CABAC QP 22 | 260.5 | 264.9 | 222.7 | 248.8 |
| High CABAC QP 32 | 23.3 | 35.9 | 19.2 | 34.4 |
| High CAVLC QP 22 | 68.5 | 75.6 | 61.3 | 82.1 |
| Baseline QP 27 | 17.8 | 30.9 | 11.9 | 26.2 |

"OpenH264 asm" is libheif 1.23.4 with OpenH264 2.6.0 built `USE_ASM=Yes`
(`tools/build_reference.py --avc --avc-asm`). It is a performance baseline only;
correctness comparisons use the scalar build. The "Before SIMD" column is the
syntax-layer checkpoint (25ea4cc), measured in the same session (3 samples of 3
decodes) against the asm build. High-bitrate streams are bound by the serial entropy decoders, where
SIMD does not help.

## Known gaps

- Coverage is finite: the truncation/corruption corpus covers six source streams,
  including SPS variants whose VUI HRD reaches OpenH264's error-code loop.
  SVC extension units are rejected as OpenH264's header checks decide, never
  decoded; multi-access-unit input, FMO slice groups and constrained-intra P
  prediction are not yet compared.
- AVC sequences/tracks (P/B slices), registered-plugin priority interaction with
  the built-in decoder and resource-limit corpora remain open.
