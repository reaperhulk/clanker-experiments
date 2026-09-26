#!/usr/bin/env python3
"""Original-header AVC pixel and memory limit boundaries at read and decode (OpenH264 oracle)."""
import json
import struct
import sys
from pathlib import Path

import test_security
from item_fixtures import item_file, ispe
from test_avc import item

STREAMS = ['size-18x34-baseline-1', 'size-130x66-high-1', 'limits-272x256-high', 'limits-270x250-baseline']


def corpus():
    entries = {e['name']: e for e in json.loads(Path('tests/fixtures/avc-generated.json').read_text())['fixtures']}
    cases = []
    for name in STREAMS:
        entry = entries[name]
        w, h = entry['width'], entry['height']
        cw, ch = -(-w // 16) * 16, -(-h // 16) * 16
        data = item(entry)
        padded = (w + 16) * (h + 16)
        edges = lambda n: [n - 1, n, n + 1]
        fields = {
            0: [0, 1, 65535, 65536, 65537, *edges(w * h), *edges(cw * ch), *edges(padded)],
            5: [0, 1, *edges(len(data)), *edges(w * h), *edges(cw * ch), 4096, 65536],
            10: [0, 1, 1024, 16384, 65536, *edges(w * h * 3 // 2), 1 << 20],
        }
        for field, values in fields.items():
            for value in sorted(set(v for v in values if v >= 0)):
                for phase in [0, 1]:
                    cases.append((f'{name}/{field}/{phase}/{value}', struct.pack('=IIIQ', len(data), field, phase, value) + data))
    # Declared ispe smaller than the coded picture: libheif's ispe+16 padded
    # pixel limit (floored at 65536) then decides the coded-size check.
    for name, dims in [('limits-272x256-high', [(255, 240), (256, 240), (240, 256), (272, 240), (200, 200)]),
                       ('limits-270x250-baseline', [(255, 240), (256, 240), (254, 241)])]:
        entry = entries[name]
        for iw, ih in dims:
            data = item(dict(entry, width=iw, height=ih))
            for value in [0, 65536, 69632, 70000, 1 << 20]:
                for phase in [0, 1]:
                    cases.append((f'{name}-ispe-{iw}x{ih}/0/{phase}/{value}', struct.pack('=IIIQ', len(data), 0, phase, value) + data))
    return cases


if __name__ == '__main__':
    test_security.corpus = corpus
    test_security.__doc__ = __doc__
    if '--reference-build' not in sys.argv:
        sys.argv += ['--reference-build', '.build/reference-avc']
    if '--output' not in sys.argv:
        sys.argv += ['--output', '.build/avc-limits-report.json']
    test_security.main()
