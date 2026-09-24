#!/usr/bin/env python3
"""Original-header built-in JPEG2000 encoding (libheif's OpenJPEG plugin as oracle): exact file bytes, handles and decoded pixels."""
import struct
import sys

import test_encoding


def corpus():
    cases = []

    def add(v):
        cases.append(('-'.join(map(str, v)), struct.pack('=8I', *(x & 0xffffffff for x in v))))

    lossy = 32768
    for width, height in [(1, 1), (31, 40), (40, 31), (32, 32), (33, 47), (64, 48), (97, 70)]:
        for space, chroma in [(0, 1), (0, 2), (0, 3), (1, 3), (1, 10), (2, 0)]:
            add([7, width, height, space, chroma, 8, 1, 8])
            add([7 | lossy, width, height, space, chroma, 8, 1, 8])
    for quality in [0, 1, 10, 25, 50, 70, 90, 97, 98, 99, 100]:
        add([7 | lossy | (quality + 1) << 16, 64, 48, 0, 1, 8, 1, 8])
        add([7 | lossy | (quality + 1) << 16, 48, 40, 2, 0, 8, 1, 8])
    for mode in [16384, 16384 | lossy, 2048, 2048 | lossy]:
        add([7 | mode, 64, 48, 0, 3, 8, 1, 8])
        add([7 | mode, 64, 48, 1, 3, 8, 1, 8])
    # Generic heif_encoder_set_parameter (tests/encoding.c parameter sets):
    # chroma=422, lossless=true, and names the plugin does not have.
    for index in [31, 32, 1, 21]:
        add([7 | index << 24, 64, 48, 0, 1, 8, 1, 8])
        add([7 | lossy | index << 24, 64, 48, 0, 1, 8, 1, 8])
    for depth in [1, 7, 9, 10, 12, 16]:
        for space, chroma in [(0, 1), (2, 0), (1, 3)]:
            add([7, 40, 36, space, chroma, depth, 1, 8])
            add([7 | lossy, 40, 36, space, chroma, depth, 1, 8])
    # Metadata, orientations, thumbnails, overlays, alpha and repeated encodes.
    for flags in [1024, 2048, 3072, 4096, 8192]:
        for orientation in [1, 6]:
            add([7, 48, 36, 0, 1, 8, orientation, 8 | flags])
    for mode in [256, 1024, 4096, 8192]:
        add([7 | mode, 48, 36, 0, 1, 8, 1, 8])
    for space, chroma in [(0, 1), (0, 3), (1, 3), (2, 0)]:
        add([7, 48, 36, space, chroma, 8, 1, 8 | 512])
        add([7 | lossy, 48, 36, space, chroma, 8, 1, 8 | 512])
    for bbox in [1, 2, 6]:
        add([7 | 512, 64, 40, 0, 1, 8, bbox, 8])
    return cases, b''.join(data for _, data in cases)


def main():
    for flag, value in [('--work', '.build/jpeg2000-encoding'), ('--output', '.build/jpeg2000-encoding-report.json'),
                        ('--reference-build', '.build/reference-jpeg2000')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    test_encoding.corpus = corpus
    test_encoding.__doc__ = __doc__
    test_encoding.main()


if __name__ == '__main__':
    main()
