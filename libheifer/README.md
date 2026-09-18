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
notices in the test submodule. See COPYING.

## Development validation

The inventory includes 465 functions, one public exported data object, and the
non-exported plugin convenience declarations. The default completion check fails
on any missing or unvalidated API. `--development` only permits incomplete
coverage; it still rejects stale claims, unknown exports and reference drift.

```sh
python -m pip install libclang==18.1.1
python tools/inventory.py --check
python tools/audit_dependencies.py
python tools/build_reference.py --hevc
cargo test --workspace --all-features
cargo build --release -p libheifer-capi
cargo build --release --features hevc --example hevc_probe
python tools/test_brands.py --reference-build .build/reference
python tools/test_images.py --reference-build .build/reference
python tools/test_color.py --reference-build .build/reference
python tools/test_hevc.py --reference-build .build/reference
python tools/test_mutations.py --reference-build .build/reference
python tools/check_coverage.py --reference .build/reference/libheif/libheif.so
```

The native reference build is test-only and requires a C/C++ toolchain and CMake.
It is never linked into libheifer. The Rust `hevc` feature enables an experimental
**direct-item, native YUV decoder**. It does not yet implement libheif default
color conversion, grids, transforms, alpha composition, all HEVC profiles or the
C decoding API. Do not use the experiment with untrusted images: codec resource
budgets and malformed-bitstream safety have not passed the final gate.

`test_hevc.py` reports both native-plane equality and the default-output gap.
`--require-default-output` makes that gap a hard failure. Neither native-plane
success nor a green Rust build is whole-library compatibility. CI's final
completion step is intentionally red while the contract remains incomplete.

For interleaved native-sample decode timing (not an end-to-end library claim):

```sh
cargo build --release --features hevc --example bench_hevc
python tools/bench_hevc.py --reference-build .build/reference
```
