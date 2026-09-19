# libheifer compatibility patches

Source: crates.io rusty_h265 0.6.0; original Apache-2.0 license retained.
The original registry checksum is retained in the history of Cargo.lock.

- Set absent VUI colour-description fields to H.265's unspecified value (2).
  Zero incorrectly selects identity-matrix RGB when exposing bitstream NCLX.
  This change preserves codec math; it corrects metadata exposed by libheifer.

Monochrome syntax is implemented for profile 4 streams with no extension tools:
no chroma prediction-mode or coded-block flags are consumed; chroma reconstruction,
PCM samples, motion compensation and deblocking are omitted. Extension-tool flags
remain checked and unsupported tools are still rejected. The independent alpha
fixture exercises this path; broader monochrome conformance remains required.

Parameter-set lookup failures now have a typed `MissingParameterSet` error (with
the previous Display text). The libheifer adapter can discard such slices and
continue accepting subsequent NAL units, matching libde265 stream behavior.
