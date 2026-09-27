#!/usr/bin/env python3
"""VVC image-sequence tracks (intra, random-access and low-delay inter frames) against the vvdec oracle."""
import hashlib
import json
import math
import subprocess
import sys
from pathlib import Path

import test_sequence_reading
from generate_vvc_fixtures import REVISION
from test_avc import nals
from test_avc_sequences import track_file
from test_vvc import kind, vvcc

FRAMES = 9
# (preset, QP, vvenc_set_param options); the fixture encoder defaults to all intra.
RA = ['IntraPeriod=32', 'DecodingRefreshType=cra']
LD = ['IntraPeriod=32', 'DecodingRefreshType=none', 'PicReordering=0', 'GOPSize=8', 'POC0IDR=1']
STREAMS = {
    'ra-medium': ('medium', 32, RA),
    'ra-faster': ('faster', 37, RA),
    'ra-slower': ('slower', 32, RA),
    'ra-10bit': ('medium', 32, RA),
    'ra-mono': ('medium', 32, RA),
    'ra-gop16': ('fast', 32, ['IntraPeriod=16', 'GOPSize=16', 'DecodingRefreshType=cra']),
    'ra-idr-4': ('medium', 32, ['IntraPeriod=4', 'DecodingRefreshType=idr']),
    'ra-cra-4': ('medium', 32, ['IntraPeriod=4', 'DecodingRefreshType=cra']),
    'ld-medium': ('medium', 32, LD),
    'ld-fast': ('fast', 37, LD),
    'all-intra': ('medium', 32, []),
}


def frames(w, h, chroma, depth, n):
    """n frames of a moving deterministic pattern with a little noise."""
    sub = {400: None, 420: (1, 1)}[chroma]
    top = (1 << depth) - 1
    raw = bytearray()
    state = 12345
    for t in range(n):
        planes = [(w, h, 0, 0)]
        if sub:
            planes += [((w + 1) >> sub[0], (h + 1) >> sub[1], sub[0], sub[1])] * 2
        for c, (pw, ph, sx, sy) in enumerate(planes):
            for y in range(ph):
                for x in range(pw):
                    lx, ly = (x << sx) + t * 3, (y << sy) + t * 2
                    v = 128 + int(60 * math.sin(lx * 0.21 + c) * math.cos(ly * 0.17)) + ((lx // 8 + ly // 8) % 2) * 30 - 15
                    state = (state * 1103515245 + 12345) & 0x7FFFFFFF
                    v = max(0, min(255, v + (state >> 16) % 5 - 2))
                    raw += (v * top // 255).to_bytes(2, 'little')
    return bytes(raw)


FIXTURES = Path('tests/fixtures/vvc-sequences.json')


def generate():
    """Encode the streams with the pinned test-only vvenc into the committed fixture file."""
    work = Path('.build/vvc-sequences-inputs').resolve()
    work.mkdir(parents=True, exist_ok=True)
    source = Path('.build/vvenc-source').resolve()
    if subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip() != REVISION:
        raise SystemExit('wrong native vvenc revision')
    encoder = Path('.build/vvc-generated/vvc_fixture_encoder').resolve()
    if not encoder.exists():
        raise SystemExit('run tools/generate_vvc_fixtures.py first to build the fixture encoder')
    streams = []
    for name, (preset, qp, options) in STREAMS.items():
        chroma = 400 if name.endswith('mono') else 420
        depth = 10 if '10bit' in name else 8
        for w, h in [(64, 48), (50, 36)]:
            raw = work / f'{name}-{w}x{h}.yuv'
            out = work / f'{name}-{w}x{h}.266'
            raw.write_bytes(frames(w, h, chroma, depth, FRAMES))
            command = [str(encoder), str(w), str(h), str(chroma), str(depth), preset, str(qp), str(raw), str(out), *options]
            subprocess.run(command, check=True, capture_output=True)
            streams.append({'name': f'{name}-{w}x{h}', 'width': w, 'height': h, 'chroma': chroma, 'depth': depth,
                            'preset': preset, 'qp': qp, 'options': options, 'hex': out.read_bytes().hex()})
    generator = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    helper = hashlib.sha256(Path('tests/vvc_fixture_encoder.c').read_bytes()).hexdigest()
    FIXTURES.write_text(json.dumps({'vvenc_revision': REVISION, 'generator_sha256': generator, 'helper_sha256': helper,
                                    'streams': streams}, indent=1) + '\n')


def access_units(units):
    """Units per picture without the parameter sets (they go into vvcC).

    Every VCL unit starts a picture unless a picture header unit already did;
    prefix units (APS, picture header, AUD, prefix SEI) belong to the next
    picture and suffix units to the current one.
    """
    out = []
    pending = []
    header = False
    for u in units:
        k = kind(u)
        if k in (14, 15, 16):
            continue
        if k <= 12:
            if header:
                out[-1] += pending + [u]
            else:
                out.append(pending + [u])
            pending = []
            header = False
        elif k == 19:
            out.append(pending + [u])
            pending = []
            header = True
        elif k in (18, 24, 21) and out:
            out[-1].append(u)
        else:
            pending.append(u)
    return out


def sequence(w, h, units, aus, **kw):
    samples = [b''.join(len(u).to_bytes(4, 'big') + u for u in au) for au in aus]
    return track_file(w, h, b'vvc1', vvcc(units), samples, **kw)


def corpus():
    cases = []
    for stream in json.loads(FIXTURES.read_text())['streams']:
        name, w, h = stream['name'], stream['width'], stream['height']
        units = nals(bytes.fromhex(stream['hex']))
        aus = access_units(units)
        cases.append((name, sequence(w, h, units, aus)))
        # A truncated track: fewer samples than frames.
        cases.append((f'{name}-short', sequence(w, h, units, aus[:3])))
        # A repeating edit list and chunks of two samples (one decoder per
        # chunk, parameter sets with sample 0 only).
        cases.append((f'{name}-repeat', sequence(w, h, units, aus, repeat=2.5)))
        cases.append((f'{name}-chunks', sequence(w, h, units, aus, per_chunk=2)))
        # Not compared: a damaged last sample, and alternating sample
        # descriptions (whose second decoder lacks parameter sets). vvdec only
        # returns pictures after a parse delay derived from the host's thread
        # count, so how many pictures precede the decoding error depends on
        # the machine; the candidate outputs pictures as soon as the DPB
        # output rules allow (docs/RESULTS.md, known differences).
    return cases


def main():
    if '--generate' in sys.argv:
        return generate()
    for flag, value in [('--work', '.build/vvc-sequences'), ('--output', '.build/vvc-sequences-report.json'),
                        ('--reference-build', '.build/reference-vvc')]:
        if flag not in sys.argv:
            sys.argv += [flag, value]
    test_sequence_reading.corpus = corpus
    test_sequence_reading.__doc__ = __doc__
    test_sequence_reading.main()


if __name__ == '__main__':
    main()
