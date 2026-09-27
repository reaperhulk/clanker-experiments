#!/usr/bin/env python3
"""Original-header AV1/mini resource-limit boundaries during read and decode."""
import json
from pathlib import Path
import struct
import sys
import test_security
from test_mini import fixture


def corpus():
    fixtures = [('mini-alpha', Path('tests/upstream/tests/data/simple_osm_tile_alpha.avif').read_bytes()),
                ('mini-metadata', fixture(icc=b'ICC-profile', exif=b'\0\0\0\0Exif', xmp=b'XMP', alpha=True, hdr=63)),
                ('mini-tmap-icc', fixture(gain=True, tmap_icc=bytes(17)))]
    for entry in json.loads(Path('tests/fixtures/av1-generated.json').read_text())['fixtures']:
        if entry['sampling'] in ('420', 'gray'):
            fixtures.append((entry['name'], bytes.fromhex(entry['hex'])))
    cases = []
    for name, data in fixtures:
        for field, values in {0: [0, 1, 526, 527, 528, 65535, 65536],
                              3: [0, 1, 2, 3, 4], 4: [0, 1, 10, 11, 16, 17],
                              5: [0, 1, 16, len(data)-24, len(data)-23, len(data), 4096, 16384],
                              6: [0, 1, 2, 3], 7: [0, 1, 2], 9: [0, 1, 2, 3, 4, 5, 8, 9, 16],
                              10: [0, 1, len(data)-24, len(data), 4096, 16384],
                              14: [0, 1, 2]}.items():
            for value in values:
                for phase in (0, 1):
                    cases.append((f'{name}/{field}/{phase}/{value}', struct.pack('=IIIQ', len(data), field, phase, value) + data))
    return cases


if __name__ == '__main__':
    test_security.corpus = corpus
    test_security.__doc__ = __doc__
    if '--output' not in sys.argv: sys.argv += ['--output', '.build/av1-limits-report.json']
    test_security.main()
