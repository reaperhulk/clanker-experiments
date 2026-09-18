# Draft: libheifer — pure Rust libheif replacement

This implements an independent Rust library with an optional compatible C ABI,
targeting the complete libheif 1.23.4 contract. All implementation dependencies
are pure Rust. Native libheif and libde265 are independent test oracles only.

**Incomplete: 150 of 465 functions are implemented and remain marked partial;
315 functions are missing.** Keep this PR draft. The strict completion gate
rejects missing APIs and unvalidated behavior. Unsupported stubs and forwarding
to libheif do not count toward coverage.

The current implementation includes:

- The pinned header inventory, staged acceptance plan, optional C adapter,
  dependency audit and independent original-header C clients.
- Brands, image planes, color/HDR metadata, conversion and crop/scale.
- Contexts, retained handles, metadata, thumbnails, auxiliary/depth images,
  item properties, raw/UUID data, owned descriptions and camera matrices.
- Pure Rust direct HEVC decoding, alpha, grids, identities, overlays, masks,
  ordered transforms, warnings, callbacks and context thread controls.
- Versioned security limits, allocation accounting and ownership across reloads,
  malformed SPS rejection, and image-owned error-message buffers.

Recent validation compares 960 property-query/insertion transcripts, 3,772
property-decode cases and 1,641 camera-matrix transcripts. The camera tests check
exact floating-point bits, legacy UUIDs, malformed inputs, transform order,
warnings and objects surviving context/handle release. Both new C clients pass
ASan/UBSan and codec-free builds. Thirteen public struct layouts match the
original headers on Linux x86_64. Four property defects and two camera-math
defects are independently rejected; the default mutation set now has 22 defects.

These tests supplement the existing brand, image, color, context, decode,
geometry, derived-image, auxiliary, error-lifetime and security suites. Exact
binary/client/corpus hashes and per-case evidence are in `docs/results/`;
`docs/RESULTS.md` records the scope and known gaps. Local sanitizer coverage is
limited to C clients, with leak checking disabled where ptrace prevents it.

The preceding property commit (`0e37368`) passed all development CI steps,
all 20 then-present mutation tests and Rust builds on Linux, macOS and Windows.
Only the full-API completion gate failed. Camera evidence is recorded separately
until its CI completes. Cross-platform ABI, fuzzing, full codec conformance,
remaining APIs and whole-library memory-safety validation remain open.

Compatibility comes before optimization. There is no demonstrated performance
win: the initial single-file benchmark was slower and noisy. No full-equivalence
claim is made from these finite passing tests.
