#!/usr/bin/env python3
"""Original-header vvdec/Rust VVC decoded samples, metadata, conversion and errors.

Each vvenc-generated stream from tests/fixtures/vvc-generated.json becomes a
`vvc1` item laid out the way libheif's encoder writes one: a vvcC property with
the VPS/SPS/PPS NAL arrays (src/vvc_config.rs), ispe from the cropped
picture size and every other NAL unit as 4-byte length-prefixed item data.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path

import test_decode
from item_fixtures import item_file, ispe
from test_avc import nals
from test_context import box, full


class Bits:
    """MSB-first reader over an SPS NAL with emulation prevention removed; reads past the end as 0."""

    def __init__(self, nal):
        out = bytearray()
        at = 0
        while at < len(nal):
            if nal[at:at + 3] == b'\0\0\3':
                out += b'\0\0'
                at += 3
            else:
                out.append(nal[at])
                at += 1
        self.data = bytes(out)
        self.pos = 0

    def u(self, n):
        v = 0
        for _ in range(n):
            byte = self.data[self.pos >> 3] if self.pos >> 3 < len(self.data) else 0
            v = (v << 1) | ((byte >> (7 - (self.pos & 7))) & 1)
            self.pos += 1
        return v

    def align(self):
        self.pos = (self.pos + 7) & ~7

    def ue(self):
        zeros = 0
        while self.u(1) == 0:
            zeros += 1
            assert zeros <= 20
        return (1 << zeros) - 1 + self.u(zeros)


def kind(nal):
    return (nal[1] >> 3) & 31


def sps_config(nal):
    """vvcC record fields and the cropped size from an SPS (libheif's encoder-side parse)."""
    b = Bits(nal)
    b.u(24)
    c = dict(layers=b.u(3) + 1, chroma=b.u(2), profile=0, tier=0, level=0, frame_only=0, multi_layer=0,
             constraints=[], level_flags=[], levels=[], subprofiles=[])
    b.u(2)
    assert b.u(1), 'SPS without profile_tier_level'
    c['profile'] = b.u(7)
    c['tier'] = b.u(1)
    c['level'] = b.u(8)
    c['frame_only'] = b.u(1)
    c['multi_layer'] = b.u(1)
    assert not b.u(1), 'general constraint info present'
    c['constraints'].append(0)
    b.align()
    c['level_flags'] = [False] * c['layers']
    c['levels'] = [0] * c['layers']
    for i in reversed(range(c['layers'] - 1)):
        c['level_flags'][i] = bool(b.u(1))
    b.align()
    for i in reversed(range(c['layers'] - 1)):
        if c['level_flags'][i]:
            c['levels'][i] = b.u(8)
    c['subprofiles'] = [b.u(32) for _ in range(b.u(8))]
    b.u(1)
    if b.u(1):
        b.u(1)
    c['width'] = b.ue()
    c['height'] = b.ue()
    left = right = top = bottom = 0
    if b.u(1):
        left, right, top, bottom = b.ue(), b.ue(), b.ue(), b.ue()
    assert not b.u(1), 'subpicture info present'
    c['depth'] = b.ue()
    sx = 2 if c['chroma'] in (1, 2) else 1
    sy = 2 if c['chroma'] == 1 else 1
    cropped = (c['width'] - sx * (left + right), c['height'] - sy * (top + bottom))
    return c, cropped


def vvcc(units):
    """vvcC exactly as EncoderConfiguration::write_into serializes it (VPS/SPS/PPS arrays in stream order)."""
    arrays = {}
    config = None
    for u in units:
        if kind(u) == 15:
            config, _ = sps_config(u)
        if kind(u) in (14, 15, 16):
            arrays.setdefault(kind(u), []).append(u)
    c = config
    body = bytes([255]) + struct.pack('>H', (c['layers'] << 4) | (1 << 2) | c['chroma'])
    body += bytes([(c['depth'] << 5) | 31, len(c['constraints']) & 63, (c['profile'] << 1) | c['tier'], c['level']])
    for i, byte in enumerate(c['constraints']):
        body += bytes([(c['frame_only'] << 7) | (c['multi_layer'] << 6) | byte if i == 0 else byte])
    if c['layers'] > 1:
        flags = 0
        for shift, i in enumerate(reversed(range(c['layers'] - 1))):
            if c['level_flags'][i]:
                flags |= 128 >> shift
        body += bytes([flags])
    for i in reversed(range(c['layers'] - 1)):
        if c['level_flags'][i]:
            body += bytes([c['levels'][i]])
    body += bytes([len(c['subprofiles'])]) + b''.join(struct.pack('>I', v) for v in c['subprofiles'])
    body += struct.pack('>HH', c['width'], c['height']) + b'\0\0' + bytes([len(arrays)])
    for k, us in arrays.items():
        body += bytes([128 | k]) + struct.pack('>H', len(us)) + b''.join(struct.pack('>H', len(u)) + u for u in us)
    return full(b'vvcC', body)


def item(entry):
    """HEIF vvc1 item; VPS/SPS/PPS go to vvcC, every other NAL unit to length-prefixed item data."""
    units = nals(bytes.fromhex(entry['hex']))
    sps = [u for u in units if kind(u) == 15]
    assert len(sps) == 1, entry['name']
    _, cropped = sps_config(sps[0])
    assert cropped == (entry['width'], entry['height']), (entry['name'], cropped)
    data = b''.join(struct.pack('>I', len(u)) + u for u in units if kind(u) not in (14, 15, 16))
    heif = item_file([dict(id=1, kind=b'vvc1', data=data, props=[vvcc(units), ispe(entry['width'], entry['height'])])])
    # Brands as libheif writes a VVC still image (item_file's defaults are HEVC ones).
    ftyp = box(b'ftyp', b'vvic' + bytes(4) + b'mif1vvic')
    assert heif.startswith(ftyp.replace(b'vvic', b'heic'))
    return ftyp + heif[len(ftyp):]


def fixtures(directory):
    entries = json.loads(Path('tests/fixtures/vvc-generated.json').read_text())['fixtures']
    paths = []
    for entry in entries:
        data = bytes.fromhex(entry['hex'])
        assert hashlib.sha256(data).hexdigest() == entry['sha256']
        path = directory / (entry['name'] + '.heif')
        path.write_bytes(item(entry))
        paths.append(str(path))
    return paths


def main():
    if '--reference-build' not in sys.argv:
        sys.argv += ['--reference-build', '.build/reference-vvc']
    if '--work' not in sys.argv:
        sys.argv += ['--work', '.build/vvc']
    if '--output' not in sys.argv:
        sys.argv += ['--output', '.build/vvc-report.json']
    if '--modes' not in sys.argv:
        sys.argv += ['--modes', ','.join(map(str, [*range(23), 26, 27]))]
    directory = Path(sys.argv[sys.argv.index('--work') + 1]).resolve() / 'fixtures'
    directory.mkdir(parents=True, exist_ok=True)
    test_decode.FIXTURES = fixtures(directory)
    test_decode.__doc__ = __doc__
    test_decode.main()


if __name__ == '__main__':
    main()
