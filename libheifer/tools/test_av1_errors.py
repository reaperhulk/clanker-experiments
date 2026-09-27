#!/usr/bin/env python3
"""Independent AV1 packet truncation/corruption comparisons with valid mini metadata."""
import json
from pathlib import Path
import sys
import test_decode
from test_mini import fixture


def main():
    if '--work' not in sys.argv: sys.argv += ['--work', '.build/av1-errors']
    if '--output' not in sys.argv: sys.argv += ['--output', '.build/av1-errors-report.json']
    if '--modes' not in sys.argv: sys.argv += ['--modes', '0,11,27']
    work = Path(sys.argv[sys.argv.index('--work') + 1]).resolve() / 'fixtures'
    work.mkdir(parents=True, exist_ok=True)
    paths = []
    for entry in json.loads(Path('tests/fixtures/av1-generated.json').read_text())['fixtures']:
        data = bytes.fromhex(entry['hex'])
        config_at = data.index(b'av1C')
        config_size = int.from_bytes(data[config_at-4:config_at], 'big') - 8
        config = data[config_at+4:config_at+4+config_size]
        at = 0
        while data[at+4:at+8] != b'mdat':
            at += int.from_bytes(data[at:at+4], 'big')
        size = int.from_bytes(data[at:at+4], 'big')
        packet = data[at+8:at+size]
        variants = [('full', packet)]
        variants += [(f'truncated-{n}', packet[:n]) for n in [1, 2, 4, 8, 16, len(packet)//2, len(packet)-1]]
        for position in [0, 1, len(packet)//2, len(packet)-1]:
            damaged = bytearray(packet)
            damaged[position] ^= 255
            variants.append((f'corrupted-{position}', bytes(damaged)))
        for name, packet in variants:
            path = work / (name + '-' + entry['name'])
            path.write_bytes(fixture(width=entry['width'], height=entry['height'],
                                     chroma={'gray': 0, '420': 1, '422': 2, '444': 3, 'rgb': 3}[entry['sampling']],
                                     depth=entry['bit_depth'], config=config, coded_data=packet))
            paths.append(str(path))
    test_decode.FIXTURES = paths
    test_decode.__doc__ = __doc__
    test_decode.main()


if __name__ == '__main__':
    main()
