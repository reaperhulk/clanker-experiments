#!/usr/bin/env python3
"""Independent AVC truncations, malformed parameter sets, NAL framing and avcC boxes."""
import hashlib
import json
import struct
import sys
from pathlib import Path

import test_decode
from item_fixtures import item_file, ispe
from test_avc import Bits, avcc, nals, unescape
from test_context import box

SOURCES = ['size-18x34-baseline-1', 'size-18x34-high-1', 'slices-main-slice-max-mbs', 'cqm-high-custom-10',
           'size-16x16-high-cavlc-3', 'format-i400-8']


def framed(units):
    return b''.join(struct.pack('>I', len(u)) + u for u in units)


def escape(rbsp):
    out = bytearray()
    zeros = 0
    for b in rbsp:
        if zeros >= 2 and b <= 3:
            out.append(3)
            zeros = 0
        out.append(b)
        zeros = zeros + 1 if b == 0 else 0
    return bytes(out)


def with_byte(unit, at, value):
    """Replace one RBSP byte of a NAL unit (after its header), re-escaping the result."""
    rbsp = bytearray(unescape(unit[1:]))
    if at < len(rbsp):
        rbsp[at] = value
    return unit[:1] + escape(bytes(rbsp))


def sps_head_bits(sps):
    """RBSP bits of an SPS up to (excluding) vui_parameters_present_flag."""
    rbsp = unescape(sps[1:])
    r = Bits(rbsp)
    profile = r.u(8)
    r.u(16)
    r.ue()
    if profile in (100, 110, 122, 244, 44, 83, 86, 118, 128):
        chroma = r.ue()
        if chroma == 3:
            r.u(1)
        r.ue()
        r.ue()
        r.u(1)
        assert r.u(1) == 0, 'scaling matrices are not rewritten'
    r.ue()
    poc = r.ue()
    if poc == 0:
        r.ue()
    elif poc == 1:
        r.u(1)
        r.ue()
        r.ue()
        for _ in range(r.ue()):
            r.ue()
    r.ue()
    r.u(1)
    r.ue()
    r.ue()
    if not r.u(1):
        r.u(1)
    r.u(1)
    if r.u(1):
        for _ in range(4):
            r.ue()
    return ''.join(str((rbsp[i >> 3] >> (7 - (i & 7))) & 1) for i in range(r.pos))


def hrd_sps(sps, vcl, zeros, tail):
    """The SPS with a VUI whose (NAL or VCL) HRD cpb_cnt_minus1 is a run of
    `zeros` zero bits followed by `tail`: openh264 2.6.0 loops over the read's
    error code there, not the value."""
    bits = sps_head_bits(sps) + '1' + '00000' + ('01' if vcl else '1') + '0' * zeros + tail + '1'
    bits += '0' * (-len(bits) % 8)
    rbsp = bytes(int(bits[i:i + 8], 2) for i in range(0, len(bits), 8))
    return sps[:1] + escape(rbsp)


def cases(entry):
    units = nals(bytes.fromhex(entry['hex']))
    sps = [u for u in units if u[0] & 31 == 7]
    pps = [u for u in units if u[0] & 31 == 8]
    slices = [u for u in units if u[0] & 31 in (1, 5)]
    config = avcc(units)
    payload = framed(slices)
    out = [(f'prefix-{i}', config, payload[:i]) for i in range(0, len(payload) + 1, 1 if len(payload) < 600 else 3)]
    last = slices[-1]
    for i in range(1, len(last)):
        out.append((f'slice-{i}', config, framed(slices[:-1] + [last[:i]])))
    for at in [0, 1, 2, 3, 4, 5, 8, 12, len(last) // 2, len(last) - 2, len(last) - 1]:
        for value in [0, 1, 3, 0x80, 0xFF]:
            broken = bytearray(last)
            broken[min(at, len(last) - 1)] = value
            out.append((f'slice-byte-{at}-{value}', config, framed(slices[:-1] + [bytes(broken)])))
    for header in [0x00, 0x05, 0x21, 0x41, 0x65, 0x85, 0x06, 0x09, 0x0C, 0x14, 0x1F]:
        out.append((f'header-{header:02x}', config, framed(slices[:-1] + [bytes([header]) + last[1:]])))
    # SPS/PPS carried in the item payload instead of (or in addition to) avcC.
    out.append(('inline-parameter-sets', config, framed(sps + pps + slices)))
    out.append(('only-inline-parameter-sets', avcc_raw(entry, [], []), framed(sps + pps + slices)))
    out.append(('no-sps', avcc_raw(entry, [], pps), payload))
    out.append(('no-pps', avcc_raw(entry, sps, []), payload))
    out.append(('no-slices', config, b''))
    out.append(('parameter-sets-only', config, framed(sps + pps)))
    out.append(('duplicate-slices', config, framed(slices + slices)))
    for size in [0, 1, 2, 3]:
        out.append((f'nal-size-{size}', config, framed([last[:size]]) if size else b'\0\0\0\0' + payload))
    out.append(('length-overflow', config, b'\xff\xff\xff\xff' + payload))
    out.append(('length-short', config, struct.pack('>I', len(last) - 1) + last))
    out.append(('trailing-2', config, payload + b'\0\0'))
    out.append(('leading-emulation', config, framed([b'\0\0\1' + last, last])))
    for i in range(1, len(sps[0])):
        out.append((f'sps-prefix-{i}', avcc_raw(entry, [sps[0][:i]], pps), payload))
    for i in range(1, len(pps[0])):
        out.append((f'pps-prefix-{i}', avcc_raw(entry, sps, [pps[0][:i]]), payload))
    for profile in [0, 44, 66, 77, 83, 86, 88, 100, 110, 118, 122, 128, 244, 255]:
        altered = sps[0][:1] + bytes([profile]) + sps[0][2:]
        out.append((f'sps-profile-{profile}', avcc_raw(entry, [altered], pps), payload))
    for level in [0, 9, 10, 11, 13, 20, 31, 40, 51, 52, 60, 62, 255]:
        altered = sps[0][:3] + bytes([level]) + sps[0][4:]
        out.append((f'sps-level-{level}', avcc_raw(entry, [altered], pps), payload))
    for at in range(3, min(len(unescape(sps[0][1:])), 12)):
        for value in [0, 0x80, 0xFF]:
            out.append((f'sps-byte-{at}-{value}', avcc_raw(entry, [with_byte(sps[0], at, value)], pps), payload))
    for at in range(0, min(len(unescape(pps[0][1:])), 6)):
        for value in [0, 0x40, 0x80, 0xFF]:
            out.append((f'pps-byte-{at}-{value}', avcc_raw(entry, sps, [with_byte(pps[0], at, value)]), payload))
    for vcl, zeros, tail in [(v, z, t) for v in (False, True) for z in (32, 33, 40, 47)
                             for t in ('', '1', '1' * 8, '1' * 16, '10' * 12, '1' * 40, '0000000100000001', '1' + '0' * 20 + '1')]:
        label = f'sps-{"vcl" if vcl else "nal"}-hrd-{zeros}-{len(tail)}-{tail.count("1")}'
        out.append((label, avcc_raw(entry, [hrd_sps(sps[0], vcl, zeros, tail)], pps), payload))
    out.append(('no-avcc', None, payload))
    out.append(('two-sps', avcc_raw(entry, sps + sps, pps), payload))
    out.append(('two-pps', avcc_raw(entry, sps, pps + pps), payload))
    for i in range(len(config) - 8):
        truncated = config[:8 + i]
        out.append((f'avcc-prefix-{i}', struct.pack('>I', len(truncated)) + truncated[4:], payload))
    return out


def avcc_raw(entry, sps, pps):
    """avcC with explicit parameter sets; header fields come from the fixture's own SPS."""
    units = nals(bytes.fromhex(entry['hex']))
    first = [u for u in units if u[0] & 31 == 7][0]
    body = bytes([1, first[1], first[2], first[3], 0xFF, 0xE0 | len(sps)])
    body += b''.join(struct.pack('>H', len(u)) + u for u in sps)
    body += bytes([len(pps)]) + b''.join(struct.pack('>H', len(u)) + u for u in pps)
    if first[1] not in (66, 77, 88):
        body += bytes([0xFC | (0 if 'i400' in entry['name'] else 1), 0xF8, 0xF8, 0])
    return box(b'avcC', body)


def main():
    if '--work' not in sys.argv:
        sys.argv += ['--work', '.build/avc-errors']
    if '--output' not in sys.argv:
        sys.argv += ['--output', '.build/avc-errors-report.json']
    if '--modes' not in sys.argv:
        sys.argv += ['--modes', '0,11,27']
    directory = Path(sys.argv[sys.argv.index('--work') + 1]).resolve() / 'fixtures'
    directory.mkdir(parents=True, exist_ok=True)
    test_decode.FIXTURES = []
    for entry in json.loads(Path('tests/fixtures/avc-generated.json').read_text())['fixtures']:
        if entry['name'] not in SOURCES:
            continue
        assert hashlib.sha256(bytes.fromhex(entry['hex'])).hexdigest() == entry['sha256']
        for label, config, payload in cases(entry):
            path = directory / f'{entry["name"]}-{label}.heif'
            props = [config, ispe(entry['width'], entry['height'])] if config else [ispe(entry['width'], entry['height'])]
            path.write_bytes(item_file([dict(id=1, kind=b'avc1', data=payload, props=props)]))
            test_decode.FIXTURES.append(str(path))
    test_decode.__doc__ = __doc__
    test_decode.main()


if __name__ == '__main__':
    main()
