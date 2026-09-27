#!/usr/bin/env python3
"""Original-header built-in JPEG encoding (libheif's libjpeg-turbo plugin as oracle): exact file bytes, handles and decoded pixels."""
import struct
import sys

import test_encoding


def corpus():
    cases = []

    def add(v):
        cases.append(('-'.join(map(str, v)), struct.pack('=8I', *(x & 0xffffffff for x in v))))

    sizes = [(1, 1), (2, 3), (3, 2), (5, 7), (16, 9), (17, 33), (64, 48)]
    for width, height in sizes:
        for space, chroma in [(0, 1), (0, 2), (0, 3), (1, 3), (1, 10), (2, 0)]:
            add([3, width, height, space, chroma, 8, 1, 8])
        for quality in [0, 1, 10, 25, 49, 50, 51, 75, 90, 99, 100, 101]:
            add([3 | (quality + 1) << 16, width, height, 0, 1, 8, 1, 8])
        add([3 | 16384, width, height, 0, 1, 8, 1, 8])
    for depth in [1, 7, 9, 10, 12, 16]:
        for space, chroma in [(0, 1), (2, 0), (1, 3)]:
            add([3, 5, 7, space, chroma, depth, 1, 8])
    # Metadata, pixel aspect ratio (JFIF density), orientations, thumbnails,
    # overlays and repeated encodes.
    for flags in [1024, 2048, 3072, 1024 | 256, 4096, 8192]:
        for orientation in [1, 5, 6, 8]:
            add([3, 17, 9, 0, 1, 8, orientation, 8 | flags])
    for mode in [256, 1024, 4096, 8192]:
        add([3 | mode, 17, 9, 0, 1, 8, 1, 8])
    for flags in [8, 8 | 1024]:
        add([3 | 512, 24, 18, 0, 1, 8, 1, flags])
    # Alpha planes (encoded as an auxiliary image by the same encoder).
    for space, chroma in [(0, 1), (0, 3), (1, 3), (2, 0)]:
        add([3, 17, 9, space, chroma, 8, 1, 8 | 512])
    for bbox in [1, 2, 3, 6, 12]:
        for width, height in [(24, 18), (18, 24), (33, 9)]:
            add([3 | 512, width, height, 0, 1, 8, bbox, 8])
    return cases, b''.join(data for _, data in cases)


def main():
    for flag, value in [('--work', '.build/jpeg-encoding'), ('--output', '.build/jpeg-encoding-report.json'),
                        ('--reference-build', '.build/reference-jpeg')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    test_encoding.corpus = corpus
    test_encoding.__doc__ = __doc__
    test_encoding.main()


if __name__ == '__main__':
    main()
