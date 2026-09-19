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
allocation accounting and object-specific resource lifetimes. Auxiliary/depth, item-property, six camera-matrix, twenty-two sensor-metadata and thirty-nine component APIs, generic items/compression, twelve TAI timestamp APIs, five metadata writers nine text-item APIs three image-area APIs thirteen handle color/aspect APIs and three component-definition queries bring the total to
457 partial functions, with 8 functions missing. Compatibility work comes first;
performance optimization is deferred until the complete compatibility gate passes.
The full header contract, finite behavioral reports, known differences and
performance evidence are retained in `compat/` and `docs/results/`.

Uncompressed pixel iteration: all seven decoder factories now have Rust paths.
The independent client records complete channel/component bytes, format/depth,
callbacks and retained handles; generated cases cover integer, float and complex
formats, packing, padding, endian flags and rejected layouts. This adds behavior
to existing exports; the 279 partial / 186 missing function totals are unchanged.
Next: uncompressed generic compression and sensor properties, exact range reads
and resource limits, then remaining codec/API families. No optimization claims.

Compressed-unit iteration: full-item and indexed zlib/deflate data, tile-local
ranges, all icef field widths and overflow handling have independent pixel/error
coverage, codec-free and C-client sanitizer runs, and six new mutation checks.
Brotli and exhaustive resource behavior remain open. Sequence sample objects are
next, providing owned data/metadata for the still-missing track APIs.

Sequence sample iteration: sixteen missing exports now provide owned payloads,
durations, GIMI sample content IDs and version-aware timestamp ownership, plus
image duration/content-ID accessors. Independent cases, C-client sanitizers,
codec-free builds and semantic mutations cover the standalone object model.
Track storage/encoding and allocation-failure behavior remain open.

OMAF iteration: four projection APIs now distinguish mutable descriptions from
retained prfr properties, initialize descriptions from files, and propagate
projection metadata into decoded/derived/transformed images. Independent tests,
codec-free runs, C-client sanitizers and five mutations pass. Encoding integration
remains open.

Encoding-option iteration: ten additional APIs provide defaults, version-aware
copies/releases for still-image, sequence and uncompressed encoding options,
and orientation composition. Original-header field layouts, exact historical
allocation prefixes, sanitizers, codec-free runs and mutations are covered.
These objects do not substitute for the still-missing encoder/track machinery.

Region iteration: all thirty-six region entry points now have implementations.
The original-header corpus covers owned/retained geometry objects, ordered item
references, all seven geometry types, partial parsing, exact transformed values,
full inline/referenced-mask pixels, and context reload/free. Local regular,
codec-free and ASan/UBSan C-client runs agree on 2,396 cases. Writer serialization,
allocation-failure injection and cross-platform C validation remain required.
The strict completeness gate remains enabled while remaining families are built.

Entity-group iteration: both missing APIs now return independently owned arrays
with ordered/filterable IDs and exact null-versus-empty result behavior. The
1,196-case corpus covers all declared group kinds, supported typed parsers,
version bytes, truncation, duplicates, limits and snapshots across reload/free.
Original-header layouts, ASan/UBSan C clients, codec-free builds and five new
behavioral mutations pass. Writer integration remains open.

GIMI handle iteration: five missing APIs now implement owned content-ID strings,
shared mutable component-ID properties and read-only-context setter semantics.
File IDs propagate through decoded, derived and scaled images. Embedded NULs
retain presence separately from the visible C string; malformed property errors
preserve the native partial context. The 496-case independent corpus includes
unknown FourCC collision regressions, shared aliases, every byte value, sparse
indices, reload/free and component limits. Normal, codec-free and C-client
sanitizer runs agree. Encoder/plugin parameter and registry work is next.

### Encoder parameter records checkpoint

Five parameter-query APIs now pass a 2,020-case original-header corpus, including exact historical allocations, raw flags, signed counts, borrowed pointers, optional outputs and output aliasing. Normal, client ASan/UBSan and codec-free runs match; all five deliberate mutations are detected. Plugin public structures and nested parameter fields match original-header ABI layouts. Registry and encoder integration continue next. Full completion remains false: 357 partial functions and 108 missing.

### Plugin registries and encoder facade checkpoint

Thirty-nine APIs add registration, init/deinit, discovery and descriptor queries, encoder allocation/release, parameter validation and callback dispatch. A 224-case original-header corpus compares callback traces, historical record allocations, borrowed descriptors and context-sensitive errors. Normal, codec-free and client ASan/UBSan runs match; all twelve deliberate mutations are detected, as do context and parameter regressions. The implementation still needs registered-plugin image encode/decode integration, dynamic loading and file writing; discovery/configuration coverage alone does not establish those. The Rust HEVC decoder keeps its own implementation name. Current count: 396 partial APIs, 69 missing.

### File serialization and writer callbacks

Six APIs now serialize ordered metadata boxes, payload extents, properties and references, manage brands/ID mode and invoke caller/file writers. The 480-case independent original-header corpus matches exact bytes, repeated writes, read-back outcomes, callback errors and userdata, reentrant queries, historical writer prefixes, UUIDs and property-index boundaries. Normal, codec-free and ASan/UBSan clients pass. All six semantic mutations are detected; the default suite contains 137 defects. Image/sequence integration, compact image output, large offsets, namespace switching and region/text round trips remain open. Current count: 402 partial APIs, 63 missing; strict completion remains false.

### Image encoding and live file-model integration

Five APIs add mask/uncompressed image encoding, primary-image changes, thumbnails
and overlays. Exact BMFF and active decoded pixel bytes match 1,292 independent
original-header cases in regular, codec-free and ASan/UBSan client runs. Context,
writer, items, uncompressed pixels, derived, overlay and mask regressions also
match 10,264 cases. Freshly encoded uncompressed alpha queries and encoded TAI
ownership crash the native oracle in isolated ASan reproducers; they are recorded
separately and never counted as parity matches. Candidate safety probes pass.
External codec callback integration, other codecs, sensor configurations,
allocation failures and complete cross-platform behavior remain open. The strict
completion gate remains false at 407 partial APIs and 58 missing APIs. Continue
with tile queries/decode, grid/unci construction and sequence tracks.

All ten encoding mutations are detected by transcript mismatches (not crashes or compilation failures): encode_mask_stride 350, encode_orientation 115, encode_primary_flag 24, encode_profile_fallback 16, encode_unc_component_endian 230, encode_unc_compression_flag 280, encode_thumbnail_noop 12, encode_thumbnail_direction 14, encode_overlay_background 8, encode_repeated_extent 24. The default mutation suite contains 147 defects.


Tiling checkpoint: seven further APIs have Rust implementations, with 11,602
geometry/decode and 966 construction cases passing regular, sanitizer-client and
codec-free comparisons. Twelve new semantic mutations and the repaired historical
ID-allocation mutation are detected. Default mutation inventory: 159. The current
inventory is 414 partial functions and 51 missing; next are the 43 track/sequence,
three context input and five dynamic-plugin APIs, followed by the remaining
behavioral gaps and acceptance gates. Symbol coverage alone is not completion.


Sequence checkpoint: 43 track APIs added; 457 partial functions and eight missing.
Construction (892) and independent-file reading (212) pass normal, client
sanitizers and codec-free runs. Twelve sequence mutations added (171 total).
Continue file/reader/debug and dynamic plugin APIs, then close documented
behavior gaps; symbol presence never upgrades a partial record by itself.
