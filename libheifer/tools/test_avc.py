#!/usr/bin/env python3
"""Original-header OpenH264/Rust AVC decoded samples, metadata, conversion and errors."""
import hashlib
import json
import struct
import sys
from pathlib import Path

import test_decode
from item_fixtures import item_file, ispe
from test_context import box


class Bits:
    def __init__(self, rbsp):
        self.data = rbsp
        self.pos = 0

    def u(self, n):
        v = 0
        for _ in range(n):
            v = (v << 1) | ((self.data[self.pos >> 3] >> (7 - (self.pos & 7))) & 1)
            self.pos += 1
        return v

    def ue(self):
        zeros = 0
        while self.u(1) == 0:
            zeros += 1
        return (1 << zeros) - 1 + self.u(zeros)


def unescape(nal):
    out = bytearray()
    zeros = 0
    for b in nal:
        if zeros >= 2 and b == 3:
            zeros = 0
            continue
        out.append(b)
        zeros = zeros + 1 if b == 0 else 0
    return bytes(out)


def nals(stream):
    """Split an Annex B stream into NAL units without start codes."""
    starts = []
    i = 0
    while i + 3 <= len(stream):
        if stream[i:i + 3] == b'\0\0\1':
            starts.append(i + 3)
            i += 3
        else:
            i += 1
    units = []
    for n, start in enumerate(starts):
        end = starts[n + 1] - 3 if n + 1 < len(starts) else len(stream)
        unit = stream[start:end]
        while n + 1 < len(starts) and unit.endswith(b'\0'):
            unit = unit[:-1]
        units.append(unit)
    return units


def sps_format(sps):
    """profile_idc, chroma_format_idc, luma/chroma bit depth from an SPS NAL."""
    r = Bits(unescape(sps[1:]))
    profile = r.u(8)
    r.u(16)
    r.ue()
    chroma, luma_depth, chroma_depth = 1, 8, 8
    if profile in (100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135):
        chroma = r.ue()
        if chroma == 3:
            r.u(1)
        luma_depth = 8 + r.ue()
        chroma_depth = 8 + r.ue()
    return profile, chroma, luma_depth, chroma_depth


def avcc(units):
    sps = [u for u in units if u[0] & 31 == 7]
    pps = [u for u in units if u[0] & 31 == 8]
    profile, chroma, luma_depth, chroma_depth = sps_format(sps[0])
    body = bytes([1, sps[0][1], sps[0][2], sps[0][3], 0xFF, 0xE0 | len(sps)])
    body += b''.join(struct.pack('>H', len(u)) + u for u in sps)
    body += bytes([len(pps)]) + b''.join(struct.pack('>H', len(u)) + u for u in pps)
    if profile not in (66, 77, 88):
        body += bytes([0xFC | chroma, 0xF8 | (luma_depth - 8), 0xF8 | (chroma_depth - 8), 0])
    return box(b'avcC', body)


def item(entry, keep=(1, 5)):
    """HEIF avc1 item; parameter sets go to avcC, kept NAL types to length-prefixed item data."""
    units = nals(bytes.fromhex(entry['hex']))
    data = b''.join(struct.pack('>I', len(u)) + u for u in units if u[0] & 31 in keep)
    return item_file([dict(id=1, kind=b'avc1', data=data, props=[avcc(units), ispe(entry['width'], entry['height'])])])


def fixtures(directory):
    entries = json.loads(Path('tests/fixtures/avc-generated.json').read_text())['fixtures']
    paths = []
    for entry in entries:
        data = bytes.fromhex(entry['hex'])
        assert hashlib.sha256(data).hexdigest() == entry['sha256']
        path = directory / (entry['name'] + '.heif')
        path.write_bytes(item(entry))
        paths.append(str(path))
    # SEI/AUD NAL units inside the item payload.
    for entry in entries[:8]:
        path = directory / (entry['name'] + '-sei.heif')
        path.write_bytes(item(entry, keep=(1, 5, 6, 9, 12)))
        paths.append(str(path))
    # Several access units (P/B pictures, multiple slices) in one item payload.
    for stream in json.loads(Path('tests/fixtures/avc-sequences.json').read_text())['streams']:
        units = nals(bytes.fromhex(stream['hex']))
        slices = [i for i, u in enumerate(units) if u[0] & 31 in (1, 5)]
        starts = [i for i in slices if units[i][1] & 0x80]
        for label, end in [('multi', len(units)), ('multi2', starts[2] if len(starts) > 2 else len(units))]:
            entry = dict(stream, hex=b''.join(b'\0\0\0\1' + u for u in units[:end]).hex())
            path = directory / f"{stream['name']}-{label}.heif"
            path.write_bytes(item(entry))
            paths.append(str(path))
    return paths


def main():
    if '--work' not in sys.argv:
        sys.argv += ['--work', '.build/avc']
    if '--output' not in sys.argv:
        sys.argv += ['--output', '.build/avc-report.json']
    if '--modes' not in sys.argv:
        sys.argv += ['--modes', ','.join(map(str, [*range(23), 26, 27]))]
    directory = Path(sys.argv[sys.argv.index('--work') + 1]).resolve() / 'fixtures'
    directory.mkdir(parents=True, exist_ok=True)
    test_decode.FIXTURES = fixtures(directory)
    test_decode.__doc__ = __doc__
    test_decode.main()


if __name__ == '__main__':
    main()
