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
"Pure Rust" means no C, C++ or assembly linked into the candidate; Rust SIMD is
allowed. Prefer fearless_simd (safe `core::arch` wrappers with runtime dispatch);
`unsafe` intrinsics are acceptable only where they give a major measured gain,
with documented safety contracts and a scalar Rust oracle compared in tests.
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
465 partial functions, with no functions missing. Compatibility work comes first;
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

Six APIs now serialize ordered metadata boxes, payload extents, properties and references, manage brands/ID mode and invoke caller/file writers. The 480-case independent original-header corpus matches exact bytes, repeated writes, read-back outcomes, callback errors and userdata, reentrant queries, historical writer prefixes, UUIDs and property-index boundaries. Normal, codec-free and ASan/UBSan clients pass. All six semantic mutations are detected; the default suite contains 137 defects. Image/sequence integration, compact image output, large offsets, namespace switching and region/text round trips remain open. Current count: 402 partial APIs, 61 missing; strict completion remains false.

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


Dynamic module checkpoint: five loader/path APIs added. 462 partial functions,
three missing. A separate plugin-enabled oracle validates 425 cases, including
stable stderr diagnostics; ASan/UBSan clients and codec-free runs pass. Unpinned
module safety probes are separate from native parity. Eight loader mutations
bring the inventory to 179. Continue file/reader input and debug dump, then
remaining behavior, codec, resource and platform gates.


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


### Rust AV1 decoding and minimized image containers

Built-in AV1 decoding now uses the pinned rav1d 1.1.0 Rust sources with assembly,
native build tooling and exported compatibility symbols removed. The owned safe
adapter transfers 8/10/12-bit monochrome, 4:2:0, 4:2:2, 4:4:4 and RGB planes into
the existing Rust transform pipeline. AV1 configuration upload, missing-decoder
errors, color disagreement warnings, warning persistence and optional full-range
correction match the independent native dav1d oracle for the recorded corpus.
Dependency review and reproducible fixture provenance are recorded separately.

The minimized `mini` reader now expands metadata while retaining physical media
offsets and input ownership. It handles alpha, Exif/XMP, ICC/NCLX, HDR properties,
orientation, compact field widths and native global/context limit distinctions.
Callback readers preserve partial item state after configuration errors, native
timeout behavior and payload allocation checks before reading. Compact diagnostics
and writing remain open, as do wider AV1 configuration/bitstream conformance and
registered/built-in decoder cache interactions.

Independent original-header comparisons pass: AV1 1,400; malformed AV1 540;
resource boundaries 3,089; compact context 1,990; compact properties 1,990;
compact files 1,031; compact callbacks 4,655. Normal and ASan/UBSan client runs
pass; codec-free AV1 and compact context/property/file/reader runs also pass.
Local sanitizer reports explicitly disable leak checking under ptrace; CI retains
it. Regression checks cover context (1,786), callbacks (4,712), registered decoders
(616) and HEVC pixels (115). Rust all-feature/no-feature tests, Clippy, formatting,
original-header ABI, dependency guard and development inventory checks pass.
Reports retain exact client/corpus/library hashes in `docs/results/av1-*.json`
and `docs/results/mini-*.json`; they describe the tested build for each step.

All 14 new mutations are detected without process failures. The initial alpha
configuration mutation survived the generic metadata client; the retained failed
report documents that gap, and a new original-header property-query client detects
86 differences. Follow-up reports also detect HDR, context-limit, partial-state,
timeout and payload-budget defects. The default mutation inventory is now 222.

All 465 function exports still have partial status. The strict full-completion
gate remains false. AV1 encoding, other codec replacements, sequence/encoder
plugin integration, exhaustive behavior and platform/downstream/performance gates
remain open; implementation continues rather than treating finite corpus parity
or symbol presence as full compatibility.


### Minimized box diagnostics

Compact inputs now retain their original mini diagnostic fields and physical
chunk offsets. Dumps preserve native spelling, raw FourCC bytes, inherited
configuration sizes, signed HDR primaries, gain-map metadata and successfully
parsed boxes after unsupported-brand expansion failures. The independent
original-header client compares exact bytes, repeated calls, descriptor ownership,
memory/file reads and failed reloads across 1,372 cases. Normal, ASan/UBSan client
and codec-free runs pass. Four mutations are detected by 424, 296, three and 424
semantic differences, with no process failures (226 default mutations).
Compact context/property regressions (1,990 each), ordinary diagnostics (1,729),
Rust all-feature/no-feature tests, Clippy, formatting and original-header ABI pass.
Evidence and exact artifact hashes are in `docs/results/mini-debug-*.json`.
Local leak detection remains disabled under ptrace; CI retains it. All 465
functions remain partial. Compact output requires the still-image encoder path;
registered encoder integration and the other codec/behavioral gates continue.


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

### Registered VVC encoder checkpoint

VVC encoder callbacks now create profile/tier/level and sublayer configuration,
parameter-set arrays, framed media data, image brands and ordinary image/alpha
output. Fresh partial SPS records replace previous configuration just as the
pinned reference does. All 1,509 independent original-header callback/file cases
match in regular, ASan/UBSan-client and codec-free runs, including full/partial
SPS fields, duplicate arrays, 65,535/65,536 NAL counts and byte lengths, and
partial writer output. Native invalid multi-layer SPS paths that index absent
vectors remain excluded from parity evidence. Built-in VVC coding, reader/decoder
configuration, sequence integration and the other mandatory gates remain open;
all functions remain partial and strict completion remains false.

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


### Built-in scalar Rust AVC decoding

The optional `avc` feature (default in the C adapter) decodes `avc1` items with
vendored rusty_h264 0.16.0, built `no_std` without accel kernels, a global
allocator, environment knobs or threads. Reviewed patches follow libheif's only
AVC decoder, OpenH264: monochrome syntax with flat chroma, four-sided two-sample
cropping, the CAVLC `level_prefix` limit and CABAC end-of-data failures. The
adapter reproduces the OpenH264 plugin's length-prefixed conversion (including
its start-code emulation handling), silent rejection of unsupported SPS profiles,
I420 output, error texts, priority 70, libheif's ispe+16 pixel limit, SPS
coded-size checks and missing-avcC handle errors.

A pinned test-only x264 generates 371 owned streams covering profiles, sizes and
cropping, QP sweeps, scaling matrices, deblocking, slices, presets, encoder
options, monochrome and formats OpenH264 rejects. All 9,475 cases (25 modes) match
a pinned OpenH264 2.6.0/libheif 1.23.4 oracle in normal, ASan/UBSan-client and
codec-free builds. The 86-suite normal group and AVC-oracle plugin regressions
pass. Five new mutations are detected (303 default); an initial CBP-table
mutation survived because the corpus had one monochrome stream, and is retained
with its replacement after adding 72 monochrome fixtures.

Malformed-stream parity is not complete: the 35,895-case truncation/corruption
corpus records 975 differences (first run: 17,679), mostly where OpenH264's own
NAL splitting and SPS/VUI/PPS/slice-header validation reject streams the Rust
decoder accepts. It is excluded from CI and from parity counts until it matches.
CI now builds and caches the OpenH264 oracle and runs the AVC suites.

Next: an exact port of OpenH264's syntax layer ahead of reconstruction, then
CABAC I_PCM, registered-plugin priority cases, limits and AVC sequences. All 465
functions remain partial; strict completion remains false.

### AVC syntax layer and Rust SIMD

`src/avc_openh264.rs` now models OpenH264 ahead of reconstruction. It covers
Annex B splitting and unescaping, the 32-bit-cache bit reader and its bounded
over-reads, NAL header checks, and SPS/VUI/HRD, PPS and slice-header acceptance
with OpenH264 2.6.0's quirks. It also decides when an access unit is
constructed, which settles whether an incomplete picture is an error or yields
no image. The vendored decoder follows OpenH264's reconstruction rules: CABAC
and CAVLC end-of-data, `coeff_token` fallbacks, intra-mode availability,
separate Cb/Cr QP offsets, macroblock-count completeness, and uint16 scaling
factors and int16 coefficient storage. Only accepted units reach it, as RBSP.
The malformed-stream corpus went from 975 differences to 0. It is now a parity
suite in CI, with an HRD family that reaches OpenH264's error-code loop.

The x264 corpus has 427 streams. x264's constant-QP I-frames are coded about 3
below `--qp`, so the original sweep never reached QP 49..51. Exact
`--ipratio 1` streams now cover that range, including OpenH264's uninitialized
QP 51 scaling row. All 10,875 valid-stream cases and 37,047 malformed-stream
cases match in normal, ASan/UBSan-client and codec-free builds. Twelve new
mutations bring the total to 315. The first run missed two:
`avc_hrd_return_code` and `avc_scaling_qp51`. The added fixtures now catch both,
and both reports are retained.

Per the clarified requirement (Rust SIMD allowed, no C/C++/assembly),
deblocking uses fearless_simd kernels with runtime dispatch and no `unsafe`,
with the scalar filters kept as the test oracle. A still-image decode skips the
redundant re-escape pass and the final picture's reference copy. On 1920x1080
streams, libheifer takes 0.67-0.90x the time of libheif with OpenH264's
assembly build (0.41-0.83x of its scalar build); before this it took up to
1.48x. See `docs/AVC_DEPENDENCIES.md`. `test_decode`-based suites now run cases
in parallel. CI runs the vendored crates' tests on Linux, macOS and Windows.

Registered AVC plugins competing with the built-in decoder now match libheif
(480 cases): address-ordered priority ties, per-item decoder reuse including
built-in selections, and avcC units plus the coded-size check on pushed data.
See `docs/RESULTS.md`.

AVC pixel and memory limits at read and decode match on 2,527 cases, including
items whose declared ispe is smaller than the coded picture.

I_PCM streams in both entropy modes match, after reproducing OpenH264's
monochrome chroma behaviour: I_PCM chroma bytes are kept, and DC prediction
assumes both neighbours.

Next: AVC sequences, then remaining plan gates. All 465 functions remain partial; strict completion
remains false.
