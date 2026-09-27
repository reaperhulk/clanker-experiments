#!/usr/bin/env python3
"""Compact-container reader callbacks, late AV1 decoding, range failures and source offsets."""
from pathlib import Path
import sys
import test_reader_input
from test_context import box
from test_mini import corpus


def cases():
    base = corpus(Path('tests/upstream'))
    out = [(f'{name}-v{version}', data, version, 0, 0) for version in (0, 1, 2, 3) for name, data in base]
    fixtures = [(name, data) for name, data in base[:4] if name.endswith('.avif')]
    for name, data in fixtures:
        for padding in (0, 32, 8192):
            data_start = int.from_bytes(data[:4], 'big')
            padded = data[:data_start] + box(b'free', bytes(padding)) + data[data_start:]
            for version in (1, 2, 3):
                for flags in (0, 8, 16, 64, 72):
                    for status in (0, 1, 2, 3, 99):
                        out.append((f'{name}-pad{padding}-v{version}-f{flags}-s{status}', padded, version, flags | 128, status))
    return out


if __name__ == '__main__':
    test_reader_input.cases = cases
    test_reader_input.__doc__ = __doc__
    if '--work' not in sys.argv: sys.argv += ['--work', '.build/mini-reader']
    if '--output' not in sys.argv: sys.argv += ['--output', '.build/mini-reader-report.json']
    test_reader_input.main()
