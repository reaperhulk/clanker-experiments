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
