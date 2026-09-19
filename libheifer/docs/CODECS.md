# Pure Rust codec audit

The candidate must not link or spawn C/C++ codec implementations. The libheif
submodule and native libraries built by the test runner are reference-only.

Initial source audit (2026-09-18):

| Codec | Candidate | Evidence and limitations |
|---|---|---|
| HEVC decode | [rusty_h265 0.6.0](https://github.com/Remade-With-Rust/rusty_h265) | Apache-2.0; decoder plus its Rust SIMD crate; default graph has no native implementation. Its own docs restrict support to Main/Main 10/Main Still 4:2:0, 8/10 bit and reject range extensions, SCC and layered profiles. Must independently compare raw samples before adopting. |
| HEVC alternative | [heic](https://github.com/imazen/heic) | Optional pure Rust backend, but AGPL/commercial licensing and documented missing RExt/inter-prediction functionality. Not selected. Native optional backends are prohibited. |
| AV1 decode | [rav1d 1.1.0](https://github.com/memorysafety/rav1d) | Vendored Rust-only variant, with assembly features, native build script and cc/nasm dependencies removed. Safe internal integration; independent pinned dav1d comparisons cover real and generated 8/10/12-bit samples, monochrome/420/422/444/RGB, alpha, transforms, color warnings and damaged packets. Complete conformance remains open; see AV1_DEPENDENCIES.md. |
| AV1 encode | [rav1e](https://github.com/xiph/rav1e) | Evaluate without native/assembly defaults; audit the resolved graph and actual build artifacts. Not integrated yet. |
| JPEG2000 decode | [hayro-jpeg2000](https://github.com/LaurenzV/hayro) | Advertises pure Rust; conformance, profiles, encoder support and HTJ2K coverage still require evaluation. |
| VVC | [gamut-vvc](https://github.com/justin13888/gamut/tree/main/crates/gamut-vvc) | Pure Rust intra-image codec candidate; workspace also contains an unrelated native JXL crate which must not enter our graph. Requires newer Rust and independent coverage validation. |
| Raw mask decode | In-tree Rust | 8/16-bit mask samples and generated malformed input/conversion cases compared independently; mask encoding remains open. |
| Remaining codecs/encoders | Open | JPEG, AVC, HEVC encoding, JPEG2000 encoding/HTJ2K, full VVC and uncompressed formats require further implementation/audit. |

Repository descriptions and upstream conformance claims are leads, not our test
results. A decoder successfully handling the example image is not complete codec
support. Lossy encoders require interop plus matched quality/rate-distortion
benchmarks; they cannot be judged faster by lowering quality.
