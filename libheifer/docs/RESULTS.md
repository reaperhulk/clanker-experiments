# Current result: incomplete implementation, no demonstrated performance win

Reference: libheif 1.23.4, commit `4e14f5942c1732ace9611b9522cc991501445463`.
HEVC reference backend: libde265 1.0.16, commit
`7ba65889d3d6d8a0d99b5360b028243ba843be3a`. Candidate uses only Rust code;
the optional experimental HEVC feature uses rusty_h265/rusty_h265-accel 0.6.0.

## Implemented scope

37 of 465 public functions are exported, plus `heif_error_success`.
428 functions are missing. Even the exported functions are marked **partial**:
coverage is finite, platform coverage is incomplete, and one malformed-box
behavior gap is explicitly retained. There is no claim of a compatible library.

- Version, brands and MIME/file sniffing; owned compatible-brand lists.
- Image creation, aligned zeroed planes, layout/stride/bit-depth/channel queries,
  plane access, alpha-premultiplication flag and pixel aspect ratio.
- Rust-only bounded item-container parsing and experimental direct HEVC native
  YUV decoding. Neither context/handle nor decode C functions are implemented.

## Executed checks

| Check | Outcome | Scope/limits |
|---|---|---|
| Header inventory | 465 functions, 1 exported variable, 37 structs, 38 enums, 108 macros; hashes for 29 headers | Three unexported plugin convenience variables retained separately; C++ wrappers tracked by header hash |
| Brand/version differential | 17,126 cases, 0 mismatches in this corpus | Original-header C clients in separate processes; header truncation, malformed lengths, extended sizes, duplicate brands, NULs, signatures, error messages, out-argument preservation |
| Image differential | 6,553 transcripts, 0 mismatches | Color/chroma combinations, dimensions, bit-depth boundaries, alignment/stride, zeroed storage, duplicate planes, pointer identity, flags; excludes transforms and resource budgets |
| ABI | heif_error size/alignment/field offsets agree; struct-return and data-symbol clients pass | Linux x86_64 only |
| HEVC native output | 5/5 fixtures match exactly | Y/Cb/Cr data, image IDs/order, dimensions, depths and strides; transformations disabled, native NCLX passthrough; **alpha not compared** |
| HEVC default output | 4/5 tested color-plane outputs match | Example image differs because default NCLX conversion is not implemented |
| Dependency guard | Pass | Two reviewed codec crates; no native build scripts or codec link dependencies in the resolved candidate graph |
| Rust checks | Unit tests, ABI test, formatting and Clippy pass | Full sanitizer, fuzzing and cross-platform ABI validation remain open |
| Completeness gate | **Fail**, as required | Missing API and unvalidated entries prevent a success claim |

The separately retained `known-differences.json` records malformed non-ftyp box
behavior that the broad brand corpus did not cover. This is not suppressed or
treated as matching. The five HEIC fixtures are smoke coverage for direct items,
not HEVC conformance or full container compatibility. The decoder experiment has
not established safe resource-budget behavior on hostile inputs.

A Valgrind attempt could not execute the client in this environment (permission
denied). No memory-safety or leak-test pass is claimed. The authored CI workflow
has not run remotely because publishing is blocked. Its completion step is
intentionally failing until the API contract is implemented and validated.

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
output are in [results](results/). No performance optimization is claimed yet.

## Next implementation work

1. Finish the parser and resource-budget model; add context/handle C APIs and
   independent malformed-input/property/ownership differentials.
2. Implement NCLX/ICC metadata and exact color conversion, transforms and
   alpha/grid composition; extend codec fixtures and conformance profiles.
3. Implement remaining codecs/encoders and all advanced API families in PLAN.md.
4. Profile and optimize the actual codec hot paths, with repeated output-checked
   A/B results; expand beyond the current single-file native-sample benchmark.
5. Complete cross-platform ABI, fuzzing, memory-safety, downstream-client and
   full API gates before turning the future PR into a compatibility claim.
