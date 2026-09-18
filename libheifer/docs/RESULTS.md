# Current result: incomplete implementation, no demonstrated performance win

Reference: libheif 1.23.4, commit `4e14f5942c1732ace9611b9522cc991501445463`.
HEVC reference backend: libde265 1.0.16, commit
`7ba65889d3d6d8a0d99b5360b028243ba843be3a`. Candidate uses only Rust code;
the HEVC feature uses rusty_h265/rusty_h265-accel 0.6.0. The former is vendored
with VUI unspecified-color, monochrome-decoding and typed missing-parameter fixes; its Apache-2.0 notices are retained.

## Implemented scope

172 of 465 public functions are exported, plus `heif_error_success`.
293 functions are missing. Even the exported functions are marked **partial**:
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
| Brand/version differential | 17,126 cases, 0 mismatches in this corpus | Original-header C clients in separate processes; header truncation, malformed lengths, extended sizes, duplicate brands, NULs, signatures, error messages, out-argument preservation |
| Image differential | 6,553 transcripts, 0 mismatches | Color/chroma combinations, dimensions, bit-depth boundaries, alignment/stride, zeroed storage, duplicate planes, pointer identity, flags; excludes transforms and resource budgets |
| Color/HDR differential | 198,932 transcripts, 0 mismatches | All uint16 NCLX setter inputs, all uint8 option-version pairs, all chromaticity coordinates, exact floating-point bits, ICC ownership, HDR boundaries and output sentinels; no image-handle APIs or color transforms |
| Context/handle differential | 1,786 transcripts, 0 mismatches | Copied/borrowed input, destroyed input copies, aliases, handles after free/reload, early and late read failures, primary/hidden/boundary IDs, metadata bytes/filters, thumbnails, alpha references, color profiles, rotations, every synthetic-file/property truncation; no C decoding |
| C client sanitizers | ASan/UBSan clients pass the context corpus | Libraries are not sanitizer-instrumented; local LeakSanitizer could not run under ptrace, so leak detection was explicitly disabled |
| ABI | Fifteen structs match original-header size, alignment and every field offset; struct-return and data-symbol clients pass | Linux x86_64 only |
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

The separately retained `known-differences.json` records malformed non-ftyp box
behavior that the broad brand corpus did not cover. This is not suppressed or
treated as matching. The five HEIC fixtures are smoke coverage for direct items,
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
