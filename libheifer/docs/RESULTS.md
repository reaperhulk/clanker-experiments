# Current result: incomplete implementation, no demonstrated performance win

Reference: libheif 1.23.4, commit `4e14f5942c1732ace9611b9522cc991501445463`.
HEVC reference backend: libde265 1.0.16, commit
`7ba65889d3d6d8a0d99b5360b028243ba843be3a`. Candidate uses only Rust code;
the optional experimental HEVC feature uses rusty_h265/rusty_h265-accel 0.6.0.

## Implemented scope

103 of 465 public functions are exported, plus `heif_error_success`.
362 functions are missing. Even the exported functions are marked **partial**:
coverage is finite, platform coverage is incomplete, and one malformed-box
behavior gap is explicitly retained. There is no claim of a compatible library.

- Version, brands and MIME/file sniffing; owned compatible-brand lists.
- Image creation, aligned zeroed planes, layout/stride/bit-depth/channel queries,
  plane access, alpha-premultiplication flag and pixel aspect ratio.
- Color-conversion option defaults/copy, ICC and NCLX profile storage/queries,
  HDR content light, mastering display, ambient viewing and diffuse white metadata.
  Profile storage does not yet perform color conversion during decoding.
- Rust-only bounded item-container parsing and experimental direct HEVC native
  YUV decoding. The C decoding entry points remain unimplemented.
- Context allocation and memory reads; shared image handles, primary/top-level IDs,
  direct HEVC descriptions, thumbnails, uncompressed metadata and color queries.
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
| ABI | Eight structs match original-header size, alignment and every field offset; struct-return and data-symbol clients pass | Linux x86_64 only |
| Mutation checks | Seven deliberately wrong implementations rejected | Wrong filetype enum, error code, plane samples, primary coordinate, primary item ID, alpha reload state and ABI field order; isolated builds, successful baselines, compiler/crash failures do not count |
| HEVC native output | 5/5 fixtures match exactly | Y/Cb/Cr data, image IDs/order, dimensions, depths and strides; transformations disabled, native NCLX passthrough; **alpha not compared** |
| HEVC default output | 4/5 tested color-plane outputs match | Example image differs because default NCLX conversion is not implemented |
| Dependency guard | Pass | Two reviewed codec crates; no native build scripts or codec link dependencies in the resolved candidate graph |
| Rust checks | Unit tests, ABI test, formatting and Clippy pass | Linux, macOS and Windows Rust build jobs passed for the color/HDR iteration; full sanitizer, fuzzing and cross-platform ABI validation remain open |
| Completeness gate | **Fail**, as required | Missing API and unvalidated entries prevent a success claim |

The separately retained `known-differences.json` records malformed non-ftyp box
behavior that the broad brand corpus did not cover. This is not suppressed or
treated as matching. The five HEIC fixtures are smoke coverage for direct items,
not HEVC conformance or full container compatibility. The decoder experiment has
not established safe resource-budget behavior on hostile inputs.

A previous Valgrind attempt could not execute the client in this environment
(permission denied). The context C clients now pass ASan/UBSan; the libraries
are not instrumented, and no full-library memory-safety or local leak-test pass
is claimed. The [color/HDR CI run](https://github.com/reaperhulk/clanker-experiments/actions/runs/35313115635)
passed Rust builds on Linux, macOS and Windows and all Linux development checks,
including the color differential and mutation tests. Its only failing step was
the full-completion gate. A job summary is retained in `results/ci-color-report.json`.

The context corpus is a finite tested subset, not full parser equivalence. It
includes five real HEIC fixtures and generated containers. All 38 added exports
remain partial. Unsupported image types, compressed metadata, duplicate-box and
essential-property behavior, clean-aperture edge cases, configurable budgets,
file/reader callbacks and C decoding remain open. `context-report.json` records
exact binary, client and corpus hashes. `context-sanitized-report.json` records
the limited sanitizer scope; mutation evidence rejects wrong reload semantics.

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
4. Profile and optimize the actual codec hot paths, with repeated output-checked
   A/B results; expand beyond the current single-file native-sample benchmark.
5. Complete cross-platform ABI, fuzzing, memory-safety, downstream-client and
   full API gates before marking the draft PR ready or making a compatibility claim.
