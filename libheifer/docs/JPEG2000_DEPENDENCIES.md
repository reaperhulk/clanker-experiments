# JPEG2000 implementation dependencies

The candidate vendors hayro-jpeg2000 0.4.0 from LaurenzV/hayro at
`a728f7cb6826c1e167973b8fd71867ebb4cf39af`. Only the Rust sources, profile assets
and license/notices are retained. The standalone manifest enables only `std`;
optional SIMD, image and logging integrations are disabled. This scalar subset
builds with Rust 1.90. There are no transitive dependencies, build scripts, native
codec sources or external decoder calls. The crate forbids unsafe Rust. The
resolved dependency guard requires exactly the reviewed feature set.

The fork exposes unshifted component samples, tracks present tile rectangles,
and leaves absent tiles zero-filled. Irreversible inverse-wavelet scaling and
coefficient dequantization match the pinned OpenJPEG implementation. Its BSD
license is retained as LICENSE-OPENJPEG; upstream Rust sources retain their
MIT/Apache-2.0 licenses and profile assets retain their own notices. See the
vendor LIBHEIFER_CHANGES.md for the exact integration changes.

The safe adapter validates SIZ geometry and marker boundaries, preserves native
header/decode error ordering, applies allocation and dimension limits, and writes
owned monochrome or 4:4:4/4:2:2/4:2:0 planes. Samples are rounded before integer
unsigned level shifting, then clamped and stored with native signed-sample
truncation. Compressed input uses the shared retained input reservation.

The independent oracle is libheif 1.23.4 at
`4e14f5942c1732ace9611b9522cc991501445463`, built by
`tools/build_reference.py --jpeg2000` with OpenJPEG 2.5.4 at
`6c4a29b00211eb0430fa0e5e890f1ce5c80f409f`. OpenJPEG is test-only. The fixture
generator explicitly sets its native library search path to that pinned install,
so a system OpenJPEG cannot silently replace the generator runtime. The manifest
records commands, owned input hashes, binary hashes, codestream hashes and all
native encoder failures. Failed native generation is excluded from the successful
fixture count. Expected metadata, errors and samples are obtained only from
separately compiled original-header clients linked to the native oracle.

The C adapter enables the optional `jpeg2000` feature by default. Codec-free
validation links a separately built candidate without defaults and compares it
against a native oracle with OpenJPEG disabled. Local sanitizer client runs
disable leak checking under ptrace; CI retains it. Finite test coverage does not
establish complete codec conformance. Per-tile transforms, raw component-count
ordering, common sample grids, packet extents and nested channel/palette/layer
properties have independent follow-up evidence. HTJ2K code-blocks use an in-tree
port of OpenJPEG's BSD-2-Clause `ht_dec.c` and its VLC tables (no native code);
OpenJPH is only a pinned fixture generator. Tile-part progression changes,
wider precision, remaining header semantics, HT mixed mode, HTJ2K encoding,
sequence integration and broader platform/downstream gates remain open.

JPEG2000 encoding is in-tree Rust (`src/jpeg2000_encoder.rs`), written against
OpenJPEG 2.5.4's encoder (revision 6c4a29b) with no native code. OpenJPEG's
encoder is built into the `.build/reference-jpeg2000` and
`.build/reference-encoders` oracles only.
