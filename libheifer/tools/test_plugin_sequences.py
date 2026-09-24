#!/usr/bin/env python3
"""Original-header registered decoder plugins driving image-sequence tracks: push/poll/flush order, user data, errors and output."""
import json
import struct
import sys
from pathlib import Path

import test_plugin_decoding
from test_avc import avcc, nals
from test_avc_sequences import track_file
from test_context import box

SAMPLES = [b'\x00\x00\x00\x03ABC', b'\x00\x00\x00\x02DE', b'\x00\x00\x00\x04FGHI', b'\x00\x00\x00\x01J', b'\x00\x00\x00\x03KLM']


def configs():
    stream = next(e for e in json.loads(Path('tests/fixtures/avc-generated.json').read_text())['fixtures']
                  if e['name'] == 'size-16x16-baseline-1')
    return {
        2: (b'avc1', avcc(nals(bytes.fromhex(stream['hex'])))),
        1: (b'hvc1', box(b'hvcC', bytes.fromhex('01016000000090000000000078f000fcfdf8f800000f00'))),
        4: (b'av01', box(b'av1C', bytes.fromhex('81000c00'))),
    }


def files(kind, config):
    return {
        'track': track_file(16, 16, kind, config, SAMPLES),
        'short': track_file(16, 16, kind, config, SAMPLES[:2]),
        'repeat': track_file(16, 16, kind, config, SAMPLES[:3], repeat=2),
        'chunks': track_file(16, 16, kind, config, SAMPLES, per_chunk=2),
        'descriptions': track_file(16, 16, kind, config, SAMPLES, per_chunk=2, alternate=True),
        'empty-sample': track_file(16, 16, kind, config, SAMPLES[:2] + [b''] + SAMPLES[3:]),
    }


def cases():
    tests = []

    def add(label, config, data):
        tests.append((label, struct.pack('=12I', *(x & 0xffffffff for x in config)) + struct.pack('=I', len(data)) + data))

    for format, (kind, config) in configs().items():
        tracks = files(kind, config)
        base = [5, format, 0, 0, 0, 0, 16, 16, 8, 0, 0, 0]
        for name, data in tracks.items():
            for flags in [0, 128]:
                v = base.copy()
                v[9] = flags
                add(f'{format}-{name}-{flags}', v, data)
            for delay in [1, 2, 6]:
                v = base.copy()
                v[5] = delay
                add(f'{format}-{name}-delay{delay}', v, data)
        data = tracks['track']
        for version in [1, 2, 3, 4, 5, 6]:
            v = base.copy()
            v[0] = version
            add(f'{format}-version{version}', v, data)
        for flags in list(range(64)) + [64, 64 | 128]:
            v = base.copy()
            v[9] = flags
            add(f'{format}-flags{flags}', v, data)
        for stage in [1, 2, 3, 4]:
            for call in [0, 1, 2, 3]:
                for subcode, prefixed in [(0, 0), (2006, 1), (3003, 0)]:
                    v = base.copy()
                    v[2:5] = [stage, subcode, prefixed]
                    v[10] = call
                    add(f'{format}-failure{stage}-{call}-{subcode}-{prefixed}', v, data)
        for w, h, bits in [(1, 1, 8), (15, 17, 8), (16, 16, 10), (64, 64, 16)]:
            v = base.copy()
            v[6:9] = [w, h, bits]
            add(f'{format}-output{w}x{h}x{bits}', v, data)
        # Only AVC has the same built-in decoder set in both builds (OpenH264).
        for priority in [1, 69, 70, 71] if format == 2 else []:
            v = base.copy()
            v[1] = format | priority << 8
            add(f'{format}-priority{priority}', v, data)
    return tests


def main():
    for flag, value in [('--work', '.build/plugin-sequences'), ('--output', '.build/plugin-sequences-report.json')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    test_plugin_decoding.cases = cases
    test_plugin_decoding.CLIENT = Path('tests/plugin_sequences.c')
    test_plugin_decoding.__doc__ = __doc__
    test_plugin_decoding.main()


if __name__ == '__main__':
    main()
