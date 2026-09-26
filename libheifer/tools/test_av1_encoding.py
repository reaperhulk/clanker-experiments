#!/usr/bin/env python3
"""Original-header built-in AV1 encoding (libheif's rav1e plugin as oracle): exact file bytes, handles and decoded pixels."""
import struct
import sys

import test_encoding

# tests/encoding.c parameter sets 40-57 are the rav1e ones; the HTJ2K sets exercise unknown names.
PARAMETER_SETS = list(range(40, 58)) + [1, 31, 32]


def corpus():
    cases = []

    def add(v):
        cases.append(('-'.join(map(str, v)), struct.pack('=8I', *(x & 0xffffffff for x in v))))

    lossless = 16384
    for width, height in [(1, 1), (2, 3), (7, 5), (31, 40), (64, 48), (97, 70)]:
        for space, chroma in [(0, 1), (0, 2), (0, 3), (1, 3), (1, 10), (2, 0)]:
            add([4, width, height, space, chroma, 8, 1, 8])
    for quality in [0, 1, 25, 50, 75, 99, 100]:
        add([4 | (quality + 1) << 16, 64, 48, 0, 1, 8, 1, 8])
        add([4 | (quality + 1) << 16, 48, 40, 0, 3, 10, 1, 8])
    for mode in [lossless, 2048, 32768]:
        add([4 | mode, 64, 48, 0, 3, 8, 1, 8])
    for depth in [7, 8, 9, 10, 12, 16]:
        for space, chroma in [(0, 1), (2, 0), (0, 3)]:
            add([4, 40, 36, space, chroma, depth, 1, 8])
    for index in PARAMETER_SETS:
        add([4 | index << 24, 70, 50, 0, 1, 8, 1, 8])
        add([4 | 51 << 16 | index << 24, 33, 29, 0, 3, 10, 1, 8])
    # Metadata (nclx, HDR), orientations, thumbnails, overlays, alpha and repeated encodes.
    for flags in [1024, 2048, 3072, 4096, 8192]:
        for orientation in [1, 6]:
            add([4, 48, 36, 0, 1, 8, orientation, 8 | flags])
    for mode in [256, 1024, 4096, 8192]:
        add([4 | mode, 48, 36, 0, 1, 8, 1, 8])
    for space, chroma in [(0, 1), (0, 3), (1, 3), (2, 0)]:
        add([4, 48, 36, space, chroma, 8, 1, 8 | 512])
        add([4, 48, 36, space, chroma, 10, 1, 8 | 512])
    # Alpha planes are encoded as 4:2:0 whatever the chroma parameter.
    for index in [50, 51]:
        add([4 | index << 24, 48, 36, 0, 1, 8, 1, 8 | 512])
        add([4 | index << 24, 48, 36, 0, 3, 10, 1, 8 | 512])
    for bbox in [1, 2, 6, 16]:
        add([4 | 512, 64, 40, 0, 1, 8, bbox, 8])
    return cases, b''.join(data for _, data in cases)


def main():
    for flag, value in [('--work', '.build/av1-encoding'), ('--output', '.build/av1-encoding-report.json'),
                        ('--reference-build', '.build/reference-av1')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    test_encoding.corpus = corpus
    test_encoding.__doc__ = __doc__
    test_encoding.main()


if __name__ == '__main__':
    main()
