# libheifer patches to brotli 9.0.0

libheif compresses metadata items and `brot` unci data with the C brotli
encoder at its defaults (quality 11, window 22, BROTLI_OPERATION_FINISH on the
whole input; `compress_brotli` in libheif). The native oracle links
google/brotli v1.1.0 (ed738e84). These changes make the Rust encoder emit the
same bytes on that path:

1. **Floating-point types as in C** (`src/enc/util.rs`, `log_table_8.rs`,
   `backward_references/hq.rs`, `hash_to_binary_tree.rs`, `literal_cost.rs`,
   `combined_alloc.rs`, `backward_references/mod.rs`). C computes entropy,
   histogram, clustering and block-splitting costs in `double` and only the
   zopfli cost model and the literal cost estimate in `float`. The crate used
   `f32` everywhere. `floatX` is now `f64`; the zopfli model (`hq.rs`, the
   `ZopfliNode` cost) and literal costs are `f32`, with the same double
   computations cast to float where C casts. `FastLog2` is C's: the 256-entry
   table holds float literals widened to double (C's `kBrotliLog2Table` values
   carry an `f` suffix), larger values use `f64::log2`. The zopfli model's
   `f32` allocator is added to `BrotliAlloc`; `CombiningAllocator` no longer
   implements it.
2. **Block splitter** (`block_splitter.rs`): `FindBlocks` is C's scalar
   double loop instead of an 8-lane `f32` one, and quality 11 runs C's 10
   refinement iterations (`quality < 11 ? 3 : 10`), not 3.
3. **Population cost** (`bit_cost.rs`): `log2` of histogram counts uses
   `FastLog2`, as in C, instead of a 16-bit table indexed by the count
   truncated to `u16`.
4. **H10 stitching** (`backward_references/hq.rs`): `StitchToPreviousBlockH10`
   starts at `position - MAX_TREE_COMP_LENGTH + 1`, as in C.
5. **Literal context mode** (`encode.rs`): `ChooseContextMode` reads the ring
   buffer data, not the allocation that starts two bytes earlier.
6. The Rust-only context-mixing helpers (`prior_eval.rs`, `vectorization.rs`)
   keep their `f32` arithmetic; they are off by default.

The binaries, examples and FFI module (`src/bin`, `examples`, `src/ffi`) are
removed from the vendored tree.

Differential testing against C 1.1.0 at the default settings (random, image,
16-bit, text, UTF-8, repeated and mixed inputs from empty to 1.5 MB) finds no
difference in the compressed bytes.
