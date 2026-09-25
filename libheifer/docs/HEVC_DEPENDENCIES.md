# HEVC implementation dependencies

## HEVC decoding (rusty_h265)

The decoder is rusty_h265 0.6.0, vendored in `vendor/rusty_h265`; its SIMD
crate rusty_h265-accel comes from crates.io. See `CODECS.md`. It supports only 4:2:0
and 4:0:0 at 8 and 10 bits. It rejects range-extension streams (4:2:2, 4:4:4,
12-bit).

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
- the candidate decoder cannot decode the encoder's own 4:2:2, 4:4:4 and
  12-bit output (RExt); libde265 decodes it.
