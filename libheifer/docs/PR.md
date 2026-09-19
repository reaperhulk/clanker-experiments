# Draft: libheifer — pure Rust libheif replacement

This implements an independent Rust library with an optional compatible C ABI,
targeting the complete libheif 1.23.4 contract. All implementation dependencies
are pure Rust. Native libheif and libde265 are independent test oracles only.

**Incomplete: 234 of 465 functions are implemented and remain marked partial;
231 functions are missing.** Keep this PR draft. The strict completion gate
rejects missing APIs and unvalidated behavior. Unsupported stubs and forwarding
to libheif do not count toward coverage.

The current implementation includes:

- The pinned header inventory, staged acceptance plan, optional C adapter,
  dependency audit and independent original-header C clients.
- Brands, image planes, component descriptions and typed access, color/HDR
  metadata, conversion and crop/scale.
- Contexts, retained handles, metadata, thumbnails, auxiliary/depth images,
  item properties, raw/UUID data, owned descriptions, camera matrices and in-memory sensor metadata.
- Pure Rust direct HEVC decoding, alpha, grids, identities, overlays, masks,
  ordered transforms, warnings, callbacks and context thread controls.
- Versioned security limits, allocation accounting and ownership across reloads,
  malformed SPS rejection, and image-owned error-message buffers.

This increment adds 23 generic-item and compression APIs. All declarations in
`heif_items.h` now have exports. The independent C client matches 2,840 cases,
including full byte-for-byte payload comparisons, malformed streams, exact error
text, item-table duplicates, reference order, failed-add IDs and owned values
after context destruction. Another 430 cases compare compressed image metadata.
Both suites pass C-client ASan/UBSan. Compression uses pure Rust inflation and an
in-tree Rust adaptation of zlib's default encoder; no C implementation dependency
is introduced. Six new deliberate defects are rejected, bringing the mutation
set to 41. Both new suites also pass without codec features. Brotli and remaining item/file behavior are still required work.

The preceding first-box increment compares 93,398 transcripts across 26 box types
and camera UUIDs. Component coverage includes 67,329 image and 791 handle/decode
transcripts, with seventeen public struct layouts checked against original
headers. Existing context, auxiliary, handle-component, security and derived
checks also pass against this increment.

These tests supplement the existing brand, image, color, context, decode,
geometry, derived-image, auxiliary, error-lifetime and security suites. Exact
binary/client/corpus hashes and per-case evidence are in `docs/results/`;
`docs/RESULTS.md` records the scope and known gaps. Local sanitizer coverage is
limited to C clients, with leak checking disabled where ptrace prevents it.

The preceding brand-box commit (`1d38abc`) passed all development CI steps,
all 35 then-present mutation tests and Rust builds on Linux, macOS and Windows.
Only the full-API completion gate failed. Cross-platform ABI, fuzzing, full codec
conformance, remaining APIs and whole-library memory-safety validation remain open.

Compatibility comes before optimization. There is no demonstrated performance
win: the initial single-file benchmark was slower and noisy. No full-equivalence
claim is made from these finite passing tests.
