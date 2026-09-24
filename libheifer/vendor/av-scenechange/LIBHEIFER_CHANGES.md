# Rust-only av-scenechange (rav1e dependency)

Upstream: rust-av/av-scenechange 0.14.1 (crates.io), MIT.

Changes for libheifer: the assembly and C sources are deleted; `build.rs`
keeps only upstream's environment steps (the nasm/cc build and the `asm`,
`cc`, `nasm-rs` and `libc` features are removed); the binary and the
ffmpeg, vapoursynth, devel, tracing and serialization features, and their
source files (`main.rs`, `ffmpeg.rs`, `vapoursynth.rs`), are removed. No
remaining Rust source is changed. rav1e depends on it with default features
off, as does the native oracle's rav1e build.
