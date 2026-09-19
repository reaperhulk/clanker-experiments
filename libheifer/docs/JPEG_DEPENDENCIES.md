# JPEG implementation dependencies

The candidate vendors image-rs/jpeg-decoder 0.3.2 at
`eb2d7c0f6a2d0298aba7a7f8b9ca1440353e8f8c`. The published crate checksum is
`00810f1d8b74be64b13dbf3db89ac67740615d6c891f0e7b6179326533011a07`.
Only its Rust implementation, manifest and license/notices are included. The
optional Rayon default is disabled; `platform_independent` forbids unsafe Rust
and selects scalar implementations. There is no build script, assembly, C/C++
source, native decoder linkage or external decoder process. The dependency guard
requires exactly that resolved feature set. The core also forbids unsafe code.

The fork changes full-resolution integer IDCT precision, rounding and range
limiting; fancy chroma upsampling biases and narrow-component fallback; damaged
entropy recovery; sequential/progressive scan validation; and progressive block
smoothing. Smoothing is a Rust adaptation of the pinned native coefficient
controller and retains its IJG attribution and complete license. Other upstream
sources retain MIT/Apache-2.0 licensing. `LIBHEIFER_CHANGES.md` in the vendor tree
records the changes. This software is based in part on the work of the
Independent JPEG Group.

The safe adapter combines optional jpgC bytes with the image item's owned input,
checks dimensions and allocation limits, and copies decoded components into
owned monochrome or 4:2:0 planes. As in the native plugin, color JPEG output keeps
chroma from even rows and columns of the fully upsampled JPEG. RGB JPEG color
conversion rejection and lossless precision handling are separately tested.
The item retains one compressed-input reservation across description/decode
transitions, replacing it on subsequent decoding. Native codec heap accounting
and libheif plane/input accounting are distinct.

`tools/build_reference.py --jpeg` independently builds libheif at
`4e14f5942c1732ace9611b9522cc991501445463` with libjpeg-turbo 3.1.1 at
`7723f50f3f66b9da74376e6d8badb6162464212c`, with native SIMD disabled. These native
libraries are test-only. Native cjpeg encodes owned synthetic patterns into the
committed fixture manifest; ordinary comparisons require no installed encoder.
Expected errors, metadata, callbacks and complete active pixels come from a
separate original-header C client linked to this oracle. The candidate never
produces expected output. Reports record binary, client and fixture hashes.

The default C adapter includes the optional `jpeg` feature. Disabling all default
features excludes this codec and is compared against a JPEG-disabled oracle.
Local ASan/UBSan client runs disable LeakSanitizer under ptrace; CI retains leak
checking. The linked Linux candidate depends only on libc, libgcc_s and the ELF
loader. Symbol inspection finds no foreign JPEG/AV1/HEVC codec entry points.

This is finite compatibility evidence, not complete JPEG or libheif conformance.
Arithmetic JPEG decoding, built-in JPEG encoding, broader restart/corruption and
sequence behavior, cross-platform C clients and downstream/performance gates
remain open. Strict project completion remains false.
