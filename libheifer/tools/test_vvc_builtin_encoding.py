#!/usr/bin/env python3
"""Original-header built-in VVC encoding against libheif with its vvenc plugin: errors, parameters,
handles and non-codec boxes exactly; VVC configuration fields; candidate files decoded by vvdec."""
import re
import struct

import test_hevc_builtin_encoding as harness

# tests/encoding.c parameter sets: 31 and 77 set lossless, 74 and 75 are
# out-of-range qualities, 58, 70 and 73 are unknown to vvenc's plugin, and
# 1 and 45 are unknown names.
PARAMETER_SETS = [1, 31, 45, 58, 70, 73, 74, 75, 77]


def corpus():
    cases = []

    def add(v):
        cases.append(('-'.join(map(str, v)), struct.pack('=8I', *(x & 0xffffffff for x in v))))

    lossless = 16384
    for width, height in [(1, 1), (2, 3), (7, 5), (31, 40), (64, 48), (97, 70), (130, 66), (200, 136)]:
        for space, chroma in [(0, 1), (0, 2), (0, 3), (1, 3), (1, 10), (2, 0)]:
            add([5, width, height, space, chroma, 8, 1, 8])
    for quality in [0, 1, 25, 50, 75, 99, 100]:
        add([5 | (quality + 1) << 16, 64, 48, 0, 1, 8, 1, 8])
        add([5 | (quality + 1) << 16, 48, 40, 2, 0, 8, 1, 8])
        add([5 | (quality + 1) << 16, 40, 32, 0, 3, 8, 1, 8])
    for mode in [lossless, 2048, 32768, lossless | 2048]:
        add([5 | mode, 64, 48, 0, 1, 8, 1, 8])
        add([5 | mode, 40, 36, 0, 3, 8, 1, 8])
    for depth in [7, 8, 9, 10, 12, 16]:
        for space, chroma in [(0, 1), (2, 0), (0, 3)]:
            add([5, 40, 36, space, chroma, depth, 1, 8])
    for index in PARAMETER_SETS:
        add([5 | index << 24, 70, 50, 0, 1, 8, 1, 8])
    # Metadata (nclx, HDR, ICC), orientations, thumbnails, overlays, alpha and repeated encodes.
    for flags in [1024, 2048, 3072, 4096, 8192, 16384 | 3072, 32768 | 1024]:
        for orientation in [1, 6]:
            add([5, 48, 36, 0, 1, 8, orientation, 8 | flags])
    for mode in [256, 1024, 4096, 8192]:
        add([5 | mode, 48, 36, 0, 1, 8, 1, 8])
    for space, chroma in [(0, 1), (1, 3), (2, 0)]:
        add([5, 48, 36, space, chroma, 8, 1, 8 | 512])
    for bbox in [1, 2, 6, 16]:
        add([5 | 512, 64, 40, 0, 1, 8, bbox, 8])
    return cases, b''.join(data for _, data in cases)


CONFIG = re.compile(r'vvcC\([^)]*\)')


def classify(reference, candidate):
    """A named, visible known difference, or None for a mismatch."""
    return None


NAMES = (('libheifer-vvc', 'vvenc'),)


def fallback(reference, candidate):
    """Classify a differing transcript line of another VVC encoding suite in
    which libheif falls back to its default VVC encoder: vvenc in the oracle,
    libheifer's in the candidate (see harness.fallback)."""
    r, _ = harness.transcript(reference, NAMES)
    c, _ = harness.transcript(candidate, NAMES)
    if r == c:
        # Equal once files are normalized and the encoder names swapped.
        return 'builtin-vvc-fallback'
    return classify(r, c)


def main():
    harness.corpus = corpus
    harness.classify = classify
    harness.NAMES = NAMES
    harness.LISTING_FORMAT = 5
    harness.DEFAULTS = ('.build/reference-vvc', '.build/vvc-builtin-encoding-report.json', '.build/vvc-builtin-encoding')
    harness.DECODE_KNOWN = 'vvc-decoding'
    harness.__doc__ = __doc__
    harness.main()


if __name__ == '__main__':
    main()
