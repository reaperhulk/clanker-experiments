#!/usr/bin/env python3
"""Registered AVC decoder plugins competing with the built-in AVC decoder (OpenH264 in the oracle)."""
import json
import struct
import sys
from pathlib import Path

import test_plugin_decoding
from item_fixtures import item_file, ispe
from test_avc import avcc, nals
from test_context import box

# libheif's OpenH264 plugin and libheifer's built-in decoder both use priority 70.
PRIORITIES = [1, 69, 70, 71, 777]
STREAMS = ['size-16x16-baseline-1', 'size-18x34-high-1']


def cases():
    entries = {e['name']: e for e in json.loads(Path('tests/fixtures/avc-generated.json').read_text())['fixtures']}
    tests = []

    def add(label, config, data):
        tests.append((label, struct.pack('=12I', *(x & 0xffffffff for x in config)) + struct.pack('=I', len(data)) + data))

    def framed(units):
        return b''.join(struct.pack('>I', len(u)) + u for u in units)

    for name in STREAMS:
        entry = entries[name]
        units = nals(bytes.fromhex(entry['hex']))
        slices = [u for u in units if u[0] & 31 in (1, 5)]
        payloads = {'stream': framed(slices), 'truncated': framed(slices)[:-3], 'garbage': b'\x00\x00\x00\x03ABC'}
        for kind, payload in payloads.items():
            data = item_file([dict(id=1, kind=b'avc1', data=payload, props=[ispe(entry['width'], entry['height']), avcc(units)])])
            for priority in PRIORITIES:
                for version in [3, 5]:
                    # flags: 16 selects the plugin by id, 32 an absent id; 1 drops new_decoder.
                    for flags in [0, 1, 16, 32]:
                        for stage in [0, 2]:
                            v = [version, 2 | (priority << 8), stage, 0, 0, 0, 5, 7, 8, flags, 0, 0]
                            add(f'{name}-{kind}-p{priority}-v{version}-f{flags}-s{stage}', v, data)
    return tests


def main():
    for flag, value in [('--work', '.build/avc-plugins'), ('--output', '.build/avc-plugins-report.json'),
                        ('--reference-build', '.build/reference-avc')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    test_plugin_decoding.cases = cases
    test_plugin_decoding.__doc__ = __doc__
    test_plugin_decoding.main()


if __name__ == '__main__':
    main()
