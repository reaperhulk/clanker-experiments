# libheifer patches to brotli-decompressor 6.0.1

libheif reports brotli failures with `BrotliDecoderErrorString` of the error
code, so the decoder must fail with the same code as the C decoder that the
native oracle links (google/brotli v1.1.0, ed738e84). Two changes in
`src/decode.rs`:

1. A metadata or uncompressed metablock whose header is not followed by zero
   padding bits fails with `ERROR_FORMAT_PADDING_1` (C `decode.c`), not
   `PADDING_2`, which C reserves for the padding after the last metablock.
2. `BrotliAllocateRingBuffer` sizes and grows the ring buffer per metablock
   like C's `BrotliCalculateRingBufferSize` and `BrotliEnsureRingBuffer`,
   instead of sizing it once from the first metablock. The ring-buffer size
   decides when the decoder flushes, so an overlong command fails with
   `BLOCK_LENGTH_1` (flush) or `BLOCK_LENGTH_2` (end of metablock) exactly
   where C does. The `canny_ring_buffer` unit test expects C's 1024-byte
   minimum; `ring_buffer_grows_with_output` covers the growth.

Differential fuzzing of truncated, bit-flipped, overwritten, extended and
random streams (C-encoded at qualities 0-11 and windows 10-24, 1-300 byte and
256 KiB output buffers) against C 1.1.0 finds no difference in result, error
code, output or output chunking.

The crate's `scripts/` directory (a C fuzzing harness and corpus tools, not
part of the build) is removed from the vendored tree.
