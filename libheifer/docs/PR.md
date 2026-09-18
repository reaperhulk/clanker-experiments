# Draft: libheifer — pure Rust implementation plan and initial compatibility foundation

This starts a pure Rust libheif replacement with an optional compatible C ABI.
The reference contract is libheif 1.23.4. Native code is used only by independent
test oracles; the candidate's resolved implementation dependencies are Rust.

The PR is **not complete** and must remain draft. It implements 65 of 465
functions, plus the public success object. The strict completion check fails on
missing and unvalidated APIs. Unsupported-operation stubs and libheif forwarding
are not used to inflate coverage.

Changes:

- Pin the complete header/API inventory and a staged implementation/release plan.
- Add a Rust core and optional C ABI package for brands, image primitives, color profiles and HDR metadata.
- Compile the same C clients against original upstream headers and compare
  independently linked reference/candidate processes, including errors and outputs.
- Add an experimental pure Rust direct-item HEVC decoder and exact native-plane
  comparisons, with the default color-conversion gap reported separately.
- Validate eight public struct layouts and reject five deliberately mutated
  implementations (enums, errors, pixels, coordinates and field order).
- Add dependency auditing, ABI checks, CI development checks, and an intentionally
  failing full-completion gate.
- Record raw interleaved performance samples and a codec stage profile.

Local validation: 17,126 brand/version, 6,553 image and 198,932 color/HDR
transcripts match;
5/5 HEIC fixtures match native Y/Cb/Cr output. One default-color-output mismatch
and a malformed-box brand mismatch remain recorded. Unit/ABI tests, formatting
and Clippy pass on Linux x86_64. The initial CI run passed Rust builds on Linux,
macOS and Windows and all Linux development checks; its sole failing step was
the strict full-API gate. Cross-platform ABI, fuzzing and memory-safety validation
remain open.

Performance is not yet a success: the candidate's median is higher in the initial
single-file benchmark, and timing variation is substantial. See docs/RESULTS.md
and its raw evidence for scope, numbers and remaining work.
