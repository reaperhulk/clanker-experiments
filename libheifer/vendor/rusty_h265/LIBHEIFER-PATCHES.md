# libheifer compatibility patches

Source: crates.io rusty_h265 0.6.0; original Apache-2.0 license retained.
The original registry checksum is retained in the history of Cargo.lock.

- Set absent VUI colour-description fields to H.265's unspecified value (2).
  Zero incorrectly selects identity-matrix RGB when exposing bitstream NCLX.
  This change preserves codec math; it corrects metadata exposed by libheifer.
