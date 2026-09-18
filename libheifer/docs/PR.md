# Draft: libheifer — pure Rust libheif replacement

This implements an independent Rust library with an optional compatible C ABI,
targeting the complete libheif 1.23.4 contract. All implementation dependencies
are pure Rust. Native libheif and libde265 are independent test oracles only.

**Incomplete: 206 of 465 functions are implemented and remain marked partial;
259 functions are missing.** Keep this PR draft. The strict completion gate
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

Recent validation adds 67,329 component transcripts, covering every 16-bit
reference-component type, all 1–128-bit sample depths, datatype passthrough,
all typed pointer accessors, byte/element strides, duplicate channels,
reference-only entries, output sentinels and crop/scale behavior. ASan/UBSan
C clients and the codec-free build pass. Three additional deliberate defects
are detected; the default mutation set now contains 28 defects. Seventeen
public struct layouts match the original headers on
Linux x86_64. Five handle-side component APIs, multi-component file/codec paths
and content-ID serialization remain open.

These tests supplement the existing brand, image, color, context, decode,
geometry, derived-image, auxiliary, error-lifetime and security suites. Exact
binary/client/corpus hashes and per-case evidence are in `docs/results/`;
`docs/RESULTS.md` records the scope and known gaps. Local sanitizer coverage is
limited to C clients, with leak checking disabled where ptrace prevents it.

The preceding sensor commit (`8f07766`) passed all development CI steps,
all 25 then-present mutation tests and Rust builds on Linux, macOS and Windows.
Only the full-API completion gate failed. Cross-platform ABI, fuzzing, full codec
conformance, remaining APIs and whole-library memory-safety validation remain open.

Compatibility comes before optimization. There is no demonstrated performance
win: the initial single-file benchmark was slower and noisy. No full-equivalence
claim is made from these finite passing tests.
