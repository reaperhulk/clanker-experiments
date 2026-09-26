#!/usr/bin/env python3
"""Original-header built-in AVC encoding against libheif with its x264 plugin: errors, parameters,
handles and non-codec boxes exactly; AVC configuration fields; candidate files decoded by OpenH264
(which, like libheif's OpenH264 decoder, fails on the High 10, 4:2:2 and 4:4:4 files of both)."""
import struct

import test_hevc_builtin_encoding as harness

# tests/encoding.c parameter sets 58-77 exercise the x265/x264 plugin parameters
# (the x265: ones are unknown names here), 78-80 are x264: options; 1, 31 and 45
# are unknown names.
PARAMETER_SETS = list(range(58, 81)) + [1, 31, 45]


def corpus():
    cases = []

    def add(v):
        cases.append(('-'.join(map(str, v)), struct.pack('=8I', *(x & 0xffffffff for x in v))))

    lossless = 16384
    # 192x144 (108 MBs) is the smallest level 1.1 picture.
    for width, height in [(1, 1), (2, 3), (7, 5), (31, 40), (64, 48), (97, 70), (130, 66), (192, 144), (400, 300)]:
        for space, chroma in [(0, 1), (0, 2), (0, 3), (1, 3), (1, 10), (2, 0)]:
            add([2, width, height, space, chroma, 8, 1, 8])
    for quality in [0, 1, 25, 50, 75, 99, 100]:
        add([2 | (quality + 1) << 16, 64, 48, 0, 1, 8, 1, 8])
        add([2 | (quality + 1) << 16, 48, 40, 2, 0, 8, 1, 8])
    for mode in [lossless, 2048, 32768, lossless | 2048]:
        add([2 | mode, 64, 48, 0, 1, 8, 1, 8])
        add([2 | mode, 40, 36, 0, 3, 8, 1, 8])
    for depth in [7, 8, 9, 10, 12, 16]:
        for space, chroma in [(0, 1), (2, 0), (0, 3)]:
            add([2, 40, 36, space, chroma, depth, 1, 8])
    for index in PARAMETER_SETS:
        add([2 | index << 24, 70, 50, 0, 1, 8, 1, 8])
    # Metadata (nclx, HDR, ICC), orientations, thumbnails, overlays, alpha and repeated encodes.
    for flags in [1024, 2048, 3072, 4096, 8192, 16384 | 3072, 32768 | 1024]:
        for orientation in [1, 6]:
            add([2, 48, 36, 0, 1, 8, orientation, 8 | flags])
    for mode in [256, 1024, 4096, 8192]:
        add([2 | mode, 48, 36, 0, 1, 8, 1, 8])
    for space, chroma in [(0, 1), (1, 3), (2, 0)]:
        add([2, 48, 36, space, chroma, 8, 1, 8 | 512])
    for bbox in [1, 2, 6, 16]:
        add([2 | 512, 64, 40, 0, 1, 8, bbox, 8])
    return cases, b''.join(data for _, data in cases)


def classify(reference, candidate):
    """A named, visible known difference, or None for a mismatch."""
    # Neither encoder is x264: no x264: option can be applied.
    if 'Unsupported x264 encoder parameter:' in candidate and 'paramx264:' in candidate:
        return 'x264-option'
    return None


NAMES = (('rusty_h264', 'x264'),)


def fallback(reference, candidate):
    """Classify a differing transcript line of another AVC encoding suite in
    which libheif falls back to its default AVC encoder: x264 in the oracle,
    rusty_h264 in the candidate (see harness.fallback)."""
    r, _ = harness.transcript(reference, NAMES)
    c, _ = harness.transcript(candidate, NAMES)
    if r == c:
        # Equal once files are normalized and the encoder names swapped
        # (the lines may also carry only the encoder's error message).
        return 'builtin-avc-fallback'
    return classify(r, c)


def main():
    harness.corpus = corpus
    harness.classify = classify
    harness.NAMES = NAMES
    harness.LISTING_FORMAT = 2
    harness.DEFAULTS = ('.build/reference-x264', '.build/avc-builtin-encoding-report.json', '.build/avc-builtin-encoding')
    harness.DECODE_KNOWN = 'avc-decoding'
    harness.__doc__ = __doc__
    harness.main()


if __name__ == '__main__':
    main()
