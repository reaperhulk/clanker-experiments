#!/usr/bin/env python3
"""Independent HTJ2K malformed and constructed code-blocks: complete prefixes, corrupted cleanup
segments, refinement (SigProp/MagRef) passes, multi-layer segments, code-block styles and markers."""
import hashlib
import json
import random
import sys
from pathlib import Path

import test_decode
from item_fixtures import item_file, ispe
from test_context import box


def markers(data):
    """Main-header marker offsets up to the first SOT."""
    at, found = 2, {}
    while at + 4 <= len(data):
        code = data[at + 1]
        found.setdefault(code, at)
        if code == 0x90:
            break
        at += 2 + int.from_bytes(data[at + 2:at + 4], 'big')
    return found


class BitReader:
    """OpenJPEG's opj_bio: after a 0xFF byte the next byte carries seven bits."""

    def __init__(self, data):
        self.data, self.pos, self.buf, self.ct = data, 0, 0, 0

    def bit(self):
        if self.ct == 0:
            self.buf = (self.buf << 8) & 0xFFFF
            self.ct = 7 if self.buf == 0xFF00 else 8
            self.buf |= self.data[self.pos]
            self.pos += 1
        self.ct -= 1
        return (self.buf >> self.ct) & 1

    def bits(self, n):
        v = 0
        for _ in range(n):
            v = (v << 1) | self.bit()
        return v

    def align(self):
        if (self.buf & 0xFF) == 0xFF:
            self.pos += 1
        return self.pos


class BitWriter:
    def __init__(self):
        self.out, self.buf, self.ct = bytearray(), 0, 8

    def bit(self, b):
        if self.ct == 0:
            self.out.append(self.buf)
            self.ct = 7 if self.buf == 0xFF else 8
            self.buf = 0
        self.ct -= 1
        self.buf |= (b & 1) << self.ct

    def bits(self, v, n):
        for i in reversed(range(n)):
            self.bit(v >> i)

    def flush(self):
        self.out.append(self.buf)
        if self.buf == 0xFF:
            self.out.append(0)
        return bytes(self.out)


def num_passes_code(w, n):
    if n == 1:
        w.bits(0, 1)
    elif n == 2:
        w.bits(0b10, 2)
    elif n <= 5:
        w.bits(0b1100 + n - 3, 4)
    elif n <= 36:
        w.bits(0b1111, 4)
        w.bits(n - 6, 5)
    else:
        w.bits(0b111111111, 9)
        w.bits(n - 37, 7)


def parse_single(data):
    """The zero bit-planes, cleanup bytes and header pieces of a single code-block codestream."""
    m = markers(data)
    sot = m[0x90]
    sod = sot + 12
    assert data[sod:sod + 2] == b'\xff\x93'
    body = data[sod + 2:]
    r = BitReader(body)
    assert r.bit() == 1 and r.bit() == 1
    zero = 0
    while r.bit() == 0:
        zero += 1
    assert r.bit() == 0  # one pass
    k = 0
    while r.bit() == 1:
        k += 1
    length = r.bits(3 + k)
    start = r.align()
    return dict(header=data[:sot], zero=zero, cleanup=body[start:start + length], cod=m[0x52], qcd=m[0x5c])


def layer_header(state, layer, included, zero, passes, lengths, lblock_inc=None):
    """One packet of a single code-block, following opj_t2_read_packet_header for HT."""
    w = BitWriter()
    if not included and state['numsegs'] == 0 and not state['included']:
        # Zero-length packet.
        w.bits(0, 1)
        return w.flush(), []
    w.bits(1, 1)
    if not state['included']:
        # Inclusion tag tree of a single node: zeros up to the first layer, then one.
        low = state['low']
        while low < layer + 1:
            if included and low == layer:
                w.bits(1, 1)
                break
            w.bits(0, 1)
            low += 1
        state['low'] = low
        if not included:
            return w.flush(), []
        w.bits(0, zero)
        w.bits(1, 1)
        state['included'] = True
    else:
        w.bits(1 if included else 0, 1)
        if not included:
            return w.flush(), []
    num_passes_code(w, passes)
    # Segment assignment as OpenJPEG does: the first segment takes one pass.
    if state['numsegs'] == 0:
        segno = 0
    elif state['last_passes'] == 109:
        segno = state['numsegs']
    else:
        segno = state['numsegs'] - 1
    segs, n = [], passes
    while n > 0:
        take = 1 if segno == 0 else n
        segs.append((segno, take))
        n -= take
        segno += 1
    needed = max((lengths[i].bit_length() - (take.bit_length() - 1) for i, (_, take) in enumerate(segs)), default=0)
    inc = max(0, needed - state['lblock']) if lblock_inc is None else lblock_inc
    w.bits((1 << inc) - 1, inc)
    w.bits(0, 1)
    state['lblock'] += inc
    for (seg, take), length in zip(segs, lengths):
        w.bits(length & ((1 << (state['lblock'] + take.bit_length() - 1)) - 1), state['lblock'] + take.bit_length() - 1)
        if seg >= state['numsegs']:
            state['numsegs'] = seg + 1
            state['last_passes'] = 0
        state['last_passes'] += take
    return w.flush(), segs


def build(base, layers, style=None, zero=None, guard=0):
    """Codestream with the given packets: each layer is (included, passes, [segment data]).
    Extra guard bits raise Mb above the zero bit-planes, so refinement passes are decoded."""
    header = bytearray(base['header'])
    cod = base['cod']
    qcd = base['qcd']
    header[qcd + 4] = ((((header[qcd + 4] >> 5) + guard) & 7) << 5) | (header[qcd + 4] & 0x1F)
    header[cod + 6:cod + 8] = len(layers).to_bytes(2, 'big')
    if style is not None:
        header[cod + 12] = style
    state = dict(included=False, low=0, numsegs=0, last_passes=0, lblock=3)
    packets = bytearray()
    for index, (included, passes, segments) in enumerate(layers):
        head, _ = layer_header(state, index, included, base['zero'] if zero is None else zero, passes,
                               [len(s) for s in segments])
        packets += head
        if included:
            for s in segments:
                packets += s
    sot = bytearray(b'\xff\x90\x00\x0a\x00\x00') + (12 + 2 + len(packets)).to_bytes(4, 'big') + b'\x00\x01'
    return bytes(header) + bytes(sot) + b'\xff\x93' + bytes(packets) + b'\xff\xd9'


def refinement_cases(base):
    cleanup = base['cleanup']
    cases = []
    for seed in range(2):
        rng = random.Random(f"{base['name']}-{seed}")
        for n, length in [(2, 1), (2, 3), (2, 9), (2, 40), (3, 1), (3, 2), (3, 7), (3, 33), (3, 90), (2, 0), (3, 0), (4, 5)]:
            refine = bytes(rng.randrange(256) for _ in range(length))
            for style, guard in [(0x40, 0), (0x40, 1), (0x48, 3)]:
                cases.append((f'single-{n}-{length}-{seed}-{style:x}-{guard}',
                              build(base, [(True, n, [cleanup, refine])], style, guard=guard)))
    rng = random.Random(base['name'])
    refine = bytes(rng.randrange(256) for _ in range(24))
    stuffed = bytes([0xFF, 0x7F, 0x8F, 0xFF, 0x7F] * 5)
    for label, layers in [
        ('layers-1-2', [(True, 1, [cleanup]), (True, 2, [refine[:10], refine[10:]])]),
        ('layers-1-1', [(True, 1, [cleanup]), (True, 1, [refine])]),
        ('layers-1-1-1', [(True, 1, [cleanup]), (True, 1, [refine[:8]]), (True, 1, [refine[8:]])]),
        ('layers-3-1', [(True, 3, [cleanup, refine[:12]]), (True, 1, [refine[12:]])]),
        ('layers-2-2', [(True, 2, [cleanup, refine[:12]]), (True, 2, [refine[12:]])]),
        ('layers-empty-3', [(False, 0, []), (True, 3, [cleanup, refine])]),
        ('layers-empty-empty-2', [(False, 0, []), (False, 0, []), (True, 2, [cleanup, refine])]),
        ('layers-3-skip-1', [(True, 3, [cleanup, refine]), (False, 0, []), (True, 1, [refine[:3]])]),
        ('stuffed-3', [(True, 3, [cleanup, stuffed])]),
        ('stuffed-2', [(True, 2, [cleanup, stuffed])]),
        ('ones-3', [(True, 3, [cleanup, b'\xff' * 30])]),
        ('zeros-3', [(True, 3, [cleanup, b'\x00' * 30])]),
        ('zero-cleanup', [(True, 3, [b'', refine])]),
        ('short-cleanup', [(True, 1, [cleanup[-1:]])]),
    ]:
        cases.append((label, build(base, layers)))
        cases.append((label + '-guard2', build(base, layers, guard=2)))
    for delta in [-2, -1, 1, 2, 5]:
        zero = base['zero'] + delta
        if zero >= 0:
            cases.append((f'zero{delta:+}', build(base, [(True, 1, [cleanup])], zero=zero)))
            cases.append((f'zero{delta:+}-3', build(base, [(True, 3, [cleanup, refine])], zero=zero)))
            cases.append((f'zero{delta:+}-3-guard2', build(base, [(True, 3, [cleanup, refine])], zero=zero, guard=2)))
    for style in [0x41, 0x42, 0x44, 0x45, 0x50, 0x60, 0x7F, 0xC0, 0x80]:
        cases.append((f'style-{style:x}', build(base, [(True, 3, [cleanup, refine])], style, guard=2)))
        cases.append((f'style-{style:x}-layers', build(base, [(True, 1, [cleanup]), (True, 2, [refine[:10], refine[10:]])], style)))
    return cases


def corruption_cases(name, data):
    cases = [(f'prefix-{i}', data[:i]) for i in range(0, len(data) + 1, max(1, len(data) // 64))]
    m = markers(data)
    sod = m[0x90] + 12
    rng = random.Random(name)
    positions = sorted(set(list(range(sod + 2, min(len(data) - 2, sod + 34))) + list(range(max(sod + 2, len(data) - 34), len(data) - 2))
                           + [rng.randrange(sod + 2, len(data) - 2) for _ in range(24)]))
    for at in positions:
        for value in [0x00, 0x7F, 0x8F, 0x90, 0xFF, data[at] ^ 1]:
            broken = bytearray(data)
            broken[at] = value & 0xFF
            cases.append((f'byte-{at}-{value & 0xFF}', bytes(broken)))
    cod = m[0x52]
    for style in [0x00, 0x08, 0x48, 0x41, 0x44, 0x80, 0xC0]:
        broken = bytearray(data)
        broken[cod + 12] = style
        cases.append((f'cod-style-{style:x}', bytes(broken)))
    # RGN after COD: a zero shift is accepted, others fail HT decoding.
    for shift in [0, 1, 7]:
        rgn = b'\xff\x5e\x00\x05\x00\x00' + bytes([shift])
        cases.append((f'rgn-{shift}', data[:m[0x5c]] + rgn + data[m[0x5c]:]))
    cap = m.get(0x50)
    if cap is not None:
        length = int.from_bytes(data[cap + 2:cap + 4], 'big')
        cases.append(('cap-removed', data[:cap] + data[cap + 2 + length:]))
        for value in [0, 1, 2, 3, 255]:
            broken = bytearray(data)
            broken[cap + 2:cap + 4] = value.to_bytes(2, 'big')
            cases.append((f'cap-length-{value}', bytes(broken)))
        cases.append(('cpf', data[:cap] + b'\xff\x59\x00\x04\x00\x00' + data[cap:]))
    return cases


def main():
    for flag, value in [('--work', '.build/htj2k-errors'), ('--output', '.build/htj2k-errors-report.json'), ('--modes', '0')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    directory = Path(sys.argv[sys.argv.index('--work') + 1]).resolve() / 'fixtures'
    directory.mkdir(parents=True, exist_ok=True)
    test_decode.FIXTURES = []
    fixtures = {e['name']: e for e in json.loads(Path('tests/fixtures/htj2k-generated.json').read_text())['fixtures']}
    for entry in fixtures.values():
        data = bytes.fromhex(entry['hex'])
        assert hashlib.sha256(data).hexdigest() == entry['sha256']
        if entry['name'].startswith('single-'):
            base = parse_single(data)
            base['name'] = entry['name']
            cases = refinement_cases(base)
            if entry['name'] in ['single-13x7-1-8', 'single-64x22-0-12']:
                cases += corruption_cases(entry['name'], data)
        elif entry['name'] in ['17x19-3-8-0-1', '17x19-1-12-1-0', 'block-4x4-1-2', 'precincts-2-0']:
            cases = corruption_cases(entry['name'], data)
        else:
            continue
        for label, payload in cases:
            path = directory / f"{entry['name']}-{label}.heif"
            path.write_bytes(item_file([dict(id=1, kind=b'j2k1', data=payload,
                                             props=[ispe(entry['width'], entry['height']), box(b'j2kH', b'')])]))
            test_decode.FIXTURES.append(str(path))
    test_decode.__doc__ = __doc__
    test_decode.main()


if __name__ == '__main__':
    main()
