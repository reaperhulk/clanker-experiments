# libheifer compatibility patches

Source: crates.io rusty_h265 0.6.0; original Apache-2.0 license retained.
The original registry checksum is retained in the history of Cargo.lock.

Monochrome syntax is implemented for profile 4 streams with no extension tools:
no chroma prediction-mode or coded-block flags are consumed; chroma reconstruction,
PCM samples, motion compensation and deblocking are omitted. Extension-tool flags
remain checked and unsupported tools are still rejected. The independent alpha
fixture exercises this path; broader monochrome conformance remains required.

Only the monochrome decoding path is patched. libheifer handles the other
differences from libde265 around the unmodified API: an all-zero VUI colour
description is reported as unspecified (2), and slices that refer to an unknown
PPS (the decoder's "slice refers to unknown PPS" error) are discarded.
