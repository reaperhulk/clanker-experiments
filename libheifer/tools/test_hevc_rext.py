#!/usr/bin/env python3
"""Original-header libde265/Rust HEVC range-extension decoded samples, metadata, conversion and errors.

Each x265-generated stream from tests/fixtures/hevc-rext-generated.json (4:0:0
to 4:4:4, 8 to 12 bits, lossless) becomes an `hvc1` item laid out the way
libheif's encoder writes one: an hvcC property with the VPS/SPS/PPS NAL
arrays, ispe from the cropped picture size and every other NAL unit as 4-byte
length-prefixed item data. The candidate decodes the range-extension streams
with oxideav-h265 and the others with rusty_h265.

Streams whose MD5 picture-hash SEI libde265 does not reproduce (recorded by
the generator) may differ as the known difference `libde265-hash-mismatch`.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path

import test_decode
from item_fixtures import item_file, ispe
from test_avc import nals
from test_context import box
from test_vvc import Bits


def kind(nal):
    return (nal[0] >> 1) & 63


def sps_config(nal):
    """General profile/tier/level bytes, chroma format, bit depths and cropped size of an SPS."""
    b = Bits(nal[2:])
    b.u(4)
    sub_layers = b.u(3)
    b.u(1)
    ptl = b.data[1:13]
    b.pos = 8 + 96
    present = [(b.u(1), b.u(1)) for _ in range(sub_layers)]
    if sub_layers:
        b.u(2 * (8 - sub_layers))
    for profile, level in present:
        b.u(88 * profile + 8 * level)
    b.ue()
    chroma = b.ue()
    if chroma == 3:
        b.u(1)
    width, height = b.ue(), b.ue()
    if b.u(1):
        sx, sy = {0: (1, 1), 1: (2, 2), 2: (2, 1), 3: (1, 1)}[chroma]
        left, right, top, bottom = b.ue(), b.ue(), b.ue(), b.ue()
        width -= sx * (left + right)
        height -= sy * (top + bottom)
    return dict(ptl=ptl, chroma=chroma, luma=b.ue() + 8, chroma_depth=b.ue() + 8), (width, height)


def hvcc(units):
    """hvcC (ISO/IEC 14496-15 8.3.3.1) with the VPS/SPS/PPS arrays in stream order."""
    arrays = {}
    config = None
    for u in units:
        if kind(u) == 33:
            config, _ = sps_config(u)
        if kind(u) in (32, 33, 34):
            arrays.setdefault(kind(u), []).append(u)
    c = config
    body = bytes([1]) + c['ptl'] + struct.pack('>HBBBBHB', 0xf000, 0xfc, 0xfc | c['chroma'], 0xf8 | (c['luma'] - 8),
                                              0xf8 | (c['chroma_depth'] - 8), 0, 0x0f)
    body += bytes([len(arrays)])
    for k, us in arrays.items():
        body += bytes([128 | k]) + struct.pack('>H', len(us)) + b''.join(struct.pack('>H', len(u)) + u for u in us)
    return box(b'hvcC', body)


def item(entry):
    """HEIF hvc1 item; VPS/SPS/PPS go to hvcC, every other NAL unit to length-prefixed item data."""
    units = nals(bytes.fromhex(entry['hex']))
    sps = [u for u in units if kind(u) == 33]
    assert len(sps) == 1, entry['name']
    _, cropped = sps_config(sps[0])
    assert cropped == (entry['width'], entry['height']), (entry['name'], cropped)
    data = b''.join(struct.pack('>I', len(u)) + u for u in units if kind(u) not in (32, 33, 34))
    return item_file([dict(id=1, kind=b'hvc1', data=data, props=[hvcc(units), ispe(entry['width'], entry['height'])])])


def fixtures(directory):
    entries = json.loads(Path('tests/fixtures/hevc-rext-generated.json').read_text())['fixtures']
    paths = []
    for entry in entries:
        data = bytes.fromhex(entry['hex'])
        assert hashlib.sha256(data).hexdigest() == entry['sha256']
        path = directory / (entry['name'] + '.heif')
        path.write_bytes(item(entry))
        paths.append(str(path))
        if not entry['libde265_hash_match']:
            test_decode.KNOWN[str(path)] = 'libde265-hash-mismatch'
    return paths


def main():
    if '--work' not in sys.argv:
        sys.argv += ['--work', '.build/hevc-rext']
    if '--output' not in sys.argv:
        sys.argv += ['--output', '.build/hevc-rext-report.json']
    if '--modes' not in sys.argv:
        sys.argv += ['--modes', ','.join(map(str, [*range(23), 26, 27]))]
    directory = Path(sys.argv[sys.argv.index('--work') + 1]).resolve() / 'fixtures'
    directory.mkdir(parents=True, exist_ok=True)
    test_decode.FIXTURES = fixtures(directory)
    test_decode.__doc__ = __doc__
    test_decode.main()


if __name__ == '__main__':
    main()
