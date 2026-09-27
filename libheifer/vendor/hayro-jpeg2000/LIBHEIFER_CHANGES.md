# libheifer integration changes

Based on hayro-jpeg2000 0.4.0, hayro revision
`a728f7cb6826c1e167973b8fd71867ebb4cf39af`.

* Standalone scalar std-only manifest; optional dependencies and workspace links
  are removed. The scalar implementation compiles with Rust 1.90.
* A raw component decoding entry point bypasses unsigned level shifting, leaving
  final native-compatible rounding and signed storage to the libheifer adapter.
* Irreversible high-band normalization uses OpenJPEG's historical 1.625732422
  scale; irreversible dequantization does not add the reversible subband gain.
  The adapted normalization retains OpenJPEG attribution in LICENSE-OPENJPEG.
* Decoded components track present tile rectangles. Declared tiles with no tile
  parts are skipped so the adapter preserves zero-filled missing regions.

* RawCodestream parses headers without JP2 color-space/component-count assumptions.
* Inverse wavelets use each tile's component parameters. Multi-component
  transforms operate only on the corresponding tile's stored sample rectangle,
  mapping common subsampling and resolution shrink factors; scalar tails restore
  samples after the last complete eight-lane group.
* Declared packet body segments exceeding the available bytes are rejected
  independently of optional strict packet-header padding checks.

All other upstream licenses and profile notices are retained. Original-header
native oracle comparisons, fixture provenance, deliberate mutations and sanitizer
results are recorded by the enclosing libheifer project. These changes do not
claim full JPEG2000 or HTJ2K conformance.

## HTJ2K (ITU-T T.814) code-blocks

- `j2c/ht.rs` ports OpenJPEG's `ht_dec.c` block decoder (cleanup, SigProp and
  MagRef passes, its malformed-block checks and the address-aligned initial
  MEL reads); `j2c/ht_luts.rs` holds its VLC tables.
- HT code-blocks follow OpenJPEG's segment assignment
  (`opj_t2_read_packet_header`/`opj_t2_read_packet_data`): the first segment
  takes one pass per packet, the zero bit-plane count is the tag-tree value
  plus one, and T1 output is halved (5/3) or scaled by half the step (9/7).
- CAP and CPF main-header markers are skipped; mixed HT code-block style
  (0x80) is rejected; a main-header RGN shift fails HT decoding.

## Single-sample 9/7 synthesis and deep decompositions

- A lone odd sample in a row or column is halved only for the 5/3 wavelet;
  OpenJPEG's 9/7 synthesis (`opj_v8dwt_decode`) returns without scaling it.
- Precinct steps on the reference grid are computed in 64-bit arithmetic, as
  in OpenJPEG's packet iterator (`pi.c`), so position-driven progressions
  with more than 16 decomposition levels are no longer rejected.
