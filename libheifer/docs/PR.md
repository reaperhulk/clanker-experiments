# Draft: libheifer — pure Rust libheif replacement

This implements an independent Rust library with an optional compatible C ABI,
targeting the complete libheif 1.23.4 contract. All implementation dependencies
are pure Rust. Native libheif and libde265 are independent test oracles only.

**Incomplete: 276 of 465 functions are implemented and remain marked partial;
189 functions are missing.** Keep this PR draft. The strict completion gate
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

This increment adds thirteen handle HDR/aspect APIs. Independent clients match
716 cases and 7,160 successful decodes, including metadata inheritance, retained
properties, malformed boxes and setter behavior across reloads. C-client ASan/UBSan
passes. Six added mutations are detected; the default suite contains 64 defects.
All color and image header declarations now have partial implementations.

The image-area increment adds image area extraction and both extension APIs. The independent
client matches 3,365 cases and complete pixel streams, including padding, odd
chroma origins, metadata, component IDs and resource limits. C-client ASan/UBSan
passes. Five added mutations are rejected, bringing the default set to 58.

The text increment adds all nine text-item declarations. Independent clients match
1,309 cases covering creation, attachments, byte content, language ownership,
compressed/malformed input, pending payloads and retained lookup state across
changed-content reloads. C-client ASan/UBSan and codec-free checks pass. The
suite also exposed and fixed missing/unterminated item-info string handling.
Four new mutations are rejected, bringing the default set to 53.
File serialization and fresh encoded-image language insertion remain open.

The metadata-writer increment adds five metadata-writing APIs. The independent client matches
2,187 cases and complete byte-for-byte payload streams, including Exif offsets,
XMP compression, URI behavior, foreign handles, reference targets, resource limits
and metadata visibility after mutation. The same cases pass C-client ASan/UBSan
and codec-free builds. File serialization and allocation-failure coverage remain
open; all metadata declarations now have partial implementations. Three new
mutations are rejected, bringing the default set to 49.

The timestamp increment adds all twelve TAI clock/timestamp declarations. Independent C
clients match 67,745 cases, including every uint8 source/destination version pair,
raw flags, typed property equality, box versions/truncations, ownership, crop/scale,
HEVC/mask/derived decode propagation and retained handles after reloads. Both new
structs match the original headers, bringing the layout suite to nineteen.
The suite passes C-client ASan/UBSan and a clearly scoped 67,728-case codec-free
subset. Five new deliberate defects are rejected, bringing the mutation set to 46.
Sequence timing, serialization and broader reload behavior remain open.

The preceding item increment adds 23 APIs and matches 2,840 cases, including
full byte-for-byte payload comparisons, plus 430 compressed-metadata cases.
Its pure Rust inflater and default encoder introduce no C implementation
dependency. Both suites pass C-client sanitizers and codec-free checks; six
deliberate defects brought the mutation set to 41. Brotli remains required work.

The preceding first-box increment compares 93,398 transcripts across 26 box types
and camera UUIDs. Component coverage includes 67,329 image and 791 handle/decode
transcripts, with nineteen public struct layouts checked against original
headers. Existing context, auxiliary, handle-component, security and derived
checks also pass against this increment.

These tests supplement the existing brand, image, color, context, decode,
geometry, derived-image, auxiliary, error-lifetime and security suites. Exact
binary/client/corpus hashes and per-case evidence are in `docs/results/`;
`docs/RESULTS.md` records the scope and known gaps. Local sanitizer coverage is
limited to C clients, with leak checking disabled where ptrace prevents it.

The timestamp commit (`7350505`) passed all development CI steps,
all 46 then-present mutation tests and Rust builds on Linux, macOS and Windows.
Only the full-API completion gate failed. Cross-platform ABI, fuzzing, full codec
conformance, remaining APIs and whole-library memory-safety validation remain open.

Compatibility comes before optimization. There is no demonstrated performance
win: the initial single-file benchmark was slower and noisy. No full-equivalence
claim is made from these finite passing tests.
