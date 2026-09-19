# Current result: incomplete implementation, no demonstrated performance win

Reference: libheif 1.23.4, commit `4e14f5942c1732ace9611b9522cc991501445463`.
HEVC reference backend: libde265 1.0.16, commit
`7ba65889d3d6d8a0d99b5360b028243ba843be3a`. Candidate uses only Rust code;
the HEVC feature uses rusty_h265/rusty_h265-accel 0.6.0. The former is vendored
with VUI unspecified-color, monochrome-decoding and typed missing-parameter fixes; its Apache-2.0 notices are retained.

## Implemented scope

465 of 465 public functions are exported, plus `heif_error_success`.
No functions are missing. All exported functions are marked **partial**:
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


## Uncompressed pixels and retained component descriptions

All seven native decoder factory paths now have Rust implementations: byte-aligned
components, packed components, block components, pixels, block pixels, mixed chroma,
and rows (including tile-component ordering). Configuration validation, rejected
layouts, integer/float/complex data, native-endian output, padding, stable component
IDs and description queries are covered. RGB bit depths below eight use the
reference's replication rule when converted to eight bits. Identity images expose
coded RGBA alpha, and profiles which leave the preferred-chroma output untouched
preserve that behavior.

The independent original-header C client matches all 2,448 cases and 2,632
successful decodes. This includes all 79 upstream uncompressed fixtures, every
1–16-bit depth, 32/64/128-bit storage, both endian orders, component/pixel/row/tile
alignment, block padding/reversal, all minimized profiles, malformed sizes and
formats, color-conversion requests, derived items, and a second decode after
context release. Complete visible channel and component bytes are compared;
the transcript SHA-256 is
`e67280ffd6a60243bbf2ba442f02b876a0a0b9720383846ecb193f835b3825c0`.
Required success checks prevent a shared error path from passing as a decoder.

The same corpus passes with C-client ASan/UBSan (local leak checking disabled)
and with optional codecs disabled. The 1,786 context, 791 component-handle, 606
derived-handle and 115 HEVC decode regression cases also pass. Rust tests with all
features and no default features, original-header struct ABI checks, strict Clippy,
and the pure-Rust dependency audit pass. Native libraries are only test oracles.

This is finite coverage, not full libheif compatibility. Generic compressed image
units, sensor file properties, exact partial-range memory accounting, allocation
failure and concurrency coverage, uncompressed encoding, and additional codec
families remain open. Existing functions stay partial; there are 279 implemented
functions and 186 missing functions. No performance improvement is claimed.

Configuration commit 08d75cc passed all development CI steps, 68 mutation checks
and Rust builds on Linux, macOS and Windows. Handle-color commit b01cb84 likewise
passed its development checks and 64 mutations. Both failed only the full-API
completion gate; their job reports are retained in docs/results.

Six additional mutation checks reject wrong pixel values (1,130 mismatches), row
alignment (126), block extraction (134), component format (378), preferred-chroma
output writes (52), and sub-byte RGB replication (44). The default mutation suite
now contains 74 deliberate defects.

## Compressed uncompressed-image units

Pure Rust zlib/deflate decoding now supports full-item compressed data and icef
unit tables, including tile-local reads, implied offsets, all offset/length
widths, reordered and overlapping ranges, and malformed compression properties.
Uncompressed tile reads use iloc windows rather than eagerly reading the full
item. The independent original-header client matches 2,193 cases with 2,294
successful decodes, including multicomponent, planar and interleaved layouts.
The transcript SHA-256 is
`658e0f559eef1fd4a45b7afcd7497fd31c4077d8e7e0591c3729fdebba8c0bb3`.
The same corpus passes with optional codecs disabled and C-client ASan/UBSan.
Local LeakSanitizer fails under the traced execution environment; local reports
explicitly disable leak checking, while CI continues to request it.

Six new mutations are rejected: compression wrapper (552 mismatches), unit-type
boundary (236), tile index (100), implied offsets (50), overflow boundary (3),
and decompressed range origin (574). The default mutation suite has 80 defects.
The existing 2,448-case uncompressed pixel corpus passes unchanged, as do Rust
all-feature/no-default-feature tests and strict Clippy. No C dependency was added.

This extends existing exports, leaving 279 partial and 186 missing functions.
Brotli-enabled oracle/candidate support, exact cumulative resource accounting,
allocation failure injection and full-library instrumentation remain open.

## Sequence sample objects and image sample metadata

Sixteen additional exports implement owned raw sample data, duration and GIMI
content IDs, version-aware timestamp copying and image sample metadata. Buffer
and string setters copy caller memory; string getters return independently owned
copies; timestamp getters borrow sample-owned storage. Image metadata propagates
through crop, scale, conversion and canvas construction.

The original-header C client matches 713 cases: empty/filled/shrunk buffers,
all 256 timestamp version bytes, full-width duration values, all string lengths
through 255 bytes, NULL string clearing, timestamp replacement, one-byte
version-zero inputs and ownership after source/sample release. Crop/scale
propagation is independently exercised; canvas/track integration remains open.
Every operation expected to succeed is required to return success. The same
corpus passes C-client ASan/UBSan (local leak checking disabled) and codec-free
builds. Rust all-feature/no-default-feature tests, ABI checks and strict Clippy
pass. The inventory now has 295 partial functions and 170 missing functions.

Five initial mutations were rejected: payload bytes (688 mismatches), empty
buffer storage (688), duration (712), timestamp presence (713), and empty-string
semantics (713). An initial canvas-propagation mutation survived, exposing that
this corpus does not exercise canvas construction. The retained failed report
records that limitation. A separate crop-propagation mutation is rejected with
695 mismatches and replaces the unexercised canvas mutation in the default suite.
The default suite now contains 86 deliberate defects.

The pinned reference dereferences NULL timestamp inputs. They are excluded from
behavioral equality claims, rather than counted as matching errors. Candidate
NULL handling is defensive. Allocation-failure injection, track integration,
sequence file round trips and universal compatibility remain open.

## OMAF projection properties and descriptions

Four projection APIs and prfr parsing now match 1,678 independent original-header
cases and 16,780 successful decodes. Coverage includes all 256 projection bytes
and box versions, signed C enum boundaries, malformed/trailing data, flags,
property deduplication and ordering, repeated setters, shared handles, context
reload/free, derived images, decoded pixels and scale propagation.

Descriptions preserve arbitrary enum integers, while only values 0..31 append
prfr properties. Decoding uses the first retained property, which can differ from
the latest description value; clearing the description does not erase a property.
Initial file descriptions use the first valid property's masked five-bit value.
The independent corpus caught the missing initialization before the final pass.

The same corpus passes C-client ASan/UBSan (local leak checking disabled) and
codec-free builds. All five new mutations are rejected, bringing the default
suite to 91 deliberate defects. Context (1,786 cases) and property (960 cases)
regressions, Rust feature matrices and strict Clippy pass. This brings the
inventory to 299 partial functions, with 166 missing. Encoding/serialization
integration, whole-library instrumentation and allocation-failure coverage remain
open; the completion gate remains deliberately failing.

## Versioned encoding options and orientation composition

Ten additional exports implement allocation, copying and release of still-image,
sequence and uncompressed-image encoding options, plus EXIF orientation
composition. Copies read only the fields present in both caller versions, retain
the destination version, shallow-copy profile/parameter pointers and preserve the
reference's no-op behavior for unknown minimum versions.

The final original-header corpus matches 2,243 cases, including exact historical
allocation prefixes (one-byte option versions and four-byte unci versions),
all 256 byte versions, signed unci version boundaries, NULL sources, self-copy,
raw enum/flag values and both orders of orientation composition. C struct sizes,
alignment and every public field offset are independently checked against Rust.
The same final corpus passes C-client ASan/UBSan (local leak checking disabled)
and codec-free builds. Rust feature matrices and strict Clippy pass.

Four mutations are rejected: alpha default (2,242 mismatches), future-version
handling (1,539), version-copy boundary (1,861), and orientation order (258).
The default suite now contains 95 deliberate defects. The mutation corpus was
subsequently strengthened with unci-prefix and NULL-profile checks; the final
normal/sanitized/codec-free reports retain the newer client hash explicitly.

There are now 309 partial functions and 156 missing. The full completion gate
still fails with no inventory inconsistencies; encoding/track integration,
additional codecs, callbacks, plugins, region APIs and whole-library safety
validation remain open. No performance claim is made.

## Region objects, geometry, masks, and transforms (2026-09-19)

Thirty-six previously missing region functions are implemented, bringing the
inventory to 345 partial functions and 120 missing. No status is promoted to
validated by export presence or a finite corpus.

The independently compiled original-header client exercises every region API.
The 2,396 generated files/call sequences include 16/32-bit signed coordinates,
all version bytes, unknown geometry tags, every prefix of two mixed-geometry
payloads, polygon/polyline cardinality, all inline-mask coding bytes, resource
limits, ordered references, randomized geometry, rotations/mirrors/apertures,
zero and signed-overflow reference dimensions, exact hexadecimal floating-point
results, error strings/out-parameter sentinels, caller-buffer ownership, aliases,
context reload/free, mask expansion and referenced-image decoding. Full active
pixel bytes and guard bytes are compared, not hashes alone. Native undefined
behavior (including an uninitialized referenced-mask ID without a mask reference)
is excluded explicitly; defensive candidate behavior is not counted as parity.

The oracle exposed two implementation defects during development: reversed
mirror-axis meaning and use of a clamped clean aperture instead of the region
transform's unclamped aperture. Both are fixed and retained in the corpus.
The native transform's repeated x term and partial geometry retention on parser
failure are preserved as observable behavior.

Regular and codec-free reports each have zero mismatches. ASan/UBSan C clients
also pass; local LeakSanitizer remains disabled because this environment is
traced, while CI runs with leak checks. This is not whole-library sanitizer or
allocation-failure evidence. The existing 1,786-case context corpus and Rust/ABI
suites pass. The development inventory has no inconsistencies; the strict gate
still fails on remaining missing and unvalidated APIs. All evidence is in
`docs/results/regions-*.json`. Region serialization remains tied to the missing
context writer and must be validated when that path is implemented.

All eight region mutations were detected as transcript mismatches, without
counting compiler failures or crashes as detection. The default mutation suite
now contains 103 defects.

## Entity groups (2026-09-19)

Both entity-group APIs now implement filtered snapshots with independently owned
member arrays, exact group/member order, and stable lifetime after reload/free.
The 1,196-case original-header corpus exercises every declared group kind, all
version bytes for the three supported native classes, short full-box/scalar data,
all payload prefixes, count overflow, security limits, duplicate group boxes,
unknown children, empty results, caller mutation and disposal. Native parsing
ignores failed pyramid groups while treating alternative/stereo failures as fatal;
that distinction is retained. Unknown-only/filtered-out results preserve the
native allocated-empty versus absent/empty-group-list null distinction.

Normal, codec-free and ASan/UBSan C-client runs have zero mismatches; the same local
LeakSanitizer limitation applies. Public sizeof/alignof/offsetof checks pass. Five
semantic mutants are detected through transcript differences, bringing the default
mutation set to 108. The existing context corpus remains covered separately.
Reports and exact client/corpus/library hashes are in
`docs/results/entity-groups-*.json`. There are now 347 partial functions and 118
missing; the strict completion gate remains unsatisfied. Writer round trips,
allocation failures, whole-library instrumentation and cross-platform C behavior
are not established by this corpus.

## GIMI handle identifiers (2026-09-19)

Five previously absent handle APIs now expose mutable content-ID descriptions
and shared component-ID properties. Component mutations preserve property-object
identity across image aliases; property equality uses current serialized values.
Sample IDs retain full file bytes, including embedded NULs, while each returned C
string is independently owned. Descriptions changed in a read-only context are
distinct from retained file properties used when decoding.

The 496-case original-header corpus compares exact errors, output sentinels,
property lists, string presence/contents/ownership, shared images and independent
contexts, sparse component indices, all byte values, embedded NULs, every prefix
of a multi-string property, limits and overflow counts. It checks malformed
properties and failures after an earlier image was initialized, decoded/derived/
scaled content IDs, full active pixel bytes, and handles retained across reload
and context release. Unknown FourCC boxes cannot collide with internal UUID types.

Normal, codec-free and ASan/UBSan C-client runs match. Local leak checking remains
disabled under tracing; CI requests it. The 960-case property and 713-case sequence
sample corpora also match after the shared-property and sample-storage changes.
Rust feature builds, strict clippy and original-header layouts pass. Six new
semantic defects plus the existing empty-sample-ID mutation are checked; the
six new entries bring the default mutation suite to 114. Reports under
`docs/results/gimi-*.json` identify the exact binaries, client and corpus.

There are 352 partial functions and 111 missing. Encoding round trips, new-image
property insertion/deduplication, allocation failures, whole-library sanitizer
instrumentation and cross-platform C execution remain open. The full completion
gate remains unchanged and fails rather than treating symbol coverage as parity.

### Encoder parameter records

The independent original-header corpus covers 2,020 records and checks five parameter APIs, all 256 range flag values, signed count boundaries, all output-pointer masks, aliased output order, borrowed name/array identity, wrong-type errors and exact historical allocation prefixes. Normal, codec-free and ASan/UBSan client runs have zero mismatches. Five mutations are detected (raw range flags: 496 cases; empty arrays: 312; signed counts: 77; string arrays: 328; alias order: 3). The default suite now contains 119 deliberate defects. Plugin/parameter structure sizes, alignments, top-level offsets and nested parameter offsets match the untouched headers. Strict Clippy and both Rust feature configurations pass. Reports are in `docs/results/encoder-parameters-*.json`. Local leak checking remains disabled under the traced environment; CI requests it. Registry/encoding integration and non-Linux C execution remain open. The inventory now has 357 partial APIs and 108 missing; the strict gate still fails.

### Plugin registries and encoder facade

The 224-case independent original-header client exercises 39 APIs with exact callback traces. It covers accepted/rejected versions, exact old plugin and parameter allocations, priority ordering and ties, registration multiplicity, decoder descriptor identity, negative priorities/counts, name filters, explicit/implicit initialization, nested initialization counts, cleanup ordering with reentrant discovery, encoder allocation failures with and without retained state, parameter constraints across duplicate names, borrowed parameters, generic integer/boolean conversion, bounded output, callback errors and context-owned errors. Builtin uncompressed/mask configuration and retained uncompressed decoder descriptors are included. Normal, codec-free and ASan/UBSan client runs have zero mismatches, as do 1,786 context and 2,020 parameter regressions. Strict Clippy and both Rust feature/ABI suites pass. All twelve new behavioral mutations are detected; the default suite now contains 131 deliberate defects. Exact reports and hashes are in `docs/results/plugins-*.json`.

Registered-plugin image encoding/decoding, dynamic plugin loading, resource failures and cross-platform C execution remain open. Native HEVC plugin names are not imitated: the candidate identifies its Rust decoder as `rusty_h265`. This corpus checks external test plugins independently of bundled optional codecs and does not claim native codec equivalence. Full completion remains false: 396 partial APIs and 69 missing.

Region checkpoint CI (`386ed8ca16a0d4a443e960419d2b7ceaa713add7`, run `35421346438`) completed: all independent development suites, client sanitizers with leak checking, codec-free suites, all 103 then-current mutations, development inventory and Linux/macOS/Windows builds passed. Only strict full completion failed, as expected.

### File serialization and writer callbacks

Six APIs implement ordered BMFF metadata serialization, iloc/idat/mdat payloads, item information, properties/associations/references, brands, writer callbacks and file output. The independent 480-case original-header corpus compares every output byte, callback userdata and exact errors, callback reentrancy, old writer prefixes, repeated writes, read-back outcomes, UUIDs, 127/128 property boundaries, and native file-open failure behavior. Normal, codec-free and ASan/UBSan clients match. Local LeakSanitizer remains disabled under tracing; CI requests it.

All six mutations are detected by transcript differences: mdat base (198 cases), duplicate brands (265), UUID bytes (18), property index width (3), callback userdata (265), and success message (50). An initial userdata mutant crashed the internal file callback before producing a comparison report and was correctly rejected as undetected. Guarding that internal callback against null userdata lets the suite detect the semantic difference without counting a crash as evidence. The default mutation suite now contains 137 defects.

The writer guard is the only subsequent change after 11,478 item/region/property/metadata/text/context regression cases passed. Both feature builds, original-header ABI and strict Clippy passed before that guard; the mutation baseline rechecked ABI afterward. Exact tested binary hashes are retained per report under `docs/results/writing-*.json`.

This is metadata writer coverage, not complete image/sequence writing. New-image encoding integration, region/text round trips, actual compact image output, offsets beyond 32 bits, ID namespace switching, allocation failures, whole-library instrumentation and non-Linux C execution remain open. There are 402 partial APIs and 63 missing; the strict completion gate remains unchanged.

## Encoding checkpoint (407 partial APIs, 58 missing)

Mask and uncompressed still-image encoding, primary changes, thumbnails and
overlays now have real Rust paths. The original-header 1,292-case corpus checks
exact container bytes, every active decoded channel byte, planar/packed formats,
byte order and bit packing, zlib/deflate, historical options, ICC/NCLX/HDR/GIMI,
orientations, output sentinels, primary flags and repeated region/text writes.
Regular, codec-free and C-client ASan/UBSan runs have zero mismatches. Local leak
checking remains disabled because LeakSanitizer cannot operate under tracing;
CI runs the sanitizer client with leak checking enabled. Both Rust configurations
build; the default Rust unit and original-header ABI tests pass.

The two native failures are isolated in `tests/encoding_oracle_failures.c`:
fresh uncompressed alpha queries dereference uninitialized decoder state;
encoded TAI timestamp ownership triggers a double free. Full diagnostics and
candidate safety outcomes are in the encoding reports. Neither crash contributes
a parity match. Source and corpus hashes are recorded; the recovered corpus is
byte-identical to the pre-recovery corpus, and all reports were regenerated
against a newly built pinned oracle in the active workspace.

Seven related regressions pass: context 1,786; writing 480; items 2,840;
uncompressed pixels 2,448; derived 640; overlay 824; mask 1,246 (10,264 total).
External codec-plugin encode/decode integration, additional codecs/encoders,
custom/sensor encoder configurations, large/resource-failure cases and non-Linux
C validation remain open. No full compatibility or performance claim is made.

All ten encoding mutations are detected by transcript mismatches (not crashes or compilation failures): encode_mask_stride 350, encode_orientation 115, encode_primary_flag 24, encode_profile_fallback 16, encode_unc_component_endian 230, encode_unc_compression_flag 280, encode_thumbnail_noop 12, encode_thumbnail_direction 14, encode_overlay_background 8, encode_repeated_extent 24. The default mutation suite contains 147 defects.


## Tiling checkpoint (414 partial APIs, 51 missing)

Seven APIs now implement tile geometry and ID lookup, direct tile decoding,
incremental/full grid encoding and tiled uncompressed image construction. The
original-header geometry/decode corpus has 11,602 cases, including 6,579 compressed
unit tile reads; the construction corpus has 966 cases. Both match in regular,
codec-free and C-client ASan/UBSan configurations. The tests compare ordered
transforms, coordinates, output sentinels, resource limits, tile pixels, exact
serialized bytes, repeated/omitted/replaced tiles, packed depths, profiles and
zlib/deflate units. Mixed-depth grids preserve the reference's ignored copy error;
the oracle uses one worker to make canvas-depth selection deterministic.

All twelve new mutations are detected by semantic mismatches. The old
`item_failed_add_id` mutation was repaired after writer CI exposed its stale field
reference; it now compiles and produces 15 mismatches. Compiler errors and crashes
still do not count as mutation detection. The default suite contains 159 defects.
Eight regression suites pass 11,273 cases: encoding, writing, uncompressed pixels
and units, derived images, masks, adversarial graphs and security limits. Both Rust feature
configurations pass unit/ABI tests and strict Clippy. Exact binaries and corpus
hashes are retained in `docs/results/tiling-*.json` and `tile-encoding-*.json`.

Writer CI run 35424658609 passed regular, C-client sanitizer/leak, codec-free and
platform build checks, then failed on the stale mutation compilation. Development
inventory and strict completion were skipped. The workflow now includes hidden
`.build` reports and mutation logs in its artifact upload. Later CI is tracked
separately. Local leak checking remains disabled under tracing, while CI requests
it. Whole-library instrumentation, full codecs/plugins, sequence tracks, streaming
input, custom/sensor formats, allocation failures, platform ABI and fuzzing remain
open. The strict completion gate remains unchanged and fails.

## Sequence checkpoint (457 partial APIs, 8 missing)

The 43 remaining track APIs now have Rust implementations: owned/versioned
options, track references, raw metadata samples, TAI/GIMI auxiliary streams,
uncompressed visual sequences, timing/repetition, movie parsing and serialization.
Mixed still-image/sequence files preserve box/brand order and sample offsets.
Independent original-header clients compare 892 construction/write/readback cases
and 212 native-file reading/malformed-table cases. All pass in normal,
ASan/UBSan-client and codec-free runs. Local leak detection is disabled because
of the execution environment; CI requests it. Libraries are not instrumented.
Seven affected regression suites pass 19,766 cases (context, writing, encoding,
raw samples, tiling, tile encoding and security).

Twelve new mutations cover defaults, fresh handler state, reference ordering,
durations, mandatory timestamps, indefinite repetition, lazy sample offsets,
clock copies, decoder duration indexing, output sentinels, coding constraints
and movie box validation. Each produces semantic mismatches with a passing
baseline. The relocated writer-offset mutation is also rerun. The default
mutation inventory is 171; compilation and process failures do not count.

The oracle has an uninitialized two-byte `urim` data-reference-index field.
The client identifies those bytes structurally and excludes only those bytes;
they are not parity evidence. All defined bytes, including native malformed
output after duplicate track references, remain compared. Native reader fixtures
set the two indeterminate bytes to zero and document their provenance.

This remains partial: external codec sequence encoding, inter-frame sequence
decoding, alpha tracks, all sample-description/edit-list variants, reader callback
I/O and complete resource accounting need further implementation and testing.
The strict completion gate remains false. Eight unexported APIs are file/reader
input, debug dumping and dynamic plugin management. Evidence is in
`docs/results/sequences-*-report.json` and `sequence-reading-*-report.json`.

## Dynamic module checkpoint (462 partial APIs, 3 missing)

Five dynamic plugin APIs now manage OS modules and owned directory arrays.
A separate, untouched libheif oracle enables plugin loading with an empty default
search path. Its SHA-256 is `d9441cc137717ab2edbe93f1179ed5e7cbbe60bf30d8681d5b3c83f734afc077`.
The primary codec oracle remains unchanged. Test modules are caller-supplied C
fixtures; they are not candidate dependencies or bundled codec implementations.

All 425 original-header cases pass normal, ASan/UBSan-client and codec-free runs:
environment path splitting/ownership, malformed or missing modules/symbols,
stable diagnostics, plugin/library version checks, duplicate loads, partial
unload/reload identity, directory scan capacities, output sentinels, automatic
initialization errors and registry cleanup callbacks. Five affected suites pass
6,214 regressions. Rust tests (both feature sets), original-header ABI and strict
Clippy pass. Local leak detection remains disabled; CI requests it.

Upstream closes modules before dereferencing their info records and does not
remove unloaded decoder registry entries. Oracle clients pin modules so those
undefined accesses cannot invalidate comparisons. Two separate unpinned
candidate probes pass; they are not parity cases. Candidate decoder module
storage remains alive until registry teardown, and encoder cleanup precedes
closing its last module reference. The mutation runner now explicitly refuses
to count mixed process failures and semantic mismatches as detection.

Eight new semantic mutations cover directory splitting, accepted versions,
load reference counts, reload identity, directory capacity/count/termination
and cleanup ordering. The inventory is 179 mutations. Remaining gaps include
concurrent/reentrant module transactions, platform loader behavior, exhaustive
allocation failures and codec plugin integration. Three exports remain missing:
file input, reader callbacks and debug dumping. Strict completion remains false.


### Reader input and expanded sequence truncations

The file-input checkpoint `e57bfbb` passed all development CI gates, including
all 184 mutations, original-header file/sequence/sparse clients, ASan/UBSan with
leak checking, codec-free runs and Linux/macOS/Windows Rust builds. Only the
strict full-completion gate failed. Extracted CI evidence, including binary
hashes and all nine file reports, is in `docs/results/file-input-ci-e57.json`.

Reader callbacks now retain caller-owned tables/userdata for payload reads,
respect historical table prefixes, handle range results and owned error messages,
and distinguish inline data waits from file-extent range requests. All 4,712
original-header cases pass normal, client ASan/UBSan and codec-free builds.
Five reader mutations are detected by semantic differences. The read-call
schedule is not yet compared; reentrancy, growing input, precise allocation
budgets and complete sequence callback behavior remain required.

Every truncation of the three independent sequence fixtures is now exercised:
2,117 cases per input path, matching in memory and from files under normal,
client ASan/UBSan and codec-free builds. This fixed truncated movie suberrors
and uncompressed frame advancement/terminal errors after failed reads. Two
additional mutations are detected (1,659 and 44 differences); the default
mutation inventory is now 191. Construction (892), context/handles (1,786),
file input (929), and sparse-file (3) regressions pass. Exact hashes and reports
are retained in `docs/results/reader-*.json`.

Local leak checking still fails because LeakSanitizer cannot run under ptrace;
local sanitizer reports explicitly disable it. CI keeps leak checking enabled.
Current function coverage: 464 partial, one missing (debug dump). Full behavior,
remaining codecs, ABI/platform and all acceptance requirements remain open.
Symbol presence is not full compatibility; the strict gate remains unchanged.


### Debug dump export and complete symbol inventory

All 465 public functions and the exported error constant now have Rust symbols.
Every function remains partial: this is an inventory milestone, not the full
compatibility gate. The strict gate still fails with zero missing/unexpected
symbols and all behavioral/platform completion requirements retained.

Debug dumping now emits exact original-header diagnostics for fundamental BMFF
boxes, HEVC configuration, item locations/references, inline data, color/geometry
properties and optional-property parse errors. The 901-case corpus exercises
memory and file inputs, fresh and failed contexts, repeated dumps, invalid/null
calls and borrowed file-descriptor ownership. Normal, client ASan/UBSan and
codec-free comparisons pass. All five semantic mutations are detected (452,
452, 5, 901 and 452 transcript differences); default mutation inventory: 196.
Context (1,786), reader (4,712) and file-input (929) regressions pass. Original
header inventory, dependency audit, Rust/ABI tests and Clippy pass. Reports with
client/corpus/binary hashes are in `docs/results/debug-*.json`.

Owned writer models, sequence and richer-property dumps, derived-item payload
descriptions, deep nesting and platform C execution remain open for this export.
Reader CI at d76f48b has passed all platform Rust builds and new reader checks
including leak detection; its complete mutation run is tracked separately.
Compatibility implementation continues through these gaps and the remaining
codecs, callback/resource behavior, platform ABI and downstream acceptance gates.


### Mutable writer and derived-item diagnostics

Writer-created box dumps now preserve native zero-size headers, raw/UUID property
visibility, duplicate references and item base offsets after repeated writes.
Grid and overlay dumps include dimensions, backgrounds and all image offsets;
valid and truncated narrow/wide payloads have independent oracle coverage.
The 480 writer cases and expanded 1,729 input cases pass normal, ASan/UBSan client
and codec-free comparisons. The four new mutations are detected with 198, 22,
12 and six semantic differences, without process failures. Default inventory:
200 mutations. Writer (480), encoding (1,292) and sequence construction (892)
regressions pass, as do Rust tests, original-header ABI, formatting and Clippy.
Reports with binary/client/corpus hashes are in `docs/results/debug-writer-*.json`
and `docs/results/debug-derived-*.json`.

The mutation runner now isolates its baseline ABI Cargo target directory to avoid
feature-build artifact collisions; it still requires a successful baseline and
rejects compilation/process failures as mutation detections. Local leak checking
remains disabled only because of the ptrace limitation; CI retains it. Sequence
and richer-property diagnostics, loaded-model edits, deep nesting and platform C
execution remain open. All 465 function records remain partial, and strict
completion remains false. Continue codec/plugin integration and remaining plan
gates; these diagnostic checks do not establish whole-library compatibility.


### Registered still-image decoder integration

The C adapter now selects and retains registered decoder records, invokes their
allocation/push/flush/poll/free callbacks, and transfers returned image ownership
into the Rust transform and derived-image pipeline. The safe Rust core accepts a
decoder provider without depending on foreign ABI records. This also implements
AV1 handle initialization (bit depths, chroma, colorspace and missing/configuration
errors) and exact HEVC/AV1 configuration-byte upload. External plugin hooks are
optional caller interfaces, not bundled native codec dependencies.

All 616 original-header cases pass exact callback/pixel comparisons in normal,
ASan/UBSan client and codec-free builds. Cases cover exact historical allocations,
versions 1–6, selected-record reuse after decoder-ID changes, callback error
prefix handling, ignored flush failures, 50-poll exhaustion, raw strict/thread
options, context release before decoding, all AV1 configuration flags, empty
HEVC NALs, rotations, grids, identity and overlay composition. Eight new mutations
are detected (507, 16, 6, 507, 30, 539, 8 and 128 differences); default inventory:
208. Existing context (1,786), registry (224), decode (115), derived (640) and
HEVC limit (400) cases pass. Formatting, Clippy, Rust tests, original-header ABI,
dependency audit and the development inventory gate pass. Reports are retained
in `docs/results/plugin-*.json`; local sanitizer runs explicitly disable leak
checking under ptrace, while CI retains it.

Registered sequence decoders and encoders, other compressed-codec configuration
paths, all native codec replacements, exact non-UTF8 diagnostics, every resource
failure and concurrent unload/platform C behavior remain open. No function has
been promoted from partial: all 465 exports exist, and the strict full-completion
gate remains false. Continue with built-in pure Rust AV1 and remaining plan gates.


### Registered AV1 encoding and compact output

Registered AV1 encoders now receive converted pixels and versioned input queries,
legacy packet polling, encoded-size queries and normal/alpha/thumbnail input
classes. Alpha encoding allocates a separate encoder and copies dedicated and
generic parameters using historical ABI prefixes. Callback failures preserve
native partial context state. AV1 sequence headers populate av1C configuration;
padded coded images emit native clean-aperture fractions and dimension errors.
The codec-independent core now writes eligible minimized containers, including
alpha configuration inheritance, ICC, Exif/XMP, HDR, orientation and size-width
boundaries, and retains native ordinary-file fallback and repeated-write brands.
These optional caller-supplied codec hooks do not introduce bundled C codecs.

The two 441-case original-header suites pass exact callback, pixel and file-byte
comparisons in normal, ASan/UBSan client and codec-free builds. All 14 new mutations
are detected without process failures (240 default mutations). Existing encoding
(1,292), writing/diagnostics (480 each), tiled encoding (966) and sequence (892)
cases pass. Rust all-feature/no-feature tests, Clippy, formatting, ABI baseline,
dependency audit and development inventory checks pass. Exact hashes and results
are retained in docs/results/plugin-encoding-*, mini-encoding-* and encoder-*.
Local leak checking remains disabled under ptrace; CI retains it. Completed CI
at c9a2da7 passed its development checks, leak checks, all 212 executed mutations
and platform Rust builds; only the deliberately strict completion gate failed.

All 465 function records remain partial and strict completion remains false.
Built-in AV1 encoding, other registered codec encoders, sequence codec integration,
remaining codec replacements, exhaustive behavior, platform C execution and
external downstream/performance gates remain open. Compact output's HEVC and
post-write diagnostics paths still need their own end-to-end oracle coverage.
Continue through those gaps; finite corpus parity is not full compatibility.


### Registered HEVC encoding

Registered HEVC encoder packets now populate hvcC VPS/SPS/PPS arrays, deduplicate
native equivalent NAL prefixes, and length-prefix image packets. SPS fields supply
profile, chroma, bit depths and post-conformance-window dimensions; encoder-size
callbacks, alpha-specific URNs, clean apertures and heic/heix profile brands are
preserved. The compact writer now has independent HEVC end-to-end coverage.
All 762 ordinary and 762 compact cases pass exact original-header callback/pixel/
file comparisons in normal, ASan/UBSan client and codec-free builds. Seven new
mutations are detected by 8, 708, 24, 706, 51, 660 and 708 semantic differences,
without process failures (247 default mutations). AV1 encoder/compact (441 each),
HEVC size limits (400) and ordinary writer (480) regressions pass. Rust tests,
formatting, Clippy, ABI baseline, dependency and development inventory gates pass.

A workspace reset interrupted the first mutation run after five detections; that
incomplete evidence is retained separately. Restored sources/client/corpora rebuilt
to the same binary and corpus hashes, and every new suite and all seven mutations
were rerun successfully. Reports are in docs/results/hevc-encoding-*,
hevc-mini-encoding-*, hevc-encoder-* and hevc-recovery-*. Local leak checking is
disabled under ptrace; CI retains it. Built-in HEVC encoding is still open, along
with oversized/malformed encoder packets, broader compact post-write diagnostics,
other codecs, sequences and remaining platform/downstream gates. All 465 functions
remain partial and strict completion remains false; implementation continues.
