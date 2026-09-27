#!/usr/bin/env python3
"""Original-header built-in HTJ2K encoding (libheif's OpenJPH plugin as oracle): exact file bytes, handles and decoded pixels."""
import struct
import sys

import test_encoding

PARAMETER_SETS = 39


def corpus():
    cases = []

    def add(v):
        cases.append(('-'.join(map(str, v)), struct.pack('=8I', *(x & 0xffffffff for x in v))))

    lossless, lossy = 16384, 32768
    for width, height in [(1, 1), (2, 3), (7, 5), (31, 40), (40, 31), (64, 48), (97, 70)]:
        for space, chroma in [(0, 1), (0, 2), (0, 3), (1, 3), (1, 10), (2, 0)]:
            add([10, width, height, space, chroma, 8, 1, 8])
            add([10 | lossless, width, height, space, chroma, 8, 1, 8])
    for quality in [0, 50, 100]:
        add([10 | (quality + 1) << 16, 64, 48, 0, 1, 8, 1, 8])
    for mode in [2048, 2048 | lossless, lossy]:
        add([10 | mode, 64, 48, 0, 3, 8, 1, 8])
        add([10 | mode, 64, 48, 1, 3, 8, 1, 8])
    for depth in [1, 7, 9, 10, 12, 16]:
        for space, chroma in [(0, 1), (2, 0), (1, 3)]:
            add([10, 40, 36, space, chroma, depth, 1, 8])
            add([10 | lossless, 40, 36, space, chroma, depth, 1, 8])
    for index in range(1, PARAMETER_SETS + 1):
        for mode in [0, lossless]:
            add([10 | mode | index << 24, 70, 50, 0, 1, 8, 1, 8])
            add([10 | mode | index << 24, 33, 29, 2, 0, 12, 1, 8])
    # Metadata, orientations, thumbnails, overlays and alpha. A second encode
    # with one OpenJPH encoder (second images, thumbnails) aborts the native
    # process ("Quantization step sizes already initialized"), so none is made.
    for flags in [1024, 2048, 3072, 4096]:
        for orientation in [1, 6]:
            add([10, 48, 36, 0, 1, 8, orientation, 8 | flags])
    for mode in [256, 4096, 8192]:
        add([10 | mode, 48, 36, 0, 1, 8, 1, 8])
        add([10 | lossless | mode, 48, 36, 0, 1, 8, 1, 8])
    for space, chroma in [(0, 1), (0, 3), (1, 3), (2, 0)]:
        add([10, 48, 36, space, chroma, 8, 1, 8 | 512])
        add([10 | lossless, 48, 36, space, chroma, 8, 1, 8 | 512])
    for bbox in [1, 2]:
        add([10 | 512, 64, 40, 0, 1, 8, bbox, 8])
        add([10 | 512 | 11 << 24, 64, 40, 0, 1, 8, bbox, 8])
    return cases, b''.join(data for _, data in cases)


def main():
    for flag, value in [('--work', '.build/htj2k-encoding'), ('--output', '.build/htj2k-encoding-report.json'),
                        ('--reference-build', '.build/reference-jpeg2000')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    test_encoding.corpus = corpus
    test_encoding.__doc__ = __doc__
    test_encoding.main()


if __name__ == '__main__':
    main()
