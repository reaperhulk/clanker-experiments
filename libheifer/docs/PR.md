# Draft: libheifer — pure Rust implementation plan and initial compatibility foundation

This starts a pure Rust libheif replacement with an optional compatible C ABI.
The reference contract is libheif 1.23.4. Native code is used only by independent
test oracles; the candidate's resolved implementation dependencies are Rust.

The PR is **not complete** and must remain draft. It implements 109 of 465
functions, plus the public success object. The strict completion check fails on
missing and unvalidated APIs. Unsupported-operation stubs and libheif forwarding
are not used to inflate coverage.

Changes:

- Pin the complete header/API inventory and a staged implementation/release plan.
- Add a Rust core and optional C ABI package for brands, image primitives, color/HDR metadata, contexts and image handles.
- Compile the same C clients against original upstream headers and compare
  independently linked reference/candidate processes, including errors and outputs.
- Add pure Rust direct-item HEVC C decoding, alpha attachment, ordered YCbCr/RGB
  conversion and packing, crop/scale, and versioned decoding options.
- Add context/handle ownership, reload state, metadata, thumbnails and color queries.
- Validate nine public struct layouts and reject seven deliberately mutated
  implementations (enums, errors, pixels, coordinates, item IDs, reload state and field order).
- Add dependency auditing, ABI checks, CI development checks, and an intentionally
  failing full-completion gate.
- Record raw interleaved performance samples and a codec stage profile.

Local validation: 17,126 brand/version, 6,553 image and 198,932 color/HDR
transcripts match; 1,786 context/handle transcripts match, including malformed
properties, copied-buffer ownership, aliases, post-release handles and failed reloads.
ASan/UBSan C clients pass that corpus (libraries are not instrumented; local leak
checking was disabled because LeakSanitizer cannot run under ptrace).
115 decode comparisons match across five HEIC fixtures and 23 modes, including
alpha, default conversion, explicit profiles and 16-bit byte order. 65,549 decoding
option cases and 12,240 crop/scale cases match. Short option allocations also pass
C-client ASan/UBSan. A malformed-box brand mismatch remains recorded. Unit/ABI tests, formatting
and Clippy pass on Linux x86_64. The color/HDR CI run passed Rust builds on Linux,
macOS and Windows and all Linux development checks, including mutation tests; its sole failing step was
the strict full-API gate. Cross-platform ABI, fuzzing and memory-safety validation
remain open.

Performance is not yet a success: the candidate's median is higher in the initial
single-file benchmark, and timing variation is substantial. See docs/RESULTS.md
and its raw evidence for scope, numbers and remaining work.
