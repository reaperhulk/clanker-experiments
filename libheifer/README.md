# libheifer

An independent Rust implementation of the libheif API, with an optional C ABI.
**Work in progress: this is not yet a complete or drop-in libheif replacement.**
See [PLAN.md](PLAN.md) for the complete scope and acceptance criteria.

The default crate has no C ABI exports and no libheif dependency. The separate
`libheifer-capi` crate exports compatible `heif_*` entry points. All implementation dependencies, including codecs, must be pure Rust. Native
codec libraries, wrappers and forwarding to libheif are prohibited.
The pinned upstream submodule is used only for tests, headers and reference builds.

The compatibility reference is libheif **1.23.4**, commit
`4e14f5942c1732ace9611b9522cc991501445463`. Older version compatibility requires
separate header/client tests, not merely the presence of deprecated functions.

```sh
git submodule update --init --recursive
cd libheifer
cargo test --workspace
cargo build --release -p libheifer-capi
```

The interim shared library is deliberately named `libheifer`, so an unfinished
build cannot accidentally replace a system libheif installation. Release packaging
will provide the platform's libheif SONAME/install-name only after its full gate passes.

## Licensing

LGPL-3.0-or-later. Compatibility work refers to and adapts libheif, Copyright
Dirk Farin and the libheif contributors. Upstream source and headers retain their
notices in the test submodule. See COPYING. The default DEFLATE encoder adapts
zlib 1.3 algorithms under its zlib license (see `licenses/zlib.txt`); the pure Rust
inflater retains its license in `vendor/zlib-rs/LICENSE`.
The Rust AV1 decoder retains rav1d's BSD-2-Clause notices in `vendor/rav1d`.

## Development validation

The inventory includes 465 functions, one public exported data object, and the
non-exported plugin convenience declarations. The default completion check fails
on any missing or unvalidated API. `--development` only permits incomplete
coverage; it still rejects stale claims, unknown exports and reference drift.

```sh
python -m pip install libclang==18.1.1 meson==1.12.0 ninja==1.13.2
python tools/inventory.py --check
python tools/audit_dependencies.py
python tools/build_reference.py --hevc
python tools/build_reference.py --av1 --build .build/reference-av1
cargo test --workspace --all-features
cargo build --release -p libheifer-capi
cargo build --release --features hevc --example hevc_probe
python tools/test_brands.py --reference-build .build/reference
python tools/test_brand_boxes.py --reference-build .build/reference
python tools/test_images.py --reference-build .build/reference
python tools/test_color.py --reference-build .build/reference
python tools/test_context.py --reference-build .build/reference
python tools/test_av1.py --reference-build .build/reference-av1
python tools/test_av1_errors.py --reference-build .build/reference-av1
python tools/test_av1_limits.py --reference-build .build/reference-av1
python tools/test_mini.py --reference-build .build/reference
python tools/test_mini_properties.py --reference-build .build/reference
python tools/test_mini_file.py --reference-build .build/reference
python tools/test_mini_reader.py --reference-build .build/reference-av1
python tools/test_items.py --reference-build .build/reference
python tools/test_add_metadata.py --reference-build .build/reference
python tools/test_text.py --reference-build .build/reference
python tools/test_uncompressed_config.py --reference-build .build/reference
python tools/test_uncompressed_pixels.py --reference-build .build/reference
python tools/test_handle_color.py --reference-build .build/reference
python tools/test_image_area.py --reference-build .build/reference
python tools/test_metadata_compression.py --reference-build .build/reference
python tools/test_auxiliary.py --reference-build .build/reference
python tools/test_properties.py --reference-build .build/reference
python tools/test_decode_properties.py --reference-build .build/reference
python tools/test_camera.py --reference-build .build/reference
python tools/test_sensor.py --reference-build .build/reference
python tools/test_tai.py --reference-build .build/reference
python tools/test_components.py --reference-build .build/reference
python tools/test_component_handles.py --reference-build .build/reference
python tools/test_context.py --reference-build .build/reference --sanitize --output .build/context-sanitized-report.json
python tools/test_decoding_options.py --reference-build .build/reference
python tools/test_transforms.py --reference-build .build/reference
python tools/test_decode.py --reference-build .build/reference
python tools/test_decode_geometry.py --reference-build .build/reference
python tools/test_decode_derived.py --reference-build .build/reference
python tools/test_decode_overlay.py --reference-build .build/reference
python tools/test_decode_mask.py --reference-build .build/reference
python tools/test_decode_graphs.py --reference-build .build/reference
python tools/test_derived_handles.py --reference-build .build/reference
python tools/test_security.py --reference-build .build/reference
python tools/test_security_lifetimes.py --reference-build .build/reference
python tools/test_warnings.py --reference-build .build/reference
python tools/test_hevc.py --reference-build .build/reference --require-default-output
python tools/test_mutations.py --reference-build .build/reference
python tools/check_coverage.py --reference .build/reference/libheif/libheif.so
```

The native reference build is test-only and requires a C/C++ toolchain and CMake.
It is never linked into libheifer. The Rust `hevc` feature enables direct-item
HEVC decoding; the separate C ABI package enables it and the Rust `av1` feature by default. The HEVC decoder is
vendored with a documented VUI default-value fix, retaining its Apache-2.0 license.
The C API also handles native alpha, rotation/mirroring, YCbCr/RGB conversion,
8/16-bit RGB packing, image crop/scale, grid, overlay and identity derivations, raw masks, decoding
warnings, context thread controls and versioned decoding options.
AV1 uses vendored rav1d without its native assembly/build dependencies. Compact
`mini` files are expanded into the image model while media reads retain the
original source. See [docs/AV1_DEPENDENCIES.md](docs/AV1_DEPENDENCIES.md) for the
dependency review and [PLAN.md](PLAN.md) for finite validation and open gates.

The current decode differential checks 23 modes on five fixtures, including all
visible alpha samples, profiles and error outputs. Generated crop/scale cases
cover odd sizes and 8/10/12/16-bit planes. These finite checks do not establish
whole-library compatibility: complete derived-image and transform coverage, all conversion
operators, codec conformance, resource budgets and other codecs remain unfinished.
CI's final completion step remains red until the full contract is validated.

For interleaved native-sample decode timing (not an end-to-end library claim):

```sh
python tools/bench_hevc.py --reference-build .build/reference
```

The benchmark command builds its probe and timing client together in an isolated
Cargo target, verifies fresh exact native-plane output, and records source and
binary hashes. It rejects input/source changes during timing. An earlier passing
report is never accepted as evidence for a changed decoder.

The context/handle subset supports copied and borrowed memory, direct HEVC item
queries, metadata, thumbnails and color profiles. Its independent tests cover
malformed properties, reload failures, output sentinels and handles that outlive
the caller's context. Other image types, compressed metadata, file/reader callbacks,
budgets for remaining formats and full decoding orchestration remain unfinished.
Versioned security-limit APIs and safe plane allocation now track image/decoder/metadata
memory, including old handles across context reloads.
`--sanitize` instruments the C test clients, not the Rust or reference libraries;
use `--no-leak-check` only where LeakSanitizer cannot run (for example under ptrace).

Item properties support raw/UUID data, user descriptions, transform queries and
in-memory insertion. File property tables follow partial-read and reload state
independently of retained image handles; optional description errors propagate
as decode warnings. The new public description layout is checked against the
original header. These APIs remain partial while other property classes and
serialization are implemented.

Camera intrinsic/extrinsic queries parse standard and legacy UUID properties,
apply the pinned crop/mirror behavior, and return independently owned extrinsic
matrices. Tests compare exact floating-point bits, parse warnings and lifetimes.

Imaging metadata APIs store Bayer patterns, polarization filters, bad-pixel maps,
non-uniformity correction and chroma location with copied arrays and exact float
bits. Their in-memory behavior is tested independently; complete uncompressed
file-property parsing, encoding and multi-component codec paths remain open.

Decoded-image components have independent IDs, types, datatypes and aligned
storage, including reference-only entries and duplicate channels. Typed access
returns element strides; raw access returns byte strides. Handle descriptions preserve parse order, and decoded IDs are reconciled before
output conversion. Multi-component codec paths and content-ID serialization remain unfinished.

JPEG header descriptions support SOF scanning across jpgC/item data; JPEG pixel
decoding and complete dynamic read-limit recovery remain unfinished.

Uncompressed image initialization and pixel decoding support the reference's
component, pixel, mixed, row, tile-component, byte-aligned and block layouts.
Independent C clients compare complete pixels, component IDs and types, byte order,
alignment, malformed inputs, conversion and handle lifetimes. These paths require
no optional codec feature. Generic compressed units, sensor file properties,
partial-range resource accounting and uncompressed encoding remain open.
