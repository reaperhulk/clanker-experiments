# HEVC implementation dependencies

## HEVC decoding (rusty_h265)

The decoder is rusty_h265 0.6.0, vendored in `vendor/rusty_h265` because
libheif's HEVC alpha planes are monochrome: the only patch adds monochrome
(profile 4) decoding, which upstream rejects (`LIBHEIFER-PATCHES.md`). Its SIMD
crate rusty_h265-accel comes from crates.io. Two other differences from
libde265 are handled in `src/hevc.rs` around the unmodified API:

- an all-zero VUI colour description (upstream's value when none is present)
  is reported as unspecified;
- slices that refer to an unknown PPS are discarded.

It supports only 4:2:0 and 4:0:0 at 8 and 10 bits. It rejects range-extension
streams (4:2:2, 4:4:4, 12-bit).

## HEVC range-extension decoding (oxideav-h265)

Streams rusty_h265 rejects for their format go to
[oxideav-h265](https://crates.io/crates/oxideav-h265) 0.0.11 (MIT), used
unmodified from crates.io. `src/hevc.rs` routes a stream there when an SPS has
chroma_format_idc 2 or 3, a bit depth above 10, the range-extension profile
with colour, or an SPS or PPS range extension. Pictures above level 6.2 stay
with rusty_h265, which rejects them. The first output picture is used, cropped
to the conformance window.

- oxideav-h265 has no unsafe code, no build script and no native sources.
- It depends on oxideav-core 0.1.36 (MIT), whose non-optional serde_json pulls
  in serde_core, itoa and zmij. Their build scripts only probe the compiler
  version and are hash-reviewed in `dependency-build-scripts.json`.
  oxideav-core's unsafe code is in its frame arena, which the sequence decoder
  libheifer calls does not use.
- `tools/audit_dependencies.py` allowlists these versions, without features.

It decodes all 49 JCT-VC range-extension conformance streams to their
reference MD5 (`results/hevc-rext-conformance-report.json`), including the
extended-precision, 16-bit, cabac-bypass-alignment and 4:4:4 scaling-list
streams that libde265 decodes differently. It is slower than libde265: about
1 s per 1080p 4:2:2 picture.

## HEVC encoding (hpvca)

The built-in HEVC encoder is [hpvca](https://crates.io/crates/hpvca) 0.1.17,
used unmodified from crates.io with its default features. hpvca has no
dependencies and no build script, and its package contains no C, C++ or
assembly sources. It needs Rust 1.94, which is why the project's
`rust-version` is 1.94. `tools/audit_dependencies.py` allowlists exactly
this version and feature set (`avx`, `neon`).

- The `avx` and `neon` features select `core::arch` intrinsic kernels (SATD,
  transforms, adaptive quantization). On x86-64 they are chosen at run time
  with `is_x86_feature_detected!`, with scalar fallbacks.
- hpvca contains unsafe Rust: the intrinsic kernels, plus internal row
  parallelism that shares a frame buffer between threads. libheifer asks for
  a single thread (`ParallelismStrategy::Single`, `with_threads(1)`).
- Its public API produces a complete HEIC file. `src/hevc_encoder.rs` takes
  the VPS, SPS and PPS from that file's `hvcC` box and the slice NAL units
  from its item data. libheifer then writes its own file from these units, as
  libheif does with x265's packets.

hpvca is an independent all-intra encoder, not x265, so bitstreams and sizes
differ from libheif's. `crates/capi/src/builtin_hevc_encoder.rs` reproduces
libheif's x265 plugin interface:

- parameters (names, defaults, ranges and valid values, including `preset`,
  `tune`, `tu-intra-depth` and `complexity`, which hpvca does not otherwise
  use);
- input checks and bit depths (8, 10, 12);
- padding to even sizes of at least 64;
- the luma-sample limit;
- one packet per NAL unit, and priority 100.

Mappings:

- quality → a constant QP near x265's `CRF = (100 - quality) / 2`, minus one;
- presets `slow` and above → hpvca's slow search;
- tune `psnr` → hpvca's variance-boost adaptive quantization turned off;
- nclx → the SPS VUI colour description, as x265 writes it.

`x265:` pass-through options are rejected with a plugin error naming the
option.

Rate/distortion against x265 4.1 (libheif's plugin, default preset `slow` and
tune `ssim`) is measured by `tools/bench_hevc_encoding.py`. It uses 8 Kodak
images at qualities 10-95, decodes both outputs with libde265, and compares
luma PSNR (`results/hevc-rd-report.json`):

- mean BD-rate +5.6%, ranging from +2.5% to +8.4% per image;
- at equal quality, PSNR within 0.05-0.6 dB of x265 up to quality 70;
- above quality 80, hpvca's lowest QP (10) caps PSNR at about 48 dB where
  x265 reaches about 54 dB;
- single-threaded encoding takes about 2.8 times as long as x265 without
  assembly.

Known limitations:

- all-intra only, image sequences included, so sequence tracks are larger
  than x265's;
- monochrome streams signal the Main profile, so libheif chooses brand
  `heic` where x265's RExt monochrome profile gives `heix`;
- persistent Rice adaptation is turned off: with it, hpvca 0.1.17 writes
  lossless streams above 8 bits that libde265 and oxideav-h265 both find
  malformed.
