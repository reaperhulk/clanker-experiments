# Draft: libheifer — pure Rust libheif replacement

This implements an independent Rust library with an optional compatible C ABI,
targeting the complete libheif 1.23.4 contract. All implementation dependencies
are pure Rust. Native libheif and libde265 are independent test oracles only.

**Incomplete: 211 of 465 functions are implemented and remain marked partial;
254 functions are missing.** Keep this PR draft. The strict completion gate
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

Recent validation fixes the recorded malformed-brand mismatch, with 93,398
first-box transcripts covering nested errors, optional children, counts,
versions and parent/stream boundaries across 26 box types and camera UUIDs.
Other box classes remain open. The suite also
passes C-client ASan/UBSan. Three new mutations bring the default set to 35.

The preceding increment adds 791 handle/decoded-component transcripts, covering
HEVC and JPEG header descriptions, item parse order, derived alpha bit depths,
component IDs after decoding/conversion, and retained descriptions after context
reload/free. Five HEVC fixtures and generated mask/derived files are included.
The C client passes ASan/UBSan; all descriptions also match without codecs.
The preceding component suite compares 67,329 transcripts and seventeen public
struct layouts match the original headers on Linux x86_64. All 39 component
export names are present; remaining codec paths and serialized content IDs
still prevent full compatibility. Four new deliberate defects are rejected;
that increment brought the mutation set to 32 defects.

These tests supplement the existing brand, image, color, context, decode,
geometry, derived-image, auxiliary, error-lifetime and security suites. Exact
binary/client/corpus hashes and per-case evidence are in `docs/results/`;
`docs/RESULTS.md` records the scope and known gaps. Local sanitizer coverage is
limited to C clients, with leak checking disabled where ptrace prevents it.

The preceding handle-component commit (`4c6e110`) passed all development CI steps,
all 32 then-present mutation tests and Rust builds on Linux, macOS and Windows.
Only the full-API completion gate failed. Cross-platform ABI, fuzzing, full codec
conformance, remaining APIs and whole-library memory-safety validation remain open.

Compatibility comes before optimization. There is no demonstrated performance
win: the initial single-file benchmark was slower and noisy. No full-equivalence
claim is made from these finite passing tests.
