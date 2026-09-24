# AVC decoding dependencies

The optional `avc` feature (enabled by the C adapter's default features) decodes
H.264 with vendored rusty_h264-decoder/rusty_h264-common 0.16.0, built `no_std`
with `libm`. See `vendor/rusty_h264-decoder/LIBHEIFER-PATCHES.md` for every change.

Resolved implementation graph added by the feature:

| Crate | Version | Notes |
|---|---|---|
| rusty_h264-decoder | 0.16.0 | Vendored, BSD-2-Clause, `forbid(unsafe_code)`; features `libm` only. |
| rusty_h264-common | 0.16.0 | Vendored; build script only sets accel cfgs, which stay off without `asm`. |
| wide / safe_arch | 0.7.33 / 0.7.4 | Portable Rust SIMD wrappers over `core::arch`. |
| bytemuck | 1.25.2 | Safe transmutes for the SIMD wrappers. |
| once_cell | 1.21.4 | `race`/`alloc` cells for `no_std`. |
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
  lossless, interlaced, extended CAVLC escapes) are kept for error parity.

## Adapter behavior

`src/avc.rs` reproduces libheif's OpenH264 plugin: the length-prefixed to Annex B
conversion with its start-code emulation handling, OpenH264's silent rejection of
unsupported SPS profiles, I420 8-bit output, the plugin's error codes and messages,
libheif's ispe-tightened (+16) pixel limit with its SPS coded-size check, and the
plugin's plane allocation limit. `avc1` handles report `avcC` chroma and bit depths
and libheif's missing-`avcC` error.

## Known gaps

- Error-path parity on malformed streams is incomplete. `tools/test_avc_errors.py`
  (truncations, corrupted slice bytes, malformed SPS/PPS, NAL framing, avcC
  prefixes) still has recorded differences, mostly where OpenH264's own NAL
  splitting, SPS/VUI/PPS and slice-header validation reject streams the Rust
  decoder accepts. It is not a CI gate until it reaches parity.
- CABAC I_PCM, AVC sequences/tracks (P/B slices), registered-plugin priority
  interaction with the built-in decoder, and resource-limit corpora remain open.
