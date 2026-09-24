# Pure Rust codec audit

The candidate must not link or spawn C/C++ codec implementations. The libheif
submodule and native libraries built by the test runner are reference-only.

Initial source audit (2026-09-18):

| Codec | Candidate | Evidence and limitations |
|---|---|---|
| HEVC decode | [rusty_h265 0.6.0](https://github.com/Remade-With-Rust/rusty_h265) | Apache-2.0; decoder plus its Rust SIMD crate; default graph has no native implementation. Its own docs restrict support to Main/Main 10/Main Still 4:2:0, 8/10 bit and reject range extensions, SCC and layered profiles. Must independently compare raw samples before adopting. |
| HEVC alternative | [heic](https://github.com/imazen/heic) | Optional pure Rust backend, but AGPL/commercial licensing and documented missing RExt/inter-prediction functionality. Not selected. Native optional backends are prohibited. |
| AV1 decode | [rav1d 1.1.0](https://github.com/memorysafety/rav1d) | Vendored Rust-only variant, with assembly features, native build script and cc/nasm dependencies removed. Safe internal integration; independent pinned dav1d comparisons cover real and generated 8/10/12-bit samples, monochrome/420/422/444/RGB, alpha, transforms, color warnings and damaged packets. Complete conformance remains open; see AV1_DEPENDENCIES.md. |
| AV1 encode | Vendored [rav1e](https://github.com/xiph/rav1e) 0.8.1 | BSD-2-Clause; assembly sources, nasm/cc build path, binaries and native test features removed (with its av-scenechange dependency). Driven through the same `rav1e::capi` functions libheif's rav1e plugin calls, from a static record transliterating the plugin; the native oracle builds the identical rav1e source without assembly. Compared on encoded files, handles and pixels. |
| JPEG decode | Vendored jpeg-decoder 0.3.2 | Scalar safe Rust with native IDCT/upsampling rounding, truncated entropy recovery and progressive smoothing; independent original-header baseline/progressive, restart, lossless grayscale, precision/error and complete-prefix comparisons. Arithmetic-coded sequential/progressive decoding (SOF9/SOF10, DAC, restart resync) ported from libjpeg-turbo; broader conformance remains open; see JPEG_DEPENDENCIES.md. |
| JPEG2000 decode | Vendored hayro-jpeg2000 0.4.0 | Scalar safe Rust with native wavelet normalization, rounding, signed samples, subsampling, missing tiles and error ordering. Independent pinned OpenJPEG comparisons cover generated lossless/lossy, progression, coding modes and complete prefixes. HTJ2K (T.814) code-blocks decode through a port of OpenJPEG's HT block decoder, compared on OpenJPH-generated and constructed refinement-pass codestreams. Mixed tile transforms, broader conformance and HT mixed mode (rejected, as by OpenJPEG) remain open; see JPEG2000_DEPENDENCIES.md. |
| AVC decode | Vendored [rusty_h264](https://github.com/remade-with-rust/rusty_h264) 0.16.0 | BSD-2-Clause, `forbid(unsafe_code)`, built `no_std` without its OpenH264-assembly accel kernels, allocator, knobs or threads; deblocking uses fearless_simd Rust SIMD kernels with the scalar filters as test oracle. Patched to reproduce OpenH264 reconstruction and error behavior, behind an OpenH264 syntax-layer model. Independent pinned OpenH264 2.6.0 comparisons cover x264-generated profiles, cropping, QP, scaling matrices, deblocking, slices, presets and monochrome, plus a truncation/corruption corpus. See AVC_DEPENDENCIES.md. |
| VVC | [gamut-vvc](https://github.com/justin13888/gamut/tree/main/crates/gamut-vvc) | Pure Rust intra-image codec candidate; workspace also contains an unrelated native JXL crate which must not enter our graph. Requires newer Rust and independent coverage validation. |
| Raw mask decode | In-tree Rust | 8/16-bit mask samples and generated malformed input/conversion cases compared independently; mask encoding is implemented and has independent exact-byte/roundtrip coverage. |
| Registered encoder hooks | In-tree Rust | HEVC/AV1/AVC/VVC/JPEG/JPEG2000/HTJ2K packet configuration, callback lifetimes, alpha/thumbnail conversion, ordinary and compact writing have independent oracle and mutation evidence. These optional caller-provided hooks are separate from the still-open built-in codec encoders. |
| JPEG encode | In-tree Rust (`src/jpeg_encoder.rs`) | Baseline 4:2:0 encoder byte-identical to libheif's libjpeg-turbo 3.1.1 plugin (scalar islow FDCT, reciprocal quantization, standard tables, JFIF density), registered as a static encoder-plugin record with the plugin's parameters; compared on encoded files, handles and pixels. |
| JPEG2000 encode | In-tree Rust (`src/jpeg2000_encoder.rs`) | Scalar encoder byte-identical to libheif's OpenJPEG 2.5.4 plugin: 5/3 and 9/7 lifting, T1 MQ coding with OpenJPEG's context tables, PCRD rate allocation by slope bisection, tag-tree packet headers and the plugin's single-tile LRCP layout. Registered as a static encoder-plugin record with the plugin's parameters; compared on encoded files, handles and pixels. |
| HTJ2K encode | In-tree Rust (`src/htj2k_encoder.rs`) | Scalar encoder byte-identical to libheif's OpenJPH 0.32.0 plugin: OpenJPH's line-based 5/3 and 9/7 lifting and quantisation applied to whole tile-components, its HT cleanup-pass block coder and packet-header tag trees, and its progression, tile and code-block options. Registered as a static encoder-plugin record with the plugin's parameters (those the plugin actually compiles); compared on encoded files, handles and pixels. |
| Remaining codecs/encoders | Open | Built-in AVC encoding, HEVC encoding, full VVC and uncompressed formats require further implementation/audit. |

Repository descriptions and upstream conformance claims are leads, not our test
results. A decoder successfully handling the example image is not complete codec
support. Lossy encoders require interop plus matched quality/rate-distortion
benchmarks; they cannot be judged faster by lowering quality.

JPEG2000 follow-up: per-tile transforms, common sampling, component-count error
ordering and nested channel/palette/layer properties now have normal, sanitizer
and codec-free evidence in `results/jpeg2000-followup-*`. JPEG2000 remains partial.
