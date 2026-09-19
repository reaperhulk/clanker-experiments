# Current result: incomplete implementation, no demonstrated performance win

Reference: libheif 1.23.4, commit `4e14f5942c1732ace9611b9522cc991501445463`.
HEVC reference backend: libde265 1.0.16, commit
`7ba65889d3d6d8a0d99b5360b028243ba843be3a`. Candidate uses only Rust code;
the HEVC feature uses rusty_h265/rusty_h265-accel 0.6.0. The former is vendored
with VUI unspecified-color, monochrome-decoding and typed missing-parameter fixes; its Apache-2.0 notices are retained.

## Implemented scope

279 of 465 public functions are exported, plus `heif_error_success`.
186 functions are missing. Even the exported functions are marked **partial**:
coverage is finite, platform coverage is incomplete, and one malformed-box
behavior gap is explicitly retained. There is no claim of a compatible library.

- Version, brands and MIME/file sniffing; owned compatible-brand lists.
- Image creation, aligned zeroed planes, layout/stride/bit-depth/channel queries,
  plane access, alpha-premultiplication flag and pixel aspect ratio.
- Color-conversion option defaults/copy, ICC and NCLX profile storage/queries,
  HDR content light, mastering display, ambient viewing and diffuse white metadata.
  Ordered minimum-cost YCbCr/RGB conversion, chroma sampling and 8/16-bit packing.
- Rust-only bounded item-container parsing and experimental direct HEVC native
  YUV decoding, now exposed through the C API with alpha attachment, rotations,
  mirroring, clean apertures, versioned options and profile conversion. Crop/scale also export C APIs.
  Grids, identity images, overlays and 8/16-bit raw masks use Rust composition and decoding;
  mask and derived decoding also work with all optional codec features disabled.
- Item-property enumeration, typed/raw/UUID queries, owned user descriptions,
  transforms, in-memory insertion and serialized-data deduplication.
- Context allocation and memory reads; shared image handles, primary/top-level IDs,
  HEVC/derived/mask descriptions, thumbnails, uncompressed metadata and color queries.
  Handles retain their images after context release/reload; alpha lookup follows
  the context's current image table, matching the pinned reference.

## Executed checks

| Check | Outcome | Scope/limits |
|---|---|---|
| Header inventory | 465 functions, 1 exported variable, 37 structs, 38 enums, 108 macros; hashes for 29 headers | Three unexported plugin convenience variables retained separately; C++ wrappers tracked by header hash |
| First-box brand parsing | 93,398 cases, 0 mismatches | 26 types and camera UUIDs; three deliberate defects rejected (64,902 / 1,090 / 9 mismatches); C-client ASan/UBSan also passes |
| Brand/version differential | 17,126 cases, 0 mismatches in this corpus | Original-header C clients in separate processes; header truncation, malformed lengths, extended sizes, duplicate brands, NULs, signatures, error messages, out-argument preservation |
| Image differential | 6,553 transcripts, 0 mismatches | Color/chroma combinations, dimensions, bit-depth boundaries, alignment/stride, zeroed storage, duplicate planes, pointer identity, flags; excludes transforms and resource budgets |
| Color/HDR differential | 198,932 transcripts, 0 mismatches | All uint16 NCLX setter inputs, all uint8 option-version pairs, all chromaticity coordinates, exact floating-point bits, ICC ownership, HDR boundaries and output sentinels; no image-handle APIs or color transforms |
| Context/handle differential | 1,786 transcripts, 0 mismatches | Copied/borrowed input, destroyed input copies, aliases, handles after free/reload, early and late read failures, primary/hidden/boundary IDs, metadata bytes/filters, thumbnails, alpha references, color profiles, rotations, every synthetic-file/property truncation; no C decoding |
| C client sanitizers | ASan/UBSan clients pass the context corpus | Libraries are not sanitizer-instrumented; local LeakSanitizer could not run under ptrace, so leak detection was explicitly disabled |
| ABI | Seventeen structs match original-header size, alignment and every field offset; struct-return and data-symbol clients pass | Linux x86_64 only |
| Mutation checks | Thirteen deliberately wrong implementations rejected | Wrong filetype enum, error code, plane samples, primary coordinate, primary item ID, alpha reload state, worker callbacks, warning text, mask samples, overlay alpha, decode-operation/total-memory budgets and ABI field order; isolated builds, successful baselines, compiler/crash failures do not count |
| HEVC configuration | 400 cases, 0 mismatches | SPS prefixes, coded-size limits, crop/depth/chroma bounds, sub-layer flags, empty/reordered/missing parameter sets and recovery after early slices |
| Auxiliary/depth APIs | 512 file transcripts, 0 mismatches; C-client ASan/UBSan pass | Twelve APIs; every auxiliary-box version, filters/counts/output sentinels, copied type strings, typed child errors, exact depth float bits, malformed SEI and handles after reload/free. Also passes with optional codecs disabled |
| Auxiliary/depth mutations | Both defects rejected | Wrong alpha-filter bit (4 mismatches) and exponent (148 mismatches), independently passing baselines; the default mutation set now contains sixteen defects |
| Item-property APIs | 960 transcripts, 0 mismatches; C-client ASan/UBSan pass | Thirteen APIs, item-local IDs/order, raw/UUID copies, typed/raw deduplication, descriptions after context release, all rotation/mirror bytes, crop fractions, missing boxes, duplicate/invalid associations, essential flags and failed reload state. Also passes with optional codecs disabled |
| Property decoding | 3,772 comparisons, 0 mismatches | 943 property fixtures across native, transformed, RGB and strict decode modes; optional warning text, item errors, essential properties, pixels and metadata are compared |
| Property mutations | Four additional defects rejected | Wrong item-local IDs (921 mismatches), raw-class selection (921), unterminated descriptions (21) and crop origins (15); independently passing baselines and isolated builds |
| Camera matrices | 1,641 transcripts, 0 mismatches; C-client ASan/UBSan pass | Six APIs; exact floating-point bits, fixed-point shifts, 16/32-bit quaternion fields, every version, malformed prefixes, legacy UUID forms, crop/mirror/rotation order, duplicate matrices, warnings, and owned objects after context/handle release. Also passes without codecs |
| Camera mutations | Two additional defects rejected | Incorrect focal scaling (365 mismatches) and quaternion normalization (165), with independent passing baselines; the default mutation set now contains 22 defects |
| Sensor metadata | 1,039 transcripts, 0 mismatches; C-client ASan/UBSan pass | Twenty-two APIs; exact float/NaN bits, copied arrays, nulls, output sentinels, component ID allocation and crop/scale propagation. Also passes without codecs; sensor file parsing and serialization remain open |
| Sensor mutations | Three additional defects rejected | polarization_match_order (704 mismatches), polarization_nan_bits (2 mismatches), component_id_sequence (704 mismatches); independent passing baselines, isolated builds; default set now contains 25 defects |
| Decoded-component APIs | 67,329 transcripts, 0 mismatches; C-client ASan/UBSan pass | Thirty-four APIs; every uint16 reference type, all 1–128-bit depths, datatype passthrough, duplicate channels, reference-only descriptions, pointer identity, typed strides, setter errors and crop/scale. Handle-side APIs and file/codec integration remain open |
| Component mutations | Three new defects and the relocated ID-sequence defect rejected | component_id_sequence (704 mismatches), component_reference_count (67328 mismatches), component_typed_stride (1674 mismatches), component_crop_datatype (1061 mismatches); independently passing baselines and isolated builds; default set now contains 28 defects |
| Handle component APIs | 791 transcripts, 0 mismatches; C-client ASan/UBSan pass | Five APIs, HEVC/JPEG descriptions, parse order, derived alpha depths, decoded/conversion IDs, output sentinels and reload/free lifetimes. Description-only mode also passes without codecs |
| Handle-component mutations | Four new defects rejected | component_grid_parse_order (7 mismatches), component_decode_ids (4 mismatches), component_alpha_depth (20 mismatches), jpeg_sof_boundary (1 mismatches); independent baselines and isolated builds; default mutation set now contains 32 defects |
| Error-buffer lifetimes | 144 cases, 0 mismatches; C-client ASan/UBSan pass | Unrelated handles/context errors, aliases and releases; the pre-fix candidate use-after-free was reproduced under ASan |
| Coded-size mutation | One additional defect rejected (138 mismatches) | Independent baseline passed; changed the tightened limit floor from 65,536 to 65,535; the full default mutation set now contains fourteen defects |
| HEVC native output | 5/5 fixtures match exactly | Y/Cb/Cr data, image IDs/order, dimensions, depths and strides; transformations disabled, native NCLX passthrough; **alpha not compared** |
| HEVC default output | 5/5 tested color-plane outputs match | Separate default-converted Rust and reference output; no longer compares native data to default output |
| C decoding | 115 cases, 0 mismatches | Five direct HEVC fixtures x 23 modes; native/default/planar/interleaved output including alpha, profiles, both 16-bit byte orders, sampling restrictions, callbacks and errors; not codec conformance |
| Geometry/monochrome HEVC | 688 cases, 0 mismatches | Ordered rotations/mirrors/clean apertures, truncated fractions and integer boundaries, conformance-window cropping and real monochrome alpha; includes four additional upstream images |
| Grid/identity decoding | 640 cases, 0 mismatches | Generated valid/malformed tile graphs, wide fields, MIAF, cycles, strict/permissive errors, missing configurations, warning contents, callback arguments and worker thread placement; unknown-decoder cases use one worker to fix error ordering |
| Overlay decoding | 824 cases, 0 mismatches | Backgrounds, signed 16/32-bit offsets, clipping quirks, all header truncations, input counts and invalid references |
| Raw masks | 1,246 cases, 0 mismatches | 8/16-bit sample bytes, all data prefixes, depths and configuration versions, missing properties, conversion paths; also passes without the HEVC feature |
| Adversarial graphs | 47 cases, 0 mismatches | Nested overlays and identities, MIAF constraints, shared-grid amplification limit, alpha blending and mixed-reference cycles |
| Derived/mask handles | 606 transcripts, 0 mismatches | The same 303 generated files, copied/borrowed input, old handles after partial reloads, cyclic/missing references and query error outputs; also passes C-client ASan/UBSan with local leak checking disabled |
| Warning/thread controls | 1,249 cases, 0 mismatches | Every declared error/suberror pair, ignored caller message, pagination and sentinels, crop/scale warning propagation and thread-setting boundaries; C clients also pass ASan/UBSan with local leak checking disabled |
| Security limits | 2,927 transcripts, 0 mismatches | All uint8 versions and old prefixes, aliases/direct mutation, context and parent limits, safe plane allocation, exact per-block/total errors, allocation release and mask/HEVC/derived decoding; C clients also pass ASan/UBSan with local leak checking disabled |
| Resource lifetimes | 276 transcripts, 0 mismatches | Repeated decodes, replaced decoder input extents, limits changed after decode, metadata, context reload and one/two surviving handles; unrelated images release their budget independently |
| Decoding options | 65,549 cases, 0 mismatches | All byte-valued version pairs, old short prefixes and alias copies; also passes C-client ASan/UBSan with local leak checks disabled |
| Crop/scale | 12,240 cases, 0 mismatches | Odd geometry, 8/10/12/16-bit layouts, alpha and metadata, invalid margins, nonstandard planes; wider/custom component formats remain open |
| Dependency guard | Pass | Two reviewed codec crates; no native build scripts or codec link dependencies in the resolved candidate graph |
| Rust checks | Unit tests, ABI test, formatting and Clippy pass | Linux, macOS and Windows Rust build jobs passed for the grid/warning iteration; full sanitizer, fuzzing and cross-platform ABI validation remain open |
| Completeness gate | **Fail**, as required | Missing API and unvalidated entries prevent a success claim |

The separately retained `known-differences.json` records the malformed non-ftyp
regression now fixed by structural first-box parsing. The expanded suite compares
93,398 inputs over 26 box types and camera UUID aliases, including nested optional
errors, physical-stream versus parent boundaries, count/version validation and
truncations. All match, including under C-client ASan/UBSan. Other box classes and
allocation-failure behavior remain open. The five HEIC fixtures are smoke coverage for direct items,
not HEVC conformance or full container compatibility. The resource-budget comparisons cover the implemented paths, but do not establish
complete safe allocation behavior across hostile codec inputs or unimplemented formats.

A previous Valgrind attempt could not execute the client in this environment
(permission denied). The context C clients now pass ASan/UBSan; the libraries
are not instrumented, and no full-library memory-safety or local leak-test pass
is claimed. The [grid/warning CI run](https://github.com/reaperhulk/clanker-experiments/actions/runs/35378061976)
passed Rust builds on Linux, macOS and Windows and all Linux development checks,
including the client sanitizer tests with leak checking and all nine mutations
present at that revision. Its only failing step was the full-completion gate.
The job summary and exact commit are in `results/ci-grid-report.json`; the report
precedes this overlay/mask/security iteration.

The context corpus is a finite tested subset, not full parser equivalence. It
includes five real HEIC fixtures and generated containers. All 38 added exports
remain partial. Unsupported image types, compressed metadata, remaining duplicate-box/property behavior,
complete clean-aperture coverage, budgets for remaining formats,
file/reader callbacks and complete decoding orchestration remain open. `context-report.json` records
exact binary, client and corpus hashes. `context-sanitized-report.json` records
the limited sanitizer scope; mutation evidence rejects wrong reload semantics.

The overlay implementation preserves the pinned reference's byte-based blending and
negative-offset behavior. These are observable compatibility quirks, not corrected
rendering rules. Conversion graph ordering also retains otherwise unreachable states
because they affect equal-cost path selection and therefore sample values.

The grid decoder uses scoped Rust workers to honor callback execution and error
semantics. This is compatibility work, not an optimization claim. In an independent
40-run probe per fixture, libheif returned both possible errors when an invalid
identity tile raced unavailable-decoder failures. Exact error-order tests therefore
use one worker; the other grid modes retain the default four workers. Broader
concurrent trace equivalence remains open.

Six added entry points expose versioned context limits and safe plane allocation.
The Rust accounting follows libheif allocation lifetimes, including retained decoder
input and metadata. A context reload releases unrelated images while existing
handles retain their own objects and auxiliary images. Limits for codecs/formats
not yet implemented, and complete allocator-failure behavior, remain open.

HEVC configuration geometry is now checked against tightened context limits before
reading media or entering the codec. Empty parameter-set NALs are ignored, PPSs
with unavailable sequence parameters are discarded on arrival, and slices without
parameters can be followed by a later complete frame. This is a bounded set of
malformed-stream checks, not full HEVC conformance or codec allocation accounting.

Errors returned through image handles now use the image's shared error buffer.
The independent sanitizer client reproduced a use-after-free before the fix and
passes after it. The earlier security CI (`15646d9`) passed all development checks,
including thirteen mutations and sanitizer clients with leak checking enabled,
and Linux/macOS/Windows Rust builds. Only its full completion gate fails. That CI
run predates these SPS and error-lifetime additions; their local evidence is separate.

Twelve auxiliary/depth entry points expose filters, type strings, depth handles and
HEVC depth-representation SEI metadata without requiring the HEVC codec feature.
The depth parser follows the pinned reference's one-message parsing and ignores
its declared NAL/payload lengths where upstream does. Unsupported SEI syntax is
not presented as fully supported. JPEG item handles can now be constructed for
auxiliary queries without a `jpgC` box, matching upstream; complete JPEG
bitstream descriptions and decoding remain open.

The following SPS/error-lifetime CI (`f843fe7`) also passed every development check,
including all fourteen then-registered mutations and leak-enabled C sanitizers.
Its only failure was the strict full-completion gate. Auxiliary/depth results
were collected locally after that CI and remain separately identified by hash.

## Initial performance evidence

11 alternating reference/candidate pairs, 3 measured iterations per process plus
one warmup. Input is libheif's example HEIC containing two 1280x854 images.
Memory parsing, all top-level native YUV decodes and teardown are timed; file I/O
and output serialization are excluded. Equivalent native output was checked first.

| Measurement | Reference | Candidate |
|---|---:|---:|
| Median per file | 672.46 ms | 739.43 ms |
| Min–max | 583.73–715.88 ms | 475.80–818.65 ms |
| Standard deviation | 45.49 ms | 111.15 ms |

The candidate median is 9.96% higher, with substantial overlap and variation.
This does **not** demonstrate a speed win, establish a stable slowdown estimate,
or meet the performance goal. Allocation and instruction-count evidence is absent.

An independently instrumented Rust codec run attributes most time to parsing,
intra prediction and inverse transforms. Its profiler reports 39.4 ms of overhead
in a 457.6 ms run, with especially material overhead in fine-grained stages;
these are directions for investigation, not native wall-clock speedup estimates.
Optimizing small wrapper copies is unlikely to solve the dominant costs.

Raw samples, exact binary/corpus hashes, environment details and the profiler
output are in [results](results/). The initial benchmark and profile used the
foundation source at commit `4210217dacc0dd411fc5e874d31579dea765432c`; they
precede the color/HDR storage APIs. No performance optimization is claimed yet.

The subsequent benchmark harness rebuilds the probe and timing client together
and requires fresh output equality, with source/binary/input hashes checked before
and after measurement. `benchmark-guard-smoke.json` records a three-pair, one-iteration
pipeline check (also slower in this run), not an optimization experiment or a
stable speed estimate.

## Next implementation work

1. Finish the parser and configurable resource-budget model; extend context/handle
   coverage to all item types, compressed metadata, callbacks, files, duplicate boxes,
   essential properties, clean apertures, allocation failures and thread behavior.
2. Implement exact color conversion, transforms and
   alpha/grid composition; extend codec fixtures and conformance profiles.
3. Implement remaining codecs/encoders and all advanced API families in PLAN.md.
4. Complete cross-platform ABI, fuzzing, memory-safety, downstream-client and
   full API gates before making a compatibility claim.
5. Only after full compatibility, profile and optimize with repeated output-checked
   A/B results across representative workloads before marking the PR ready.

The auxiliary/depth CI at `1742c94` passed all development steps, all sixteen
then-present mutations and Rust builds on Linux, macOS and Windows. Its sole
failing step is the strict completion gate (`results/ci-auxiliary-report.json`).

Item properties now distinguish the file's property table from retained image
handles: even an early failed reload clears the table, while later failures expose
the newly installed boxes. Malformed descriptions remain error properties and
produce decode warnings. Malformed transforms retain their property indices while
image interpretation reports their error. Unknown essential raw/UUID properties
are rejected, and duplicate associations preserve the first essential flag.
The independent tests also cover insertion into a fresh context; libheif rejects
insertion after reading a file, which this implementation preserves. Remaining
property classes, serialization and allocation-failure behavior remain open.

The item-property CI at `0e37368` passed every development check, all twenty
mutations and the three-platform Rust builds; only the strict full-API gate
failed (`results/ci-properties-report.json`). Camera matrix checks are recorded
separately. The camera implementation preserves the reference's absolute
intrinsic scaling and crop/mirror semantics, including legacy UUID properties.
The reference rejects all nonzero extrinsic-box versions; this implementation
preserves that behavior rather than interpreting the unreachable Euler branch.
Cross-platform floating-point conformance remains open.

The camera CI at `2240225` passed all development steps, all twenty-two mutations
and Rust builds on Linux, macOS and Windows. Only the strict full-API completion
gate failed (`results/ci-camera-report.json`). The sensor increment adds all
remaining exported names from `heif_properties.h`, but complete property behavior
still requires file parsing, codec integration and serialization. Sensor arrays
are owned; float samples preserve their exact bits, and first matching
polarization metadata wins, including an earlier wildcard entry. Cropping copies
metadata while scaling drops it, following the pinned reference. Existing image
and transform suites pass against the same sensor candidate (6,553 and 12,240
transcripts respectively).

The decoded-component increment adds independent descriptions and per-plane ID
lists, including components without data. The allocator and typed getters support
integer, floating-point and complex storage up to 128 bits; getters preserve the
reference's casts regardless of the declared datatype. Crop copies per-component
types and datatypes into new IDs; scale recreates ordinary unsigned planes.
Unknown channels cause the reference's explicit crop/scale error. The new suite
found and fixed that earlier transform gap and the missing-type sentinel.
Content-ID setter errors are tested; serialized content-ID behavior still needs
encoder integration. Unknown-ID scalar getters that dereference null upstream
are excluded from equivalence comparisons. Candidate getters return safe
sentinels for those calls. Existing image and transform regressions pass against
the same candidate (6,553 and 12,240 transcripts).

The component suite also passes against the codec-free binary.

The sensor CI at `8f07766` passed all development steps, all twenty-five mutations
and the three-platform Rust builds. Only the full-API gate failed
(`results/ci-sensor-report.json`).

The handle-component increment preserves description population during the first
item pass, before geometry and auxiliary links are installed. Grid descriptions
therefore depend on whether their coded descendant has already been initialized;
identity items initially have no descriptions. Alpha descriptions resolve the
coded descendant's bit depth, including derived alpha images. Decoded IDs follow
the reference's reconciliation before final color conversion, including its
shortcut when ID/channel lists already align. JPEG SOF description scanning is
implemented in Rust, with input-buffer accounting retained by the image, optional
jpgC prefixes, exact marker/precision/sampling behavior and strict bounds.
This is header parsing, not JPEG pixel decoding. Dynamic JPEG read-limit recovery,
remaining codecs and component content-ID serialization remain open.

The prior decoded-component CI at `e357110` passed all development steps, all
28 mutations and Linux/macOS/Windows Rust builds. Only the full-completion gate
failed (`results/ci-components-report.json`). Existing context (1,786), auxiliary
(512) and C decode (115) regressions pass against the new handle candidate.

The handle-component CI at `4c6e110` passed all development checks, all 32
mutations and the three-platform Rust builds. Only full completion failed
(`results/ci-component-handles-report.json`). The brand-box increment adds three
mutation checks for error classification, optional children and parent boundaries.
The default set now contains 35 defects. The original 17,126-case brand corpus
also passes against the same candidate.


## Generic items and compressed metadata

The item increment adds 23 partial exports: creation and queries for generic,
MIME, precompressed MIME and URI items; names, hidden flags, languages, payloads,
ordered references and release functions; and compression capability queries.
All names declared in `heif_items.h` are present. File writing, entity/track ID
namespaces, Brotli and broad malformed-container equivalence remain open.

The independent original-header client matches 2,840 cases, including all output
pointer combinations, signed lengths, missing IDs, failed-add ID consumption,
first duplicate table selection, extent boundaries, exact error text, versioned
item tables, owned values after context destruction and reloads. In addition to
text transcripts, the complete returned payload stream is compared byte for
byte (SHA-256 `4e4976cdaad546af32a2e4922d75d943322511b0b46bcf0a5a90c2eafc5c6de3`).
The compression corpus includes 530 default-encoder boundary/random cases and
malformed/truncated streams. Another 430 cases compare compressed image metadata
through retained image handles. Both suites pass C-client ASan/UBSan; local leak
checking is disabled because of ptrace, while CI enables it.

Pure Rust `zlib-rs` 0.6.8 supplies inflation with only `std`/`rust-allocator`
features. Its vendored compatibility patch preserves the original malformed-
stream diagnostic instead of overwriting it with a repeated-call message.
The default encoder is an in-tree Rust adaptation of zlib 1.3's level-6 matching
and Huffman algorithms, retaining the zlib license in `licenses/zlib.txt`.
Using the dependency's default encoder initially produced different compressed
bytes; the in-tree encoder matches the tested corpus, including window and
symbol-buffer boundaries. This is finite evidence, not exhaustive equivalence.
The dependency audit rejects native build scripts, native link declarations,
unreviewed crates and the C allocator feature.

The reference configuration explicitly requires zlib and disables Brotli to
make the existing oracle configuration reproducible across hosts. This does
not satisfy Brotli compatibility; it remains required work. The native zlib,
libheif and libde265 implementations are test oracles only.

Existing context (1,786), auxiliary (512), component-handle (791), security
(2,927) and derived-graph (47) cases also match this candidate. Seventeen public
struct layouts still match original headers on Linux x86_64. Reports are in
`docs/results/items-*` and `docs/results/metadata-compression-*`.

The preceding brand-box commit `1d38abc` passed every development CI step,
all 35 then-present mutations and Rust builds on Linux, macOS and Windows.
Only the intentionally strict full-API completion gate failed; see
`docs/results/ci-brand-boxes-report.json`.

Six new deliberate defects are rejected: failed-add ID reuse (15 cases), reversed
reference order (2,839), missing compression output on errors (169), altered
Huffman tree tie-breaking (338), overwritten inflate diagnostics (84) and
skipped compressed handle metadata (134). All baselines pass; compiler errors
and client crashes are not counted. The default mutation set now has 41 defects.
Both new suites also match in the build with every codec feature disabled.


## TAI timestamps and clock descriptions

The next increment adds all twelve declarations from `heif_tai_timestamps.h`,
remaining partial for the full contract. Both public struct layouts match the
original headers, bringing the ABI layout suite to nineteen structs.
The independent C client matches 67,745 cases: all 65,536 uint8 source/destination
version pairs; aliased copies and exact version-zero prefixes; raw flag values;
in-memory clock/timestamp properties and typed equality despite identical
serialized bytes; duplicate properties; every box version, prefix and flag byte;
owned copies after context/image destruction; image crop/scale; HEVC and mask
output; identity, grid, overlay and alpha propagation; and retained handles after
successful/failed reloads. The complete suite also passes C-client ASan/UBSan
(local leak checking disabled). The codec-free configuration is compared on the
67,728-case subset without HEVC inputs, with the excluded scope explicit in its
report. Nine real-HEVC variants and eight synthetic HEVC cases are excluded there.

A failed read clears file tables while preserving prior image objects. The
adapter now reports missing current-file `iloc`/`iref` when decoding retained
handles in that state. Broader interactions with partially installed new tables,
reader callbacks and old decoder caches remain required coverage.
Timestamp parsing is fatal on truncated boxes; stored values preserve arbitrary
C flag bytes, while file parsing converts the packed status bits to booleans.
Raw and typed properties retain their distinct lookup/equality behavior.

Existing item (2,840), property (960), context (1,786), error-lifetime (144) and
brand-box (93,398) comparisons also pass during this increment. Each report
records its exact candidate binary hash. Sequence timestamp APIs, serialization,
allocation failures, cross-platform ABI and full-library equivalence remain open.

Five new deliberate defects are rejected: version-copy gating (1,054 cases),
serialized-only TAI equality (799), clock bit shifts (534), missing decoded
timestamps (569) and stale file tables after reload (1,091). Passing baselines
and the exact mutant hashes are in `tai-mutations-report.json`. The default
mutation set now contains 46 defects.


## Metadata writer entry points

Five metadata insertion APIs add Exif, XMP, compressed XMP, generic fourcc and
URI metadata items, bringing the exported-function count to 251 (all partial).
The original-header client reuses the independent item snapshots and compares
2,187 cases, including complete byte-for-byte returned payload streams
(SHA-256 `b0e076c79432122bbeefb69d44416e8dbd39af17d3a3c5d1f13ee5d4f98a3c93`).
All cases also match with codec features disabled and in the C-client ASan/UBSan
run (local leak checking disabled; CI enables it).

The scope includes hidden items, type/content strings, Exif offsets and signature
scan boundaries, every compression branch, malformed generic types, output-ID
sentinels, foreign/retained image handles, failed reloads, `cdsc` targets, resource
limits, payload ownership after context destruction and existing handle metadata.
The implementation preserves the reference's distinct XMP DEFLATE behavior: that
entry point wraps data in a zlib stream while labeling the encoding `deflate`.
It also preserves acceptance of nonempty Exif data without a TIFF marker, the
ignored URI-type argument in the URI metadata writer, and unchanged handle
metadata snapshots after adding file-level metadata. Generic MIME insertion
retains its separate raw-DEFLATE behavior. These are observed compatibility
semantics, not recommended file conventions.

All declarations in `heif_metadata.h` now have exports; context file serialization,
allocation-failure equivalence, additional codec configurations and cross-platform
ABI remain required. Three mutation checks target Exif offsets, the XMP wrapper
and metadata reference targets. Reports are in `docs/results/add-metadata-*`.

The item/compression commit `aaf392b` passed all development CI steps, all 41
then-present mutations and Rust builds on Linux, macOS and Windows. Only the
strict full-API completion check failed; see `docs/results/ci-items-report.json`.

All three new defects were detected: shifted Exif offsets (452 cases), raw XMP
DEFLATE instead of the reference wrapper (42), and wrong reference targets
(1,910). The default mutation set now contains 49 defects.


## Text item APIs

Nine text APIs now have partial implementations. Independent original-header
clients match 1,309 cases with full file-item payload byte comparison, including
creation, optional returned handles, pending text payloads, image attachments,
MIME content of arbitrary bytes, embedded NULs, owned content and languages,
missing IDs, duplicate references, invalid targets, compressed/truncated content,
resource limits, all file prefixes, and unterminated item-info strings. The same
cases pass C-client ASan/UBSan (local leak checking disabled) and the codec-free
configuration. The payload-stream SHA-256 is
`693b0f5e160704b413e2b6c77294d7a53a4ea527eb223caddc2d02aae9bc53c7`.

Text objects and their registry survive successful and failed context reads in
the reference. Duplicate IDs resolve to the earliest retained text object;
changed-content reload cases exercise that behavior. Image handles retain their
own text ID lists. Content remains accessible after contexts and image handles
are released, while language lookup uses the current file's properties.
New text data and `text` references remain pending until serialization, so raw
item-data queries before writing correctly report missing data. The separate
image-container parser now matches the reference's empty/unterminated item-info
string handling, fixing the URI case found by this suite.

Four text/parser mutations are rejected: discarded retained text registry (736
mismatches), premature payload insertion (736), latest-ID lookup (11), and
unterminated-string handling (4). The last mutation initially survived; adding
independent handle-metadata string queries closed that test gap. The default
mutation set now contains 53 deliberate defects.

Existing context (1,786), item (2,840), compressed-metadata (430) and security
(2,927) cases pass when rerun for the changed parser and interpretation path. File
serialization, language insertion through a newly encoded image, callback
inputs, allocation failures and cross-platform ABI remain required work.

The preceding timestamp commit `7350505` passed all development CI steps,
46 mutations and Rust builds on Linux, macOS and Windows. Only the full-API
completion gate failed; see `docs/results/ci-tai-report.json`.

## Image extraction and extension

Three additional APIs implement area extraction, replicated physical padding and
zero-filled visible extension. Independent original-header clients match 3,365
cases and the complete pixel byte stream (SHA-256
`12e8f2d8da3518b181f6773d87cf2b56ea580f8360e829fa9a38bed03a99b8d8`).
The same cases pass C-client ASan/UBSan with local leak checking disabled.

Coverage includes odd subsampled offsets, clipped/extended areas, zero extents,
64-pixel allocation boundaries, repeated calls, partial mutation on failure,
duplicate channels, 1–128-bit typed/reference components, component ID sequences,
HDR/ICC/NCLX/TAI/aspect metadata, warning propagation, source release and
registered/parent/unregistered allocation limits. Native undefined shrinking-width
writes, invalid pointers and unbounded allocations are excluded.

The implementation preserves libheif's observable reallocation behavior:
padding leaves image dimensions unchanged but exposes larger plane dimensions
after reallocation, while component-description dimensions remain unchanged.
Zero extension synchronizes plane and component dimensions, and fills 8-bit
Cb/Cr bytes with 128. Extraction uses ceiling chroma coordinates and preserves
component IDs, including reference-only descriptions.

Five new mutations are rejected: floor chroma offsets (60 mismatches), hidden
reallocation dimensions (147), wrong neutral chroma bytes (102), reset component
IDs (1,021), and skipped extension allocation limits (32). The default suite
now contains 58 deliberate defects. The existing 12,240 crop/scale and 2,927
security cases also pass; all new exports remain partial.

The metadata commit `c038b1c` passed all development CI steps, 49 mutation checks,
and Rust builds on Linux, macOS and Windows. Only the strict full-API completion
gate failed; see `docs/results/ci-metadata-report.json`.

## Handle HDR metadata and pixel aspect

Thirteen more APIs cover handle CLLI, mastering display, ambient viewing,
diffuse white and pixel-aspect setters. All declarations in heif_color.h and
heif_image.h now have exports, still classified as partial.

Independent original-header clients match 716 cases and 7,160 successful
decodes. The runner requires ten successful reference decodes per case, so
malformed fixture setup cannot silently reduce testing to error comparisons.
The suite compares complete decoded pixel transcripts as well as every HDR
field, property IDs/types, absent-output sentinels, NULL output/input behavior,
caller-buffer copies, aliases, changed-content reloads and context release.
It includes every ndwt version, malformed box prefixes, optional warnings,
ignored trailing bytes, duplicate/zero values, random fields, native/RGB
conversion and identity/grid/overlay metadata inheritance.

These tests exposed two implementation bugs: typed-property deduplication
incorrectly included ignored trailing bytes, and derived images dropped
inherited HDR/aspect values. Both are fixed. Handle property objects remain
separate from the context's current file tables: a setter appends to the
retained image while first-property lookup remains stable; HDR/aspect setters
bypass the context's read-only restriction, matching the pinned reference.

C-client ASan/UBSan (local leak checking disabled) and the codec-free build pass.
Six mutations are
rejected: latest-property lookup (716 mismatches), read-only setter restriction
(716), zero CLLI insertion (231), retained trailing bytes (10), dropped inherited
CLLI (8), and accepted ndwt version 1 (1). The default mutation suite now has
64 deliberately wrong implementations. The 1,786 context, 960 property and 640
derived-decode regression cases also pass, along with 3,772 property-decode cases.
Whole-library instrumentation,
allocation-failure injection, encoding/serialization and platform ABI remain open.

Text commit c08972b passed all development CI steps, 53 mutation checks and
Rust builds on Linux, macOS and Windows. Only the full-API completion gate
failed; see docs/results/ci-text-report.json.

## Uncompressed configuration and component definition queries

Three component-definition APIs and pure Rust cmpd/uncC parsing now match 2,364
independent original-header cases, including all 19 predefined profiles,
explicit/synthetic component tables, unsigned type boundaries, raw URI bytes and
ownership after context/handle release. Configurations are attached to mask items
to test these properties independently of uncompressed-image initialization.
Pixel decoding, encoding and initialization of uncompressed items remain open.

The corpus covers every configuration version and every sampling, interleave,
block-size and flag byte; component formats, bit depths and alignments; truncated
headers and strings; component/tile limits; and pixel-size limits gated by the
security-structure version. The Rust limit snapshot now retains that version.
The same cases pass C-client ASan/UBSan (local leak checking disabled) and the
codec-free build. The first run found a truncated-table diagnostic difference;
the final diagnostic matches the reference.

Four new mutations are rejected: component-limit boundary (8 mismatches),
unterminated URI handling (27), ignored limit version (30), and reversed RGB
component definitions (3). The default set contains 68 deliberate defects.
Existing context (1,786), properties (960), security (2,927) and handle-component
(791) cases pass. All new exports remain partial.

Image-area commit c75ff63 passed all development CI steps, 58 mutation checks
and Rust builds on Linux, macOS and Windows. Only the full-API completion gate
failed; see docs/results/ci-area-report.json.
