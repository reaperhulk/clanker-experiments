# Rust JPEG integration

Upstream: image-rs/jpeg-decoder 0.3.2, commit
`eb2d7c0f6a2d0298aba7a7f8b9ca1440353e8f8c` (MIT OR Apache-2.0).
Published crate checksum:
`00810f1d8b74be64b13dbf3db89ac67740615d6c891f0e7b6179326533011a07`.

Rust source and license files are retained. The manifest omits upstream test,
example, benchmark and development dependencies. The candidate disables the
optional Rayon default and enables `platform_independent`; no native codec,
assembly, build script or foreign JPEG API is linked. The dependency guard checks
these resolved features. Obsolete `asmjs` cfg alternatives are removed.

The full-resolution scalar integer IDCT now uses 13-bit constants, symmetric
negative-constant rounding and corresponding pass shifts/biases, matching the
pinned native libjpeg-turbo slow integer IDCT. Reduced-resolution IDCT paths are
unchanged and are not used by libheifer. Scalar chroma upsampling uses native
alternating rounding biases and native nearest-neighbor selection for component
widths <= 2. Degenerate dimensions retain the encoded sampling ratios.

The independent oracle is libheif 1.23.4 with native libjpeg-turbo 3.1.1. Native
sources are test-only. Fixture pixels are encoded by native cjpeg from owned
synthetic patterns; expected decoded bytes come from the original-header native
client, never this implementation. This integration is not a complete JPEG
conformance claim; lossless profiles, malformed streams and other open
compatibility requirements remain subject to implementation and tests.

The follow-up compatibility work also matches native post-IDCT range limiting,
zero-symbol recovery for invalid entropy codes, MCU preservation after truncated
entropy, default Huffman tables and sequential scan parameter tolerance. Native
marker/error ordering is handled by the safe libheifer adapter and scan parser.
Progressive coefficients retain current/prior scan precision and the last valid
MCU row. `src/smoothing.rs` implements the native five-by-five coefficient
estimates, including padded edge blocks, with original IJG attribution and
`LICENSE-IJG`; the combined manifest license reflects that addition. This
software is based in part on the work of the Independent JPEG Group.

Ordinary and progressive Huffman JPEG, restart intervals, grayscale lossless
predictors/point transforms and 2–8-bit lossless sample precision have independent
original-header comparisons. Higher-precision and RGB conversion errors are
compared separately. Every byte prefix of representative baseline, progressive
and grayscale streams is tested.

`src/arithmetic.rs` ports libjpeg-turbo's arithmetic entropy decoder
(jdarith.c and jaricom.c, developed by Guido Vollbeding for the IJG) for
sequential (SOF9) and progressive (SOF10) DCT frames. It covers the QM-coder
statistics and conditioning, DAC segments, restart statistics resets and
`jpeg_resync_to_restart` actions, the zero data supplied after a marker inside a
scan, and the code-error state that stops a scan. Arithmetic lossless (SOF11) is
rejected, as libjpeg-turbo has no decoder for it. Markers follow libjpeg's
`read_markers`: reserved and hierarchical markers are "Unsupported marker type",
differential SOF types and JPG are unsupported processes, a second supported SOF
is a structure error, and RSTn/TEM are skipped anywhere. `Marker::RES` keeps its
code.
