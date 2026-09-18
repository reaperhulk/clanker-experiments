# Draft: libheifer — pure Rust implementation plan and initial compatibility foundation

This starts a pure Rust libheif replacement with an optional compatible C ABI.
The reference contract is libheif 1.23.4. Native code is used only by independent
test oracles; the candidate's resolved implementation dependencies are Rust.

The PR is **not complete** and must remain draft. It implements 119 of 465
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
- Add grid/identity decoding, cycle and MIAF validation, scoped tile workers,
  callback thread/argument checks, warning APIs and context thread controls.
- Add overlay composition, 8/16-bit masks, shared-graph operation budgets and
  dynamic derived-handle queries after partial context reloads.
- Add versioned security limits, safe plane allocation and context-wide accounting
  for image data, decoder input and metadata, including release/reload semantics.
- Reject malformed/oversized HEVC SPS configurations before codec entry and match
  empty, missing and reordered parameter-set behavior.
- Keep error-message buffers on their owning image objects; reproduce and fix a
  cross-handle use-after-free with an independent sanitizer client.
- Add context/handle ownership, reload state, metadata, thumbnails and color queries.
- Validate ten public struct layouts and reject thirteen deliberately mutated
  implementations (enums, errors, pixels, coordinates, item IDs, reload state, worker callbacks, warning text, mask samples, overlay alpha, resource budgets and field order).
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
option cases and 12,240 crop/scale cases match. Another 688 geometry/monochrome
HEVC cases check transform order, fractions, conformance-window cropping and alpha. Short option allocations also pass
C-client ASan/UBSan. Another 640 grid/identity and 1,249 warning/thread-control
cases match. Another 824 overlay, 1,246 mask, 47 adversarial graph and 606
derived/mask handle comparisons match, with C-client sanitizer coverage for the
new handle corpus. Another 2,927 security-limit and 276 resource-lifetime
comparisons match; the security C client also passes ASan/UBSan. Mask comparisons also pass with optional codecs disabled. Callback values and worker placement are checked. Unknown-decoder
error-order cases use one worker because upstream races can select different
errors. A malformed-box brand mismatch remains recorded. Unit/ABI tests, formatting
and Clippy pass on Linux x86_64. Another 400 HEVC configuration and 144 error-lifetime cases match; the lifetime client also passes ASan/UBSan after reproducing the pre-fix use-after-free. A fourteenth deliberate coded-size defect is rejected in a separate local run. The preceding security/overlay CI run passed Rust builds on Linux,
macOS and Windows and all Linux development checks, including mutation tests; its sole failing step was
the strict full-API gate. Cross-platform ABI, fuzzing and memory-safety validation
remain open.

Performance is not yet a success: the candidate's median is higher in the initial
single-file benchmark, and timing variation is substantial. See docs/RESULTS.md
and its raw evidence for scope, numbers and remaining work.
