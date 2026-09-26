#!/usr/bin/env python3
"""Exact minimized box diagnostics through original-header memory/file clients."""
import itertools
import sys
from pathlib import Path
import test_debug_dump
from test_mini import corpus, fixture

def cases():
    tests = corpus(Path('tests/upstream'))
    for same, chroma, depth, cicp in itertools.product([False, True], range(4), [8, 9, 12, 16, 32, 64, 128], [None, (9, 16, 9)]):
        tests.append((f'gain-{same}-{chroma}-{depth}-{cicp}', fixture(gain=True, hdr=63, gain_same_size=same, gain_chroma=chroma, gain_depth=depth, gain_float=depth >= 16, tmap_cicp=cicp)))
    for flags in range(256):
        tests.append((f'cclv-{flags}', fixture(hdr=4, gain=True, cclv_flags=flags)))
    for config in [b'', b'123', b'123456789']:
        for alpha in [False, True]:
            tests.append((f'config-{config}-{alpha}', fixture(gain=True, hdr=63, gain_config=config, alpha=alpha, premultiplied=True, hdr_bias=53)))
    for kind in [b'\0\1\xffa', b'\0\0\0\0', b'AV01']:
        tests.append((f'codec-{kind}', fixture(explicit=True, codec_types=(kind, kind))))
    return tests

if __name__ == '__main__':
    test_debug_dump.cases = cases
    test_debug_dump.__doc__ = __doc__
    if '--work' not in sys.argv: sys.argv += ['--work', '.build/mini-debug']
    if '--output' not in sys.argv: sys.argv += ['--output', '.build/mini-debug-report.json']
    test_debug_dump.main()
