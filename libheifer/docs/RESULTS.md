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


### Registered JPEG-family encoding

Registered JPEG, JPEG2000 and HTJ2K encoders now feed the owned still-image,
alpha and thumbnail paths. JPEG forces the native output color profile before
conversion and property insertion. JPEG2000 emits ordered channel-definition
children, retains separate j2kH container properties for separate images, and
unpacks callback error prefixes. Ordinary output uses jpeg/j2ki brands; compact
requests preserve native ordinary-container fallback for these codecs.

All 2,382 original-header callback/pixel/exact-file cases pass normal, ASan/UBSan
client and codec-free checks. Six new mutations are detected without process
failures (253 default mutations). HEVC ordinary/compact (762 each), AV1 ordinary/
compact (441 each), properties, existing encoding and writer regressions pass.
Rust tests, Clippy, formatting, original-header ABI baseline and the development
inventory gate pass. Reports and exact build/client/corpus hashes are retained in
docs/results/other-*. Sanitizer and codec-free binaries were tested in an isolated
worktree containing the same implementation; the main build independently passed
the mutation baseline and regressions. Local leak checks remain disabled under
ptrace; CI retains them. All 465 functions remain partial and strict completion
remains false. Built-in codec implementations, AVC/VVC hooks, sequence integration,
allocation/platform/downstream and broader behavioral gates remain open.


### Registered AVC encoding and configuration write failures

Registered AVC encoders now emit avcC SPS/PPS/extension arrays and length-prefixed
image packets, preserving native parameter multiplicity, profile fields, scaling
lists, interlaced geometry, cropping, alpha/thumbnail callbacks and avci brands.
Failed SPS parsing retains native partial configuration state. HEVC follow-up
SPS failures now preserve the corresponding initialized fields and dimensions.
Oversized configurations remain attached to the encoded item. The pinned native
writer stops the enclosing property containers at a failed child, emits its
zero-header partial prefix, and still reports successful file output; the owned
writer reproduces those exact bytes and repeated-write behavior.

All 1,344 AVC and 789 HEVC ordinary/compact cases pass normal, ASan/UBSan client
and codec-free comparisons. All eight new mutations are detected by 242, 48, 72,
30, 1,233, one, 21 and one semantic differences, without process failures (261
default mutations). JPEG-family (2,382), AV1 ordinary/compact (441 each), encoding
(1,292), writing/diagnostics (480 each), properties (960), GIMI (496) and TAI
(67,745) regressions pass. Rust all-feature/no-feature tests, formatting, Clippy,
original-header ABI baseline and development inventory checks pass. Reports and
exact hashes are retained in docs/results/avc-*. Local leak checking remains
disabled under ptrace; CI retains it.

The HEVC malformed-follow-up corpus first initializes all configuration members
with a valid SPS. Initial malformed HEVC configurations that expose indeterminate
native fields are not counted as parity evidence. Built-in AVC/HEVC encoders,
VVC integration, other codec replacements, sequences, broader malformed/resource
and platform/downstream gates remain open. All 465 functions remain partial and
strict completion remains false. Implementation continues through these gaps.


### Built-in Rust JPEG decoding

The optional `jpeg` feature now supplies built-in decoding through a reviewed
scalar Rust jpeg-decoder fork. Native full-resolution IDCT and chroma rounding,
small-component replication, truncated entropy recovery, progressive smoothing,
scan/marker error ordering, default Huffman tables and lossless grayscale
predictors/point transforms are implemented. Original-header comparisons include
2–8-bit lossless samples and native higher-precision/RGB rejection behavior.
Description parsing and decoding now replace one retained compressed-input
reservation instead of counting it twice. Native codec sources remain test-only.

All 7,325 decode, 7,158 complete-prefix/error and 2,297 allocation-limit cases pass
normal, ASan/UBSan client and codec-free comparisons. Ten new deliberate defects
are detected by 736, 128, 176, 16, 540, three, 15, 33, 15 and four semantic
differences, without compilation or process failures (271 default mutations).
Component handles (791), registered decoder callbacks (616), JPEG-family encoders
(2,382), AVC encoding (1,344) and HEVC ordinary/compact encoding (789 each) pass
regressions. Rust all-feature/no-feature tests, Clippy, formatting, original-header
ABI baseline, dependency guard and development coverage checks pass.

The native JPEG oracle was rebuilt with the committed pinned builder. Exact
binary/client/fixture hashes and all per-case results are retained under
`docs/results/jpeg-*`; full decode reports are compressed as `.json.gz`, with
plain JSON summaries recording both compressed and original report hashes.
Local leak checking is disabled under ptrace; CI retains it and now runs the
new normal, sanitizer, codec-free and mutation gates. Cross-platform C-client
coverage is not implied by these Linux results.

Arithmetic JPEG decoding, built-in JPEG and other compressed encoders, remaining
codec implementations, VVC hooks, sequence integration and the broader behavior,
platform/downstream/performance requirements remain open. All 465 functions
remain partial and strict completion remains false. Implementation continues.

## Registered VVC encoder callbacks

The in-tree Rust VVC configuration parser/writer and callback integration match
1,509 original-header cases in all three builds: normal, ASan/UBSan C clients,
and codec-free. Evidence is retained in `results/vvc-*-report.json`; shared AVC,
HEVC, JPEG-family, AV1 and writer regression reports also match. Local sanitizer
leak checks are disabled under ptrace; CI requests leak checking. These runs
instrument the C clients, not all Rust or native dependency internals.

The corpus compares callbacks, input pixels, output/error writes, retained
image dimensions, and repeated exact BMFF bytes. It covers historical plugin
prefixes, encoded-size callbacks, profiles/tiers/levels, sublayer order,
subprofiles, SPS crop/precision/partial records, missing parameter sets, every
NAL type, duplicates, alpha, metadata and 16-bit NAL count/length boundaries.
The native encoder deliberately ignores the SPS display size for VVC and uses
its versioned encoded-size callback; the candidate preserves that behavior.
The native writer indexes absent vectors for certain malformed multi-layer SPS
records; those undefined paths are excluded and are not claimed as matches.

Candidate: `cef2dadf7d4b5a6da045cf04285d13914af9e915970581f65d0926f27c6af28e`.
Codec-free: `39df1283790233a291aa04a6f08379c9a7f81dda17a2bd0ce841135b3d87ee42`.
Pinned libheif oracle: `5cc5713a0503098f20f8a3631e370d43256ebdca0a3209e97d541e6ecf1cd917`.
Corpus: `1fc85c4803b6b7f3c528df573541d9724cdf38061be31ebd8e2d9a2419fdb469`.
Original-header client: `0cf5bffff6f56a94ee9f567b857aadf419caf1411d23acbe36a201b6f1fc9a67`.

Rust tests (all features and no defaults), original-header ABI, Clippy, formatting,
header inventory, dependency audit and development coverage pass. Built-in VVC
coding, VVC reader/decoder configuration and sequence integration still require
implementation and evidence. No entry is promoted from partial; strict completion
remains false.

All seven new VVC mutations are detected by semantic differences, with successful
builds and completed client processes; the default suite now contains 278 defects.


### Built-in scalar Rust JPEG2000 decoding

The optional `jpeg2000` feature now supplies pure Rust decoding through a vendored
std-only hayro-jpeg2000 0.4.0 backend. Native irreversible wavelet normalization,
round-before-shift precision, signed samples, chroma sampling, zero-filled absent
tiles, SIZ descriptions, tile-header errors and allocation limits are implemented.
The default C adapter enables it; native OpenJPEG remains test-only.

A freshly rebuilt pinned OpenJPEG 2.5.4/libheif 1.23.4 oracle passes all 9,725
sample/conversion, 5,880 complete-prefix/marker, 1,327 retained-handle/description
and 2,297 allocation-limit cases. Each suite passes ordinary, ASan/UBSan-client
and separately built codec-free comparisons. All 389 owned successful fixtures
retain native encoder commands and hashes; 37 native generation failures are
recorded separately, excluded from parity counts. Shared JPEG (7,325), registered
decoder (616) and JPEG-family encoder (2,382) regressions pass, as do Rust
all-feature/no-feature tests, Clippy, formatting, ABI baseline, dependency and
development inventory checks. The resolved implementation graph has 35 packages;
the Linux library links only libc, libgcc_s and the loader.

Ten meaningful new mutations are detected through 7, 54, 383, 6, 320, 72, 90, 18,
8 and 1,118 semantic differences, with successful builds and completed processes
(288 default mutations). The initial end-marker predicate-only mutation survived
because the subsequent loop exit returned the same error; it is retained as
failed evidence. Its replacement incorrectly accepts the truncated marker and is
detected. No crash or compile failure counts as a detected behavioral defect.

Reports and exact hashes are under docs/results/jpeg2000-*. Large per-case JSON
reports are retained as reproducible gzip files, with compressed and uncompressed
hashes in adjacent summaries. Local leak checking remains disabled under ptrace;
CI retains it. This checkpoint does not claim whole-library instrumentation or
cross-platform C-client execution. Existing VVC-head CI has passed Rust builds on
Linux/macOS/Windows and new JPEG/VVC normal and sanitizer clients; its broader
Linux run was still in progress when checked.

Mixed tile transforms, component-count edge cases, richer JPEG2000 header
properties, tile-part progression, wider precision, HTJ2K, built-in encoders and
the remaining sequence/platform/downstream/performance gates remain open. New
follow-up corpora already reproduce mixed-tile and property differences. All 465
functions remain partial and strict completion remains false. Implementation
continues through those gaps.


### JPEG2000 per-tile and property follow-up

Per-tile wavelet and multi-component transforms now use each tile's coding
parameters and sample rectangle, including odd row tails and common component
subsampling. Raw codestream parsing preserves native component-count error
ordering. Oversized declared packet segments fail decoding while compatible
trailing packet-header padding remains accepted. Nested j2kH containers validate
cdef, cmap, pclr and j2kL, preserve native limit/error ordering, and produce exact
typed diagnostics. Child boxes cannot exceed their containing byte range.

Fresh independent comparisons pass 3,424 mixed-tile/component cases, 40 common
sampling cases, 1,589 property cases, 2,763 component/child-limit cases, and 1,572
exact diagnostics. Each suite passes normal, ASan/UBSan-client and codec-free
runs. The 9,725 sample/conversion and 5,880 marker regressions pass, together with
4,712 reader, 1,786 context and 1,729 ordinary-diagnostic cases. Rust all-feature
and no-default tests, Clippy, formatting, original-header ABI, inventory and
resolved dependency checks pass. All reported evidence was rebuilt after a
workspace reset; unpublished pre-reset results are not counted.

Ten new mutations detect 4, 2,968, 2,880, 1,728, 16, 24, 72, 12, 6 and 228 semantic
differences; the existing incomplete-end-marker mutation detects 18. The default
inventory is 298. The initial transform-tail execution was interrupted by a
PermissionError launching the native client and produced no report. It is not
counted as a detected defect or a semantic survivor; the isolated rerun completes
and detects 1,728 differences. Both attempts and the interrupted log are retained.

Exact per-case reports and binary/client/corpus hashes are under
`docs/results/jpeg2000-followup-*`. Local sanitizer clients disable leak checking
under ptrace; CI retains it. Whole-library instrumentation and complete platform
execution are not claimed. Eight additional owned native-generated codestreams
retain encoder commands, source revision and input/output/binary hashes.

Tile-part progression changes, wider precision, HTJ2K, built-in encoders, complete
sequence behavior and the remaining platform/downstream/performance gates stay
open. All 465 functions remain partial; strict completion remains false. The next
implementation is pure Rust AVC decoding with a pinned OpenH264 test oracle.

## Built-in Rust AVC decoding

Oracle: libheif 1.23.4 with its OpenH264 plugin and OpenH264 v2.6.0 (scalar).
Fixtures: 457 x264 (b35605ac, no assembly) streams in `tests/fixtures/avc-generated.json`.

| Suite | Cases | Mismatches | Report |
|---|---|---|---|
| test_avc (normal) | 11,625 | 0 | results/avc-decode-normal-report.json |
| test_avc (ASan/UBSan client, local leak check off under ptrace) | 11,625 | 0 | results/avc-decode-sanitized-report.json |
| test_avc (codec-free candidate vs HEVC-only oracle) | 11,625 | 0 | results/avc-decode-no-codecs-report.json |
| test_avc_errors (normal) | 37,047 | 0 | results/avc-decode-errors-normal-report.json |
| test_avc_errors (ASan/UBSan client) | 37,047 | 0 | results/avc-decode-errors-sanitized-report.json |
| test_avc_errors (codec-free) | 37,047 | 0 | results/avc-decode-errors-no-codecs-report.json |
| test_plugin_decoding vs AVC oracle | 616 | 0 | results/avc-decode-regression-plugin-decoding-report.json |
| test_plugins vs AVC oracle | 224 | 0 | results/avc-decode-regression-plugins-report.json |
| test_avc_plugins (registered vs built-in AVC decoder) | 480 | 0 | results/avc-decode-plugins-normal-report.json |
| test_avc_plugins (ASan/UBSan client) | 480 | 0 | results/avc-decode-plugins-sanitized-report.json |
| test_avc_plugins (codec-free vs HEVC-only oracle) | 480 | 0 | results/avc-decode-plugins-no-codecs-report.json |
| test_avc_limits (pixel/memory limits at read and decode) | 2,527 | 0 | results/avc-decode-limits-normal-report.json |
| test_avc_limits (ASan/UBSan client) | 2,527 | 0 | results/avc-decode-limits-sanitized-report.json |
| test_avc_limits (codec-free vs HEVC-only oracle) | 2,527 | 0 | results/avc-decode-limits-no-codecs-report.json |

Mutations: avc_mono_chroma (23), avc_level_prefix_limit (48), avc_profile_gate (24),
avc_decoder_error_text (192) and the replacement avc_mono_intra_cbp (70) are
detected by semantic differences without process failures. The initial
avc_mono_intra_cbp (codes 14/15) survived with a single monochrome fixture; both
reports are retained. The complete normal differential group passes unchanged.

### OpenH264 syntax layer and Rust SIMD

The malformed-stream corpus had 975 known differences (17,679 on its first run).
It now matches exactly. `src/avc_openh264.rs` models OpenH264's byte-level NAL
processing, bit reader, parameter-set, VUI/HRD and slice-header acceptance, and
access-unit construction. The vendored decoder follows OpenH264's reconstruction
and end-of-data rules. The corpus adds 1,152 SPS variants whose VUI HRD reaches
OpenH264's read-error-code loop.

x264's constant-QP mode codes I-frames about 3 below `--qp`, so the earlier QP
sweep topped out near QP 48. The corpus adds 24 exact (`--ipratio 1`) QP 49..51
streams, plus 8 QP 50/51 custom-matrix 4x4 streams.

Mutation evidence (`results/avc-decode-mutations-syntax-*`, 12 new mutations,
315 total): the first run detected 10 of 12. `avc_hrd_return_code` and
`avc_scaling_qp51` survived. A rerun of all 17 AVC mutations, with the HRD family
added, detected 16 (`avc_hrd_return_code`: 735 mismatches). The exact QP 51
streams then catch `avc_scaling_qp51` with 92 mismatches
(`results/avc-decode-mutations-qp51-replacement-report.json`). All three reports
are retained.

Deblocking now runs as fearless_simd kernels: Rust SIMD with runtime dispatch,
no `unsafe`, and no C or assembly. The vendored `simd_matches_scalar` test
checks them against the scalar filters on 20,000 randomized edges at the
detected and baseline levels; CI runs it on Linux, macOS and Windows. Every
suite above uses the SIMD build.

`tools/bench_avc.py` checks full decoded-plane digests on every sample. Medians
in ms (7x5, 1920x1080, AVX2 container) for OpenH264 asm / OpenH264 scalar /
libheifer:

| Stream | OpenH264 asm | OpenH264 scalar | libheifer |
|---|---|---|---|
| High CABAC QP 22 | 260.5 | 264.9 | 222.7 |
| High CABAC QP 32 | 23.3 | 35.9 | 19.2 |
| High CAVLC QP 22 | 68.5 | 75.6 | 61.3 |
| Baseline QP 27 | 17.8 | 30.9 | 11.9 |

Raw samples: `results/avc-decode-benchmark-openh264-{asm,scalar}.json`. These
are single-machine, single-image decode timings, not whole-library performance
claims. Multi-access-unit input, AVC sequences, encoding and the
remaining plan gates stay open. All 465 functions remain partial; strict
completion remains false.

### Registered AVC decoders against the built-in decoder

`tools/test_avc_plugins.py` registers a callback decoder for AVC at priorities
1, 69, 70, 71 and 777, around the built-in priority 70 that the OpenH264 plugin
and libheifer share. It covers plugin versions 3 and 5, selection by id, an
absent id, a missing `new_decoder` and failing pushes, on real, truncated and
garbage streams, with two decodes per handle. Its first run found 180
differences, in three behaviors:

- Ties at equal priority: libheif keeps plugins in a pointer-ordered set and
  takes the first, so a heap-allocated registered plugin beats a static
  built-in. The registry now orders decoders by record address, including its
  own static built-in records.
- Decoder reuse: libheif keeps the decoder an item first selected, including a
  built-in one and including after a failed decode. Later decodes ignore
  `decoder_id`.
- Pushed data: libheif prepends the avcC SPS/PPS units, and applies the
  avcC coded-size check, before handing data to an AVC plugin.

All 480 cases now match in normal, sanitizer-client and codec-free builds. Three
new mutations (`decoder_tie_order`, 48 mismatches; `builtin_decoder_cache`, 48;
`avc_plugin_headers`, 96) are detected, 318 in total. The normal CI group (88
suites) and the AVC-oracle plugin regressions still pass. Built-in AVC and
JPEG 2000 descriptors now have their own records; both previously fell through
to the HEVC record's name and id.

### AVC resource limits

`tools/test_avc_limits.py` sets `max_image_size_pixels`, `max_memory_block_size`
and `max_total_memory` before read or before decode. Values sit around each
stream's output size, macroblock-aligned coded size, libheif's ispe+16 padded
size, the 65536-pixel floor, the payload size and the plane sizes. Two new
272x256/270x250 fixtures exceed the floor. Every field and phase produces both
decodes and security-limit errors. The first run of the new `avc_ispe_padding`
mutation survived, because the padded limit only binds when the coded picture
exceeds the declared ispe. Items whose ispe is just below the coded size now
detect it (24 mismatches); both mutation reports are retained. All 2,527 cases
match in normal, sanitizer-client and codec-free builds; 319 mutations total.

### I_PCM macroblocks

x264 emits I_PCM under rate-distortion analysis at low QP with psy-RD off. There
are 28 new streams: all four profiles at QP 1, 6 and 10, with a noise pattern
and an alternating noisy/smooth macroblock pattern, plus cropped and
monochrome variants. They cover all-PCM pictures, CABAC re-initialization
after PCM, and intra prediction and deblocking next to PCM macroblocks. Every
CABAC and CAVLC colour stream matched on the first run. The monochrome CAVLC
stream differed in 23 cases. OpenH264 stores all 384 I_PCM bytes even without
chroma. Its chroma planes start at 128 and are output as reconstructed. It
skips the chroma mode check, so its DC chroma prediction assumes both
neighbours and reads 128 outside the picture. The vendored decoder now does
the same. All 11,625 valid-stream and 37,047 malformed-stream cases match.

The existing `avc_mono_chroma` mutation targeted the old flat-128 output, which
no longer exists. Moved to the initial plane fill, it survived: DC prediction
from the 128 border overwrites every predicted macroblock, so that mutant is
equivalent (`results/avc-decode-mutations-pcm-initial-report.json`). Anchored
on the border value, it is detected (1,702 mismatches). The new
`avc_mono_chroma_dc` mutation is detected (23); 320 mutations total.

### AVC image sequences

libheif decodes an `avc1` track by pushing samples into a stateful OpenH264
plugin decoder. Consecutive chunks share a decoder unless their sample
description index changes. The avcC parameter sets go only with global sample
0, so a later decoder never receives them. Frames are
polled before each push, and the decoder is flushed after the last sample.
Pictures leave OpenH264 through `ReorderPicturesInDisplay`. Baseline pictures
come out immediately. Streams without B slices come out one picture late, in
decode order. Other streams are released by POC once ready. `FlushFrame`
releases one picture per call. libheifer now emulates that loop with the
built-in decoder, including the "Did not decode all frames" error and
per-sample durations. Tracks whose AVC decoder is not built in fail decoder
selection as libheif's do.

`tools/test_avc_sequences.py` builds 194 sequence tracks from committed x264
streams (`tests/fixtures/avc-sequences.json`, 5 frames each at 64x48 and
50x36). They cover intra-only, IPPP (baseline, main, high, CAVLC, 3 refs,
weighted P), IBBP (main, pyramid, no weighted B, temporal direct, CAVLC) and
IDR every 2 frames, and pictures of 2 (CAVLC) or 3 (CABAC IBBP) slices. Each stream also appears truncated to 3 samples and with
its last slice byte cut, repeated by a 2.5-duration edit list, split into
two-sample chunks, and with those chunks alternating between two sample
descriptions. A static baseline stream has its P skip runs extended past the
picture end. The chunked tracks first differed on all 28 streams because
libheifer kept one decoder per chunk; it now follows libheif's sharing rule. The first full comparison differed on every B-frame
64x48 CABAC track. Two OpenH264 behaviours needed porting into the vendored
decoder:

- The CAVLC P and B slice end rule: the slice ends exactly at the stop bit,
  checked after skip runs too. A P skip run past the picture end fills it; a
  B run past the end is an error.
- `GetInterBPred` advances its frame destination pointer once per used list
  in 16x8 and 8x16 macroblocks. A Bi partition 0 therefore comes out list 1
  only, and a Bi partition 1 list 0 only. The displaced list-1 write lands in
  the next macroblock's area, which that macroblock rewrites. The upstream
  crate had removed this replication.

All 194 cases match in normal, sanitizer-client and codec-free builds.
Seven new mutations cover the reorder rules, parameter sets with sample 0,
decoder sharing across chunks, the Bi partitions and the P skip run. The first run of `avc_cavlc_p_skip_run`
survived because no stream ran past the picture end. The static overrun
tracks now detect it (2 mismatches). Both reports are retained
(`results/avc-sequences-mutations-initial-report.json`,
`results/avc-sequences-skip-run-mutation-report.json`); 327 mutations total.
Registered plugins decoding sequences are covered below.

### Registered decoder plugins in image sequences

libheif decodes every sequence track through a decoder plugin. When a
registered plugin wins selection for an `avc1`, `hvc1`, `hev1` or `av01`
track, libheifer now runs the same loop over it:

- The plugin is selected lazily, once per decoder, by its first push or poll.
  A plugin older than version 5 fails only that call. libheif keeps the
  selection, so later calls use the old API, and sample 0 counts as consumed.
- The plugin instance is created by the first push. An instance written by a
  failed allocation is kept, and later pushes go into it.
- `push_data2` carries the global sample index as user data. Configuration
  units go with sample 0 only. An empty sample fails after allocation.
- `flush_data` receives a NULL instance when no push ever created one.
- `decode_next_image2` user data selects the sample's auxiliary metadata.
- Sequence frames are not compared with the track dimensions.

`tests/plugin_sequences.c` registers a callback decoder that holds frames back
until the flush, fails any stage at a chosen call, and can return wrong user
data. `tools/test_plugin_sequences.py` drives it over AVC, HEVC and AV1
tracks: plain, short, repeating edit list, two-sample chunks, alternating
sample descriptions and an empty sample. It also covers plugin versions 1-6,
missing callbacks, selection by id and an absent id, output sizes and depths,
and AVC priorities around the built-in 70. The first run differed in 472 of
474 cases. The HEVC and AV1 priority cases were then dropped: the AVC oracle
build has no built-in HEVC or AV1 decoder, so they differed by build
configuration, not behaviour. All 466 cases match in normal, sanitizer-client
and codec-free builds. Five new mutations are detected; two plugin-decoding
anchors were made unique. 332 mutations total.

### Multi-picture AVC packets

An `avc1` item, or a sequence sample, may hold several access units. The
OpenH264 plugin passes the whole payload to one `DecodeFrameNoDelay` call.
Its data half decodes every NAL unit except the last one in the buffer,
which waits for the call's flush half. `ReorderPicturesInDisplay` runs once
per half, not once per picture. It sees the last picture completed in that
half, with the slice header decoded last. When the final picture has several
slices, its first slices were decoded in the data half, so the earlier
picture is filed under the final picture's POC and slice type. Pictures
completed earlier in the data half never reach the output list. The flush
half resets the output before completing the final picture. An instrumented
scratch build of OpenH264 confirmed which pictures are buffered; it is never
used as the oracle.

`tools/test_avc.py` now also decodes every sequence stream as one item, and
as an item with only its first two pictures (56 files, 25 modes). The first
run differed on 1,173 cases, where libheifer returned the last picture. It
then differed on 138 multi-slice and B-frame cases under a per-picture
reorder model, before the model above. All 13,225 still-image cases match in
normal, sanitizer-client and codec-free builds. The malformed-stream,
sequence, plugin, limits and plugin-sequence suites still match. Two new
mutations are detected (23 and 115 mismatches); 334 mutations total.

### Arithmetic-coded JPEG

The vendored jpeg-decoder now decodes arithmetic-coded sequential (SOF9) and
progressive (SOF10) JPEG. `vendor/jpeg-decoder/src/arithmetic.rs` ports
libjpeg-turbo's jdarith.c and jaricom.c:

- the QM coder, and DC/AC statistics with DAC conditioning (L, U, K);
- sequential blocks, and DC/AC first and refinement scans;
- per-scan and per-restart statistics resets;
- zero data after a marker inside a scan;
- the code-error state, which stops a scan (DC refinement ignores it, as in
  libjpeg);
- `jpeg_resync_to_restart` at restart points.

Arithmetic lossless (SOF11) stays rejected, as libjpeg-turbo has no decoder
for it. Marker handling now follows libjpeg's `read_markers`, in both
libheifer's header model and the decoder:

- reserved and hierarchical markers are "Unsupported marker type 0x..";
- differential SOF types and JPG are "Unsupported JPEG process";
- a second supported SOF is "two SOF markers";
- RSTn and TEM are skipped anywhere;
- DRI must have length 4, and DAC is parsed as `get_dac`.

`tools/generate_jpeg_fixtures.py` adds 156 `cjpeg -arithmetic` streams: four
sizes, five samplings, three qualities, sequential and progressive, plus 1B and
2-row restart intervals. DAC variants are spliced before every scan header: the
defaults, other valid conditioning and invalid segments. The first DAC
fixtures placed the segment before the frame header, where the encoder's own
per-scan DAC overrode it. They are now spliced after it and decode to 16
distinct outputs. `tools/test_jpeg_errors.py` adds every byte prefix of
sequential, progressive and restart arithmetic streams. It also adds byte flips
and inserted markers in the entropy data, and each restart marker replaced by
the next, second-next, previous and a far RST, and by APP/COM/TEM/reserved
markers.

All 11,225 decode, 24,108 error and 2,297 limit cases match libjpeg-turbo 3.1.1
in normal, sanitizer-client and codec-free builds. The first damaged-stream
run differed on 363 cases (marker messages, DAC/DRI in the header model,
Huffman table checks on arithmetic scans, restart resynchronization); all were
fixed. Eight mutations are detected. Two first survived: `jpeg_arith_resync_other_marker`
had only truncated restart streams, whose EOI makes both resync actions
equivalent. The chosen restart fixture also had no RST markers. Marker
replacements on streams with restarts now detect it (675 mismatches). The
first `jpeg_dac_index` mutant was equivalent: the decoder's own DAC parser
reports the same error. Disabling the header model's DAC handling is detected
(84). Both reports are retained. 342 mutations total.

### Built-in JPEG encoding

libheif's JPEG encoder plugin feeds libjpeg-turbo interleaved scanlines, with
the 4:2:0 chroma replicated to full resolution. libjpeg then downsamples it
back (h2v2), runs the islow forward DCT and quantizes by reciprocal
multiplication with the quality-scaled standard tables (forced baseline). It
codes with the standard Huffman tables and writes JFIF 1.01 with the pixel
aspect ratio as density. `src/jpeg_encoder.rs` reproduces that pipeline in
Rust:

- the scalar libjpeg-turbo arithmetic (int DCTELEM, `compute_reciprocal`);
- edge replication to whole blocks and MCU rows;
- dummy blocks carrying the previous DC;
- one-bit padding and byte stuffing;
- the plugin's marker order (APP0, DQT 0/1, SOF0, four DHT, SOS, EOI).

On 270 randomized images (1x1 to 100x3, quality 0-100, noise/gradient/flat,
non-square density) it is byte-identical to a harness making libheif's libjpeg
calls.

`crates/capi/src/builtin_jpeg_encoder.rs` registers it as a static
encoder-plugin record (version 4, priority 100), so the existing plugin paths
handle input conversion, alpha, thumbnails and sequences. The record carries
the plugin's semantics:

- quality 0-100, default 50;
- lossless means quality 100;
- the parameter list and unsupported-parameter errors;
- the YCbCr/8-bit input checks;
- density for normal and thumbnail images only.

The id and name are libheifer's own (`libheifer-jpeg`), as with the other
built-in codecs. The JPEG oracle is now built with libheif's JPEG encoder.

`tools/test_jpeg_encoding.py` covers:

- 200 cases of seven sizes and six input colorspaces/chroma formats;
- twelve qualities and the lossless flag;
- bit-depth errors;
- metadata and pixel-aspect-ratio flags, orientations and thumbnails;
- overlays, repeated encodes and alpha planes.

It compares exact files, handles and decoded read-back pixels in normal,
sanitizer-client and codec-free builds. It matched on the first run.

With a JPEG encoder always present, `test_encoding` format-0 cases (highest
priority encoder) and `test_other_encoding` cases whose test plugin is
rejected now use it, as libheif with its JPEG encoder does. Those suites now
compare against the JPEG oracle (1,292 and 2,382 cases, 0 mismatches). Six
mutations are detected. The first reciprocal-rounding and thumbnail-density
mutants were equivalent and were replaced. Divisors are multiples of 8, so the
remainder never equals half the divisor, and libheif's thumbnail image carries
no aspect ratio. Both reports are retained. 348 mutations total.

### HTJ2K decoding

libheif decodes HTJ2K (ITU-T T.814) `j2k1` items with OpenJPEG, which decodes
HT code-blocks with `ht_dec.c`. `vendor/hayro-jpeg2000/src/j2c/ht.rs` ports that
decoder: MEL, VLC and MagSgn cleanup decoding, SigProp and MagRef refinement,
and OpenJPEG's checks on malformed blocks. Around it, the vendored decoder
follows OpenJPEG's handling:

- segment assignment: the first segment takes one pass per packet, so
  refinement passes in later layers fail as they do in OpenJPEG;
- the zero bit-plane count is the tag-tree value plus one;
- T1 output is halved for 5/3 and scaled by half the step size for 9/7;
- the initial MEL reads depend on the data address modulo 4, which is modelled
  from the offset in OpenJPEG's concatenated tile-part buffer;
- CAP and CPF are skipped, mixed HT style (0x80) is rejected, and an RGN shift
  fails HT decoding.

The HTJ2K corpus found two packet-header differences, also affecting
ordinary JPEG2000. Both now match `opj_bio`:

- the byte after 0xFF carries seven bits, and its top bit is ignored rather
  than rejected;
- a header whose last byte read is 0xFF is followed by one skipped byte, even
  when the header ends inside that byte.

Test inputs:

- `tools/generate_htj2k_fixtures.py` uses a pinned, test-only OpenJPH
  (8c2826f) to produce 438 codestreams. They vary size, depth, signedness,
  5/3 and 9/7, block size, decomposition levels, progression, tiles and tile
  parts, precincts, subsampling, colour transform, offsets and TLM.
- `tools/test_htj2k.py` compares them in 25 decode modes: 10,950 cases, all
  matching.
- `tools/test_htj2k_errors.py` has 11,701 cases, all matching. It takes
  complete prefixes and corrupted cleanup bytes. It also builds single-block
  packets: OpenJPH cleanup data plus seeded SigProp/MagRef data, 2-4 passes,
  multiple layers, changed zero bit-planes, extra guard bits (so refinement
  is decoded) and code-block styles. It also changes COD styles and inserts
  RGN, CAP and CPF markers.
- The larger first corpus (52,347 cases) also matched after the fixes.

Eleven mutations are detected (`results/htj2k-mutations-report.json`). The
initial report keeps the first run, where `ht_stripe_causal` survived until
sparse low-amplitude bases were added. Normal and no-codecs runs pass locally
and in CI; the sanitized group runs in CI.

### Built-in JPEG2000 encoding

libheif's JPEG2000 encoder plugin hands each plane to OpenJPEG with OpenJPEG's
default parameters. Unless `set_lossless(0)` is called the codestream is
reversible (5/3, no rate); lossy encodes use the 9/7 wavelet and a single
layer at rate `1 + (100 - quality) / 2`. The plugin writes one tile, six
resolutions, 64x64 code-blocks, LRCP, no colour transform and a COM marker.
`src/jpeg2000_encoder.rs` reproduces that encoder in Rust:

- OpenJPEG's lifting (5/3 integer, 9/7 float constants) and `lrintf`
  quantisation, with QCD in no-quantisation or scalar-expounded form;
- T1 significance, refinement and cleanup passes with the MQ coder, including
  OpenJPEG's zero-coding table quirk (the swapped table serves the HL band),
  pass rates (+3 before the final flush), the monotonic rate fix-up and
  trailing-0xFF trimming;
- distortion from OpenJPEG's nmsedec tables and wavelet norms;
- rate allocation: the byte budget from `opj_j2k_update_rates` (float
  arithmetic, main header subtracted, 30-byte floor), slope bisection with
  packet sizing, and the good-threshold fallback;
- the tile buffer bound, so images whose packets outgrow it fail with
  "Failed opj_encode()" as the plugin does, and the 32-pixel minimum from
  `opj_start_compress`.

A C harness making the plugin's OpenJPEG calls found the encoder byte-identical
on 1,900 randomized lossless and 560 lossy images, plus low bit-depth cases.

`crates/capi/src/builtin_jpeg2000_encoder.rs` registers it as a static
encoder-plugin record (version 4, priority 80, id `libheifer-jpeg2000`) with the
plugin's semantics:

- quality 0-100, default 70;
- the lossless flag selecting reversible coding (default on);
- the `chroma` string parameter (420/422/444) chosen in the colorspace query;
- the plugin's getter quirks and error messages.

The native oracles are now built with OpenJPEG's encoder
(`WITH_OpenJPEG_ENCODER`). `test_encoding` and `test_other_encoding` fall back
to the highest-priority available encoder, so they now compare against
`.build/reference-encoders`, built with both libjpeg-turbo and OpenJPEG (1,292
and 2,382 cases, 0 mismatches in normal and sanitizer-client builds).

`tools/test_jpeg2000_encoding.py` covers 175 cases:

- seven sizes (including below the 32-pixel minimum) and six input
  colorspaces/chroma formats, lossless and lossy;
- eleven qualities;
- the `chroma` parameter and bit depths 1-16;
- metadata, orientations, thumbnails, overlays, alpha and repeated encodes.

It compares exact files, handles and decoded read-back pixels in normal,
sanitizer-client and codec-free builds; all match. Nine mutations are detected
(`results/jpeg2000-encode-mutations-report.json`). The initial report keeps
the first run, where the chroma mutant survived: it changed the version-1
colorspace query, which libheif does not call for version-4 plugins. It was
retargeted to `query_input_colorspace2`. 368 mutations total.

### Built-in HTJ2K encoding

libheif encodes HTJ2K with its OpenJPH plugin. Its behaviour:

- **Parameters:** quality is stored but ignored. The lossless flag defaults to
  off, so the default is 9/7 with OpenJPH's step of 2^-min(depth, 16). The
  defaults are 5 decompositions, RPCL, 64x64 code-blocks, 4:4:4 input, planar
  components and no colour transform.
- **Missing features:** the plugin tests `OPENJPH_MAJOR_VERSION` and
  `OPENJPH_MINOR_VERSION`, but OpenJPH defines `OPENJPH_VERSION_MAJOR` and
  `_MINOR`. So `tlm_marker` and `tilepart_division` are not compiled in, and a
  `codestream_comment` is stored but never written. The oracle confirmed this
  when it rejected `tilepart_division`.

`src/htj2k_encoder.rs` ports OpenJPH 0.32.0's encoder. OpenJPH works line by
line; the port applies the same arithmetic to whole tile-components:

- the 5/3 and 9/7 lifting, including OpenJPH's handling of single rows and
  columns;
- float-to-integer conversion and truncating quantisation;
- the step-size derivation, including two unsigned wraps:
  - an exponent beyond 31 wraps in the 5-bit field;
  - `get_largest_Kmax` takes its maximum before adding the guard bits.
- the 32-bit sign-magnitude layout: with no decompositions, a lossless
  sample's magnitude can reach the sign bit, and the HT coder then codes it as
  zero;
- the HT cleanup-pass coder (MEL, VLC with the OpenJPH-built tables, UVLC,
  MagSgn);
- packet headers whose tag trees read past the end of a row into the next one
  when a subband has an odd number of code-blocks;
- progression orders, tiles, tile parts, TLM and the tile-part adjustment
  messages OpenJPH prints to stdout.

A C++ harness making the plugin's OpenJPH calls compared randomized images
across sizes, depths, subsampling, lossless and lossy coding, 0-32
decompositions, all progressions, code-block shapes, tiles, tile-part
divisions, TLM and comments. The final code is byte-identical on 2,000 cases.
The cases OpenJPH rejects (too many tiles or tile parts) also fail in Rust.

`crates/capi/src/builtin_htj2k_encoder.rs` registers the encoder as a static
encoder-plugin record: format HTJ2K, priority 80, id `libheifer-htj2k`. It has
the seven parameters the plugin compiles, with their defaults. It copies the
plugin's quirks:

- `stoul` parsing of `tile_size` and `block_dimensions`;
- decompositions in 0-32;
- no `query_encoded_size`;
- one codestream per encode.

`tests/encoding.c` can now apply encoder parameter sets, selected by record
word 0, bits 24-31, through `heif_encoder_set_parameter`.
`tools/test_htj2k_encoding.py` covers 311 cases:

- sizes and input colorspaces, lossless and lossy;
- qualities, chroma and bit depths;
- all 39 parameter sets, including 32 decompositions, tiles, code-block shapes,
  comments and invalid values;
- metadata, orientation, overlays, alpha and thumbnail bounding boxes.

It compares exact files, handles and decoded read-back pixels in normal,
sanitizer-client and codec-free builds. A second encode with one OpenJPH
encoder aborts the native process ("Quantization step sizes already
initialized"), because the plugin never restarts its codestream. Those cases
are left out. libheifer encodes again.

The read-back found two decoder differences, affecting all JPEG2000. Both now
follow OpenJPEG:

- a lone odd sample in 9/7 synthesis is left unscaled; OpenJPEG halves it only
  for 5/3;
- precinct steps are computed in 64 bits, so RPCL/PCRL/CPRL with more than 16
  decompositions decode.

42 new OpenJPH fixtures cover these (480 HTJ2K fixtures, 12,000 decode cases,
all matching).

The corpus also found that the JPEG2000 encoder record from the previous step
declared `chroma` as a boolean. libheif's type numbering is integer 1,
boolean 2, string 3. Generic `heif_encoder_set_parameter` calls now reach it,
and `test_jpeg2000_encoding` gains generic-parameter cases (183).

`test_encoding` and `test_other_encoding` compare against an oracle built with
libheif's JPEG, OpenJPEG and OpenJPH encoders.

13 mutations are detected (`results/htj2k-encode-mutations-report.json`). The
initial report keeps the first run: an equivalent K constant survived there,
because 1.2301741 is the same `f32`. It was replaced by a value one ULP away.
381 mutations total.

### Built-in AV1 encoding (rav1e)

libheif's rav1e plugin configures rav1e through its C API. It sets:

- the pixel format from the image chroma; alpha is always 4:2:0;
- still-picture mode, dimensions and threads;
- the nclx colour description;
- `min_quantizer`, and `quantizer = ((100 - quality) * 255 + 50) / 100`;
- tile rows and columns when not 1, and speed.

It then sends one frame and collects the packets. The plugin defaults are:

- speed 8, 4 threads, 4x4 tiles, chroma 4:2:0, min-q 0;
- quality 0, because quality is missing from the parameter list and `new T()`
  zero-initialises it;
- priority 20;
- lossy only.

`crates/capi/src/builtin_av1_encoder.rs` transliterates the plugin. It calls
libheifer's own `heif_image_*` C API and the same `rav1e::capi` functions the
native plugin calls, so configuration parsing, frame filling, padding and
packet delivery run identical code. It also carries the plugin's input checks
(no monochrome; 8, 10 or 12 bits) and its sequence-encoding path.

rav1e 0.8.1 is vendored as a Rust-only tree (assembly removed; see
`AV1_DEPENDENCIES.md`). The oracle builds librav1e with cargo-c from git tag
v0.8.1, whose `src/` is identical, without assembly and with a lockfile
aligned to libheifer's. It is built into `.build/reference-av1` and
`.build/reference-encoders` (with dav1d for read-back).

`tools/test_av1_encoding.py` covers 143 cases:

- sizes and input colorspaces;
- qualities and bit depths 7-16;
- speed, thread, tile, min-q and chroma parameter sets, including invalid
  values;
- `heif_encoder_set_lossless` after `min-q` (the new `@lossless` entry in
  `tests/encoding.c`);
- nclx and HDR metadata, orientations, overlays, thumbnails, and alpha under
  4:2:0 and 4:4:4 chroma.

It compares exact files, handles and decoded read-back pixels in normal,
sanitizer-client and codec-free builds. It matched on the first run.

`test_plugin_encoding` and `test_mini_encoding` fall back to the default AV1
encoder when a test plugin is refused, so they now use the rav1e oracle.
`test_encoding` and `test_other_encoding` use the encoder oracle, which now
includes rav1e and dav1d.

Eight mutations are detected (`results/av1-encode-mutations-report.json`).
The initial report keeps the first run, in which three mutants survived:

- the alpha-sampling mutant needed alpha cases with 4:4:4 chroma, which were
  added;
- the colour-description class mutant is equivalent, because the plugin's
  second, unconditional call covers every class;
- the lossless/min-q mutant is equivalent, because rav1e uses
  `min_quantizer` only under bitrate control and the plugin always sets a
  quantizer. A `min_quantizer` mutant confirmed this and was dropped.

The last two were replaced by a tile-column default mutant. 389 mutations
total.


## Built-in Rust VVC decoding

libheif's only VVC decoder is its vvdec plugin. `src/vvc` is an in-tree pure
Rust decoder written from vvdec 3.2.0 (Clear BSD, `licenses/vvdec.txt`),
registered as the built-in decoder "libheifer VVC decoder" (id
`libheifer-vvc`, priority 100 like the plugin). It decodes intra and inter
pictures:

- parameter sets with vvdec's checks, CABAC, partitioning with dual and local
  dual trees;
- intra prediction (MIP, ISP, MRL, CCLM, PDPC, wide angles) and intra block
  copy with vvdec's merge list and history;
- residual coding with dependent quantization, sign hiding, transform skip,
  BDPCM, LFNST, MTS, JCCR, ACT and scaling lists;
- LMCS, deblocking with LADF, SAO, ALF and CC-ALF;
- tiles, rectangular and raster slices, WPP, subpictures and virtual
  boundaries;
- a DPB with POC derivation, RPL-based marking and vvdec's output, RASL,
  GDR and missing-reference rules;
- merge, AMVP, MMVD, SMVD, affine, SbTMVP, GEO, CIIP, HMVP and TMVP motion
  derivation; interpolation, BCW and weighted prediction, BDOF, DMVR and
  PROF; SBT and inter MTS; inter deblocking strengths with subblock edges;
- reference wraparound (vvdec's wrapped border buffers and its `clipMv`/
  `wrapClipMv` clipping), references of subpictures treated as pictures, and
  reference picture resampling with vvdec's scaling-window positions and RPR
  filter sets.

`src/vvc/heif.rs` reproduces libheif's VVC item path and the plugin:

- the `vvcC` NAL arrays with four-byte lengths;
- the pre-decode configuration SPS size check, including its unsupported
  GCI and subpicture cases;
- the plugin's NAL splitting and error codes;
- planar output in the stream's chroma format and bit depth, without colour
  information.

`vvc1` handles report chroma format and bit depth from `vvcC`.

The decoder was debugged against vvdec's syntax trace, with vvdec's in-loop
filters disabled stage by stage in a local tracing build. Every output
picture of the 268 JVET conformance streams was compared with vvdec 3.2.0:
237 streams match bit-exactly in every picture (intra, random access, low
delay, 4:0:0 to 4:4:4, 8 and 10 bit, wraparound, subpictures, tiles, slices
and reference picture resampling). The other 31 are the palette and
multi-layer streams, which are rejected as they are by vvdec.

`vvc1` sequence tracks go through a stateful decoder that follows libheif's
use of the vvdec plugin: `vvcC` units with sample 0, length-prefixed units
queued per sample and fed one at a time until a picture is output, a flush
at the end, and each picture's `cts` taken from its last slice.
`tools/test_vvc_sequences.py` builds tracks from 22 vvenc streams (random
access with CRA and IDR periods, low delay, all intra, 10-bit and 4:0:0, two
sizes) with short, repeating and chunked variants: 88 cases with no
mismatches against libheif with vvdec, and the same in the codec-free build.

`tools/generate_vvc_fixtures.py` encodes 277 owned single-picture streams with
the pinned test-only vvenc 1.14.0 (`tests/vvc_fixture_encoder.c`). They cover
presets, QPs 0-63, 8/10-bit 4:2:0 and 4:0:0, sizes from 1x1 to 256x160 with
conformance windows, 33 tool toggles, and CC-ALF and LMCS on
correlated luma/chroma content. vvenc rejects 4:2:2, 4:4:4, 12-bit
and odd 4:2:0 sizes; those are recorded as generator failures.
`tools/test_vvc.py` wraps them as `vvc1` items the way libheif's encoder
writes them. It compares samples, handles, conversions and errors against
libheif with vvdec in 25 modes: 6925 cases with no mismatches, and the same in
the codec-free build against libheif without vvdec. The two 1x1 streams fail
in both, because vvdec's conformance-window check treats 4:0:0 horizontally
like 4:2:0.

Nine deliberate VVC defects (deblocking filter length, SAO band position, ALF
boundary rounding and transposition, CC-ALF, MRL index, LMCS chroma scaling,
the `vvcC` NAL length prefix and the 4:0:0 conformance-window check) were run
against the vvdec oracle. Seven were detected initially
(`docs/results/vvc-decode-mutations-initial-report.json`). The two survivors
exposed fixture gaps: the CC-ALF rounding change was never observable on the
generated content, and no fixture scaled LMCS chroma residuals. The seven
`ccalf-*` and `lmcs-*` fixtures were added. A CC-ALF tap defect replaces the
rounding one, and both it (92 mismatches) and the LMCS chroma rounding defect
(23 mismatches) are now detected
(`docs/results/vvc-decode-mutations-replacement-report.json`).

Known differences:

- in tracks whose samples fail to decode, vvdec returns pictures only after a
  parse delay derived from the host's thread count, so how many pictures come
  before the error differs from libheif (and between machines for libheif
  itself); these variants are not compared;
- malformed-stream behavior (vvdec's exception and recovery paths) has not
  been compared systematically;
- decoding is scalar and single-threaded: the first 720p picture of ALF_A
  takes about 0.09 s against 0.033 s for single-threaded vvdec with SIMD.
