#!/usr/bin/env python3
"""Original-header AVC image-sequence tracks (intra, P and B frames) against the OpenH264 oracle."""
import hashlib
import json
import struct
import subprocess
import sys
from pathlib import Path

import test_sequence_reading
from generate_avc_fixtures import REVISION, picture
from test_avc import avcc, nals

FRAMES = 5
STREAMS = {
    'intra-high': ['--profile', 'high', '--keyint', '1'],
    'ippp-baseline': ['--profile', 'baseline', '--keyint', '30'],
    'ippp-main': ['--profile', 'main', '--keyint', '30', '--bframes', '0'],
    'ippp-high': ['--profile', 'high', '--keyint', '30', '--bframes', '0'],
    'ippp-high-cavlc': ['--profile', 'high', '--no-cabac', '--keyint', '30', '--bframes', '0'],
    'ippp-refs': ['--profile', 'high', '--keyint', '30', '--bframes', '0', '--ref', '3'],
    'ippp-weightp': ['--profile', 'main', '--keyint', '30', '--bframes', '0', '--weightp', '2'],
    'ibbp-main': ['--profile', 'main', '--keyint', '30', '--bframes', '2', '--b-pyramid', 'none'],
    'ibbp-pyramid': ['--profile', 'high', '--keyint', '30', '--bframes', '3', '--b-pyramid', 'normal'],
    'idr-every-2': ['--profile', 'high', '--keyint', '2', '--min-keyint', '1', '--bframes', '0'],
    'ibbp-noweightb': ['--profile', 'main', '--keyint', '30', '--bframes', '2', '--b-pyramid', 'none', '--no-weightb'],
    'ibbp-temporal': ['--profile', 'main', '--keyint', '30', '--bframes', '2', '--b-pyramid', 'none', '--no-weightb', '--direct', 'temporal'],
    'ibbp-cavlc': ['--profile', 'high', '--no-cabac', '--keyint', '30', '--bframes', '2', '--b-pyramid', 'none', '--no-weightb'],
    # Unchanging frames: every P slice is one skip run over the whole picture.
    'static-baseline': ['--profile', 'baseline', '--keyint', '30'],
}


def box(kind, body):
    return struct.pack('>I', 8 + len(body)) + kind + body


def full(kind, version, flags, body):
    return box(kind, bytes([version]) + flags.to_bytes(3, 'big') + body)


def frames(w, h, n, pattern, step=4):
    """n frames of a moving deterministic pattern (horizontal shift per frame)."""
    base = picture(w + 4 * n, h, 'i420', 8, pattern)
    planes = [(w + 4 * n, h, 0)]
    cw, ch = (w + 4 * n + 1) // 2, (h + 1) // 2
    planes += [(cw, ch, (w + 4 * n) * h), (cw, ch, (w + 4 * n) * h + cw * ch)]
    out = []
    for i in range(n):
        frame = bytearray()
        for c, (pw, ph, offset) in enumerate(planes):
            shift = step * i if c == 0 else step // 2 * i
            width = w if c == 0 else (w + 1) // 2
            for y in range(ph):
                row = base[offset + y * pw:offset + (y + 1) * pw]
                frame += row[shift:shift + width]
        out.append(bytes(frame))
    return out


def sequence(w, h, units, frame_units, delta=40, repeat=None, per_chunk=None, alternate=False):
    """A HEIF image sequence with one avc1 track; SPS/PPS in avcC, slices in samples.

    `repeat` (movie duration in media durations, as a fraction) adds a repeating
    single-entry edit list; `per_chunk` splits the samples into chunks, which
    `alternate` assigns to two identical sample descriptions in turn.
    """
    samples = [b''.join(struct.pack('>I', len(u)) + u for u in au) for au in frame_units]
    duration = delta * len(samples)
    movie = duration if repeat is None else int(duration * repeat)
    ftyp = box(b'ftyp', b'msf1' + b'\0\0\0\0' + b'msf1isom')
    mvhd = full(b'mvhd', 0, 0, struct.pack('>IIII', 0, 0, 1000, movie) + bytes.fromhex(
        '000100000100000000000000000000000001000000000000000000000000000000010000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000000000002'))
    # tkhd tail: layer/alt group/volume/reserved, matrix, then width/height (16.16).
    tkhd = full(b'tkhd', 0, 7, struct.pack('>IIIII', 0, 0, 1, 0, duration) + bytes(8) + struct.pack('>HHHH', 0, 0, 0, 0)
                + bytes.fromhex('000100000000000000000000000000000001000000000000000000000000000040000000')
                + struct.pack('>II', w << 16, h << 16))
    mdhd = full(b'mdhd', 0, 0, struct.pack('>IIII', 0, 0, 1000, duration) + bytes.fromhex('55c40000'))
    hdlr = bytes.fromhex('0000002168646c7200000000000000007069637400000000000000000000000000')
    dinf = bytes.fromhex('0000002464696e660000001c6472656600000000000000010000000c75726c2000000001')
    vmhd = bytes.fromhex('00000014766d6864000000010000000000000000')
    entry = box(b'avc1', bytes(6) + struct.pack('>H', 1) + bytes(16) + struct.pack('>HH', w, h)
                + bytes.fromhex('0048000000480000') + bytes(4) + struct.pack('>H', 1) + bytes(32)
                + bytes.fromhex('0018ffff') + avcc(units))
    stsd = full(b'stsd', 0, 0, struct.pack('>I', 2 if alternate else 1) + entry * (2 if alternate else 1))
    stts = full(b'stts', 0, 0, struct.pack('>III', 1, len(samples), delta))
    per = per_chunk or len(samples)
    chunks = [samples[i:i + per] for i in range(0, len(samples), per)]
    if alternate:
        runs = [(i + 1, len(c), i % 2 + 1) for i, c in enumerate(chunks)]
    else:
        runs = [(1, per, 1)] + ([(len(chunks), len(chunks[-1]), 1)] if len(chunks[-1]) != per else [])
    stsc = full(b'stsc', 0, 0, struct.pack('>I', len(runs)) + b''.join(struct.pack('>III', *r) for r in runs))
    stsz = full(b'stsz', 0, 0, struct.pack('>II', 0, len(samples)) + b''.join(struct.pack('>I', len(s)) for s in samples))

    edts = b'' if repeat is None else box(b'edts', full(b'elst', 0, 1, struct.pack('>IIIHH', 1, duration, 0, 1, 0)))

    def build(offset):
        offsets = []
        for chunk in chunks:
            offsets.append(offset)
            offset += sum(map(len, chunk))
        stco = full(b'stco', 0, 0, struct.pack('>I', len(offsets)) + b''.join(struct.pack('>I', o) for o in offsets))
        stbl = box(b'stbl', stsd + stts + stsc + stsz + stco)
        minf = box(b'minf', dinf + stbl + vmhd)
        mdia = box(b'mdia', mdhd + hdlr + minf)
        trak = box(b'trak', tkhd + edts + mdia)
        return ftyp + box(b'moov', mvhd + trak)

    head = build(0)
    head = build(len(head) + 8)
    return head + box(b'mdat', b''.join(samples))


def access_units(units):
    """Slice NAL units grouped per picture (x264 writes one slice per picture here)."""
    out = []
    for u in units:
        if u[0] & 31 in (1, 5):
            out.append([u])
    return out


FIXTURES = Path('tests/fixtures/avc-sequences.json')


def generate():
    """Encode the streams with the pinned test-only x264 into the committed fixture file."""
    work = Path('.build/avc-sequences-inputs').resolve()
    work.mkdir(parents=True, exist_ok=True)
    source = Path('.build/x264-source').resolve()
    if subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip() != REVISION:
        raise SystemExit('wrong native x264 revision')
    x264 = Path('.build/x264-install/bin/x264').resolve()
    streams = []
    for (name, options), (w, h), pattern in [((n, o), size, p) for n, o in STREAMS.items() for size, p in [((64, 48), 4), ((50, 36), 3)]]:
        raw = work / f'{name}-{w}x{h}.yuv'
        out = work / f'{name}-{w}x{h}.264'
        raw.write_bytes(b''.join(frames(w, h, FRAMES, pattern, 0 if name.startswith('static') else 4)))
        subprocess.run([str(x264), '--quiet', '--no-progress', '--threads', '1', '--frames', str(FRAMES), '--input-res', f'{w}x{h}',
                        '--input-csp', 'i420', '--qp', '26', *options, '-o', str(out), str(raw)], check=True)
        streams.append({'name': f'{name}-{w}x{h}', 'width': w, 'height': h, 'options': options, 'hex': out.read_bytes().hex()})
    generator = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    FIXTURES.write_text(json.dumps({'x264_revision': REVISION, 'generator_sha256': generator, 'streams': streams}, indent=1) + '\n')


def ue(v):
    return format(v + 1, 'b').zfill(2 * (v + 1).bit_length() - 1)


def overrun(unit, total, extra):
    """A P slice that is one skip run over `total` macroblocks, with the run lengthened."""
    rbsp = bytearray()
    zeros = 0
    for b in unit[1:]:
        if zeros >= 2 and b == 3:
            zeros = 0
            continue
        rbsp.append(b)
        zeros = zeros + 1 if b == 0 else 0
    bits = ''.join(format(b, '08b') for b in rbsp).rstrip('0')[:-1]
    if not bits.endswith(ue(total)):
        raise SystemExit('static P slice is not a single skip run')
    bits = bits[:-len(ue(total))] + ue(total + extra) + '1'
    bits += '0' * (-len(bits) % 8)
    out = bytearray(unit[:1])
    zeros = 0
    for b in int(bits, 2).to_bytes(len(bits) // 8, 'big'):
        if zeros >= 2 and b <= 3:
            out.append(3)
            zeros = 0
        out.append(b)
        zeros = zeros + 1 if b == 0 else 0
    return bytes(out)


def corpus():
    cases = []
    for stream in json.loads(FIXTURES.read_text())['streams']:
        name, w, h = stream['name'], stream['width'], stream['height']
        units = nals(bytes.fromhex(stream['hex']))
        aus = access_units(units)
        cases.append((name, sequence(w, h, units, aus)))
        # Truncated tracks: fewer samples than frames, and a missing last slice byte.
        cases.append((f'{name}-short', sequence(w, h, units, aus[:3])))
        broken = aus[:-1] + [[aus[-1][0][:-1]]]
        cases.append((f'{name}-cut', sequence(w, h, units, broken)))
        # A repeating edit list (2.5 track durations) and two samples per chunk
        # (libheif keeps one decoder per chunk and sends parameter sets with
        # sample 0 only).
        cases.append((f'{name}-repeat', sequence(w, h, units, aus, repeat=2.5)))
        cases.append((f'{name}-chunks', sequence(w, h, units, aus, per_chunk=2)))
        cases.append((f'{name}-descriptions', sequence(w, h, units, aus, per_chunk=2, alternate=True)))
        if name.startswith('static'):
            # A P skip run past the picture end fills the picture (OpenH264 clamps it).
            total = ((w + 15) // 16) * ((h + 15) // 16)
            long = [aus[0]] + [[overrun(au[0], total, 5)] for au in aus[1:]]
            cases.append((f'{name}-overrun', sequence(w, h, units, long)))
    return cases


def main():
    if '--generate' in sys.argv:
        return generate()
    for flag, value in [('--work', '.build/avc-sequences'), ('--output', '.build/avc-sequences-report.json'),
                        ('--reference-build', '.build/reference-avc')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    test_sequence_reading.corpus = corpus
    test_sequence_reading.__doc__ = __doc__
    test_sequence_reading.main()


if __name__ == '__main__':
    main()
