# libheifer implementation and acceptance plan

## Contract

Reference: libheif v1.23.4 (`4e14f5942c1732ace9611b9522cc991501445463`).
Inventory every installed C header, including APIs not included by heif.h,
deprecated declarations, codec-plugin structs and callbacks. Track experimental,
Emscripten and C++ convenience interfaces separately; they must not silently vanish
from the inventory. C++ wrappers are consumers of the C ABI, not Rust classes.

The target is the entirety of the API. Each function needs a Rust implementation,
an optional ABI adapter, and behavioral evidence. Missing functions stay missing
and are reported as failures by the completeness gate. Unsupported-operation
stubs, forwarding to libheif, or binding generation do not count as implementation.
No finite suite proves perfect equivalence for all inputs. Report the exact
reference, build configuration, architectures, tested domains and uncovered cases.

USER REQUIREMENT: no C or C++ implementation dependencies. All codecs, container
parsing/writing, image and metadata models, transforms/conversion, resource limits,
and plugin management must use pure Rust dependencies or code implemented here.
Native libheif and its native codecs are test-only reference oracles. Do not link,
load, forward to, or spawn native codecs in the candidate. Audit transitive
dependencies and generated artifacts for native code. Disabling codecs cannot
satisfy completeness. Compare equivalent codec quality/settings and disclose
algorithmic differences; independent encoders need interoperability, decoded
quality and rate/distortion checks rather than arbitrary bitstream identity.

Defined observable behavior includes pixels, metadata, IDs/order, return and
suberror codes, message contents where stable, output-argument writes on success
and failure, defaults, callbacks/order, ownership, pointer lifetimes, alignment,
strides, reference counts and concurrency. Padding bytes and allocation addresses
are not compared. Undefined upstream behavior is not a target; such inputs are
tested for candidate safety and recorded separately, never counted as matches.

## Architecture

- `libheifer`: safe Rust core, typed errors, borrowed parsing, owned images and
  explicit resource budgets. No dependency on libheif, including at runtime.
- `libheifer-capi`: optional workspace package; C integers at ABI boundaries,
  `repr(C)` public structs, documented unsafe contracts, stable owned handles,
  contained panics and no unwinding across C. Allocation failures need controlled
  errors where possible; allocator-abort behavior cannot be advertised as parity.
- Codec traits backed only by pure Rust implementations. The optional C plugin
  API can expose callback hooks for external callers without making native plugins
  a required or bundled implementation dependency. Full plugin equivalence still
  needs explicit version/lifetime/registration tests.
- Test-only pinned reference built independently. Separate processes for the
  reference and candidate avoid ELF symbol interposition and shared global state.

## Ordered implementation stages

1. **Contract and test foundation.** Freeze API inventory and header hashes;
   independent C clients, layouts/enum values, symbol checks, strict missing-function
   gate, reproducible baseline and corpus manifests. Keep the implementation on
   its feature branch so all changes remain reviewable in the PR.
2. **Fundamental values and buffers.** Brands/sniffing, errors, library lifecycle,
   security limits, image allocation/planes, color/HDR profiles, options and timestamps.
   Differential malformed-input, integer-boundary and lifetime tests first.
3. **Container and object model.** Bounded BMFF parsing, box serialization, item
   locations/references/properties, primary images, handles, metadata, auxiliary
   images/depth/thumbnails. Reader/writer callbacks and seekless/partial input.
4. **Pure Rust codecs, decode and conversion.** Audit available pure Rust HEVC,
   AV1, AVC, VVC, JPEG, JPEG2000/HTJ2K and uncompressed codec implementations.
   Implement missing codec functionality in Rust. HEVC/AV1 first; exact planes, strides, bit depths,
   endianness, color profiles, premultiplication, crop/rotate/mirror, grids/overlays,
   tiling and progressive reads. Then every other codec upstream supports.
5. **Encode and richer APIs.** Encoder discovery/configuration, lossless/lossy
   interop, all image properties, regions/masks, entity groups, timestamps,
   components/polarization, OMAF, uncompressed formats, sequences and tracks.
6. **Plugin and binary replacement.** All callback versions, plugin discovery and
   load/unload, thread stress, C/C++ downstream clients compiled against the pinned
   headers and run without rebuilding against the candidate. Historical supported
   headers, Linux SONAME, macOS install name, Windows calling convention/exports.
7. **Performance iteration.** Profile full parse/decode/convert/encode workflows;
   isolate hot paths. One hypothesis/change/commit at a time; keep raw interleaved
   release A/B timings, allocations and instruction counts separately. Common
   phone HEIC, AVIF, HDR, alpha, grids, tiny files and large metadata; files, memory,
   readers and pipes; low and deployed concurrency. Equivalent codec and quality settings.
8. **Release gate and PR.** Every inventory entry has reviewed behavioral coverage;
   zero ABI mismatches and missing symbols on all claimed platforms, full regression
   and fuzz corpus, sanitizer/ownership checks, upstream and downstream suites,
   and repeated end-to-end performance gains without unexplained regressions.
   Keep the PR draft while any mandatory gate remains open.

## Test layers and required evidence

| Layer | Required checks |
|---|---|
| API | AST declarations, typedefs, enum values, callback signatures, version guards, exported symbols and header hashes |
| ABI | C sizeof/alignof/offsetof, struct return/callback calling conventions, old option versions, sentinel buffers, old compiled clients |
| Behavior | Independent reference/candidate transcript equality, error and out-parameter semantics, nulls where defined, ownership after context release |
| Files/codecs | Real licensed corpus plus generated orthogonal fixtures; pixel/metadata hashes; both cross-encode/decode directions; deterministic bytes only when guaranteed |
| Adversarial | Every truncation, size/offset overflow, cycles, duplicated IDs, partial reads, allocation budgets, callback failures, historical regressions |
| Safety | ASan/UBSan C/reference, Rust Miri where supported, fuzzing with persisted minimized regressions, threaded lifetime/plugin tests |
| Performance | Pinned optimized binaries, same codecs/options/data, warmups, repeated interleaved A/B, sample count/dispersion, profile and full-workload results |

Every claimed API family must have independently executed tests. Generating names
from a candidate's own exports is not sufficient coverage. Mutation checks should
demonstrate that wrong enum values, pixels, errors and layouts fail the suite.
Never normalize away a real difference to turn a result green. Skips and known
differences must remain visible, and block the final compatibility claim.

## Progress

The machine-readable inventory and checked-in test reports record implemented
coverage. The whole-library success criteria remain open until stage 8 passes.

Draft PR: https://github.com/reaperhulk/clanker-experiments/pull/1. The first
implementation iteration covers 37 functions (brands and planes); the second
adds 28 color/HDR functions. The context/handle iteration adds 38 functions,
including ownership/reload behavior, metadata, thumbnail and color queries.
The decoding iteration adds six exports for C decoding/options and crop/scale,
with ordered color conversion, alpha attachment, transforms and 16-bit packing.
The derived-image iteration adds grids/identity, cycle/MIAF checks, scoped tile
workers and four warning/thread-control exports. Overlay composition, raw masks,
shared-graph decode-operation budgets and dynamic derived queries follow in the next iteration. Six security-limit/allocation exports follow, including context-wide
allocation accounting and object-specific resource lifetimes. Auxiliary/depth, item-property, six camera-matrix, twenty-two sensor-metadata and thirty-nine component APIs, generic items/compression, twelve TAI timestamp APIs, five metadata writers and nine text-item APIs bring the total to
260 partial functions, with 205 functions missing. Compatibility work comes first;
performance optimization is deferred until the complete compatibility gate passes.
The full header contract, finite behavioral reports, known differences and
performance evidence are retained in `compat/` and `docs/results/`.
