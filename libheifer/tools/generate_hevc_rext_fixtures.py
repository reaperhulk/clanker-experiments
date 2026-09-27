#!/usr/bin/env python3
"""Generate owned H.265/HEVC range-extension intra streams with the pinned test-only x265.

x265 is never a candidate dependency. tests/hevc_fixture_encoder.c drives the
reference build's multilib libx265 (8, 10 and 12 bit) and writes one intra
picture as an Annex B stream. The streams cover the formats the candidate
decodes with oxideav-h265 (4:2:2, 4:4:4, bit depths above 10, lossless) next
to 4:2:0 and 4:0:0 ones; decoded pixels are compared between libheif with
libde265 and the Rust candidate. Configurations x265 refuses are recorded as
encoder failures.

Every stream carries x265's MD5 decoded-picture-hash SEI, and the generator
records whether the test-only native dec265 (libde265, libheif's HEVC
decoder) reproduces it (`--check-hash`). tools/test_hevc_rext.py accepts a
candidate/libheif difference only for streams libde265 decodes wrongly.
"""
import argparse
import hashlib
import itertools
import json
import os
import subprocess
from pathlib import Path

REVISION = '1d117bed4747758b51bd2c124d738527e30392cb'  # 4.1, tools/build_reference.py X265

# Plane subsampling shifts.
CHROMA = {400: None, 420: (1, 1), 422: (1, 0), 444: (0, 0)}


def picture(w, h, chroma, depth, pattern):
    """Deterministic planar input (16-bit above depth 8); patterns exercise flat, directional and noisy intra modes."""
    state = 0x1234567 + pattern * 7919 + w * 31 + h

    def noise():
        nonlocal state
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        return state >> 16

    top = (1 << depth) - 1
    planes = [(w, h)]
    if CHROMA[chroma]:
        sx, sy = CHROMA[chroma]
        planes += [((w + sx) >> sx, (h + sy) >> sy)] * 2
    raw = bytearray()
    for c, (pw, ph) in enumerate(planes):
        for y in range(ph):
            for x in range(pw):
                if pattern == 0:
                    v = (x * 255 // max(1, pw - 1) + y * 3 + c * 40) & 255
                elif pattern == 1:
                    v = noise() & 255
                elif pattern == 2:
                    v = 230 if ((x >> 2) + (y >> 3) + c) & 1 else 20
                elif pattern == 3:
                    v = (x * x + y * 7 * (c + 1) + (noise() & 15)) & 255
                elif pattern == 6:
                    # Luma detail repeated in chroma at its own scale, so
                    # cross-component ALF has correlated luma to borrow from.
                    lx, ly = (x, y) if c == 0 else (x << CHROMA[chroma][0], y << CHROMA[chroma][1])
                    v = 128 + (((lx * 7 + ly * 3) % 23) - 11) * 6 + (40 if ((lx >> 3) ^ (ly >> 4)) & 1 else -40) + (noise() & 7)
                elif pattern == 5:
                    # Screen-content-like: sharp text strokes on a flat background.
                    v = 16 + 200 * (((x * 5 + y * 3) % 11) < 2) + c * 30 if (x // 8 + y // 8) & 1 else 128 + c * 20
                else:
                    v = 128 + ((x - pw // 2) * (y - ph // 2) * (c + 1) >> 3) + (noise() & 31) - 16
                v = max(0, min(255, v))
                if depth > 8:
                    # Use the low bits too, so high bit depths are not just scaled 8-bit data.
                    v = min(top, v * top // 255 + (noise() & ((1 << (depth - 8)) - 1)))
                raw += v.to_bytes(2, 'little') if depth > 8 else bytes([v])
    return bytes(raw)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--install', default='.build/x265-install')
    p.add_argument('--source', default='.build/x265-source')
    p.add_argument('--dec265', default='.build/libde265-install/bin/dec265')
    p.add_argument('--work', default='.build/hevc-rext-generated')
    a = p.parse_args()
    source = Path(a.source).resolve()
    revision = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if revision != REVISION:
        raise SystemExit('wrong native x265 revision')
    install = Path(a.install).resolve()
    library = install / 'lib/libx265.so'
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    helper_source = Path('tests/hevc_fixture_encoder.c')
    encoder = work / 'hevc_fixture_encoder'
    subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Werror', f'-I{install / "include"}', str(helper_source),
                    str(library), f'-Wl,-rpath,{library.parent}', '-o', str(encoder)], check=True)
    dec265 = Path(a.dec265).resolve()
    dec265_env = dict(os.environ, LD_LIBRARY_PATH=str(dec265.parent.parent / 'lib'))
    fixtures = []
    failures = []
    names = set()

    def add(name, w, h, preset='medium', options=(), chroma=444, depth=10, pattern=1):
        assert name not in names, name
        names.add(name)
        raw = picture(w, h, chroma, depth, pattern)
        inp = work / f'{name}.yuv'
        out = work / f'{name}.265'
        inp.write_bytes(raw)
        out.unlink(missing_ok=True)
        command = [str(encoder), str(w), str(h), str(chroma), str(depth), preset, str(inp), str(out), *options, 'hash=1']
        run = subprocess.run(command, capture_output=True)
        record = dict(name=name, width=w, height=h, chroma=chroma, depth=depth, preset=preset,
                      options=list(options), pattern=pattern,
                      command=[encoder.name, *command[1:6], f'{name}.yuv', f'{name}.265', *options, 'hash=1'],
                      input_sha256=hashlib.sha256(raw).hexdigest())
        if run.returncode or not out.exists() or not out.stat().st_size:
            failures.append(dict(record, returncode=run.returncode, stderr=run.stderr.decode(errors='replace')))
            return
        data = out.read_bytes()
        check = subprocess.run([str(dec265), '--check-hash', '--quiet', str(out)], capture_output=True, env=dec265_env)
        fixtures.append(dict(record, libde265_hash_match=check.returncode == 0 and b'mismatch' not in check.stdout + check.stderr,
                             sha256=hashlib.sha256(data).hexdigest(), hex=data.hex()))

    formats = [(c, d) for c in (400, 420, 422, 444) for d in (8, 10, 12)]
    rext = [(c, d) for c, d in formats if c in (422, 444) or d == 12]
    for (chroma, depth), pattern in itertools.product(formats, [0, 1, 3, 4]):
        add(f'format-{chroma}-{depth}-{pattern}', 64, 48, 'medium', ['qp=27'], chroma, depth, pattern)
    # Sizes: tiny, odd (conformance windows in each subsampling), thin and larger pictures.
    sizes = [(16, 16), (24, 18), (34, 22), (33, 17), (17, 33), (16, 72), (72, 16), (130, 66), (200, 136)]
    for (w, h), (chroma, depth) in itertools.product(sizes, [(422, 8), (422, 10), (444, 8), (444, 12), (400, 12), (420, 12)]):
        add(f'size-{w}x{h}-{chroma}-{depth}', w, h, 'medium', ['qp=32'], chroma, depth, 4)
    for (chroma, depth), qp, pattern in itertools.product([(444, 10), (422, 12), (400, 12), (444, 8)],
                                                         [0, 1, 4, 12, 22, 37, 45, 51], [1, 3]):
        add(f'qp-{chroma}-{depth}-{qp}-{pattern}', 32, 32, 'fast', [f'qp={qp}'], chroma, depth, pattern)
    # True lossless (cu_transquant_bypass) in every format, alone and with transform skip.
    for (chroma, depth), pattern in itertools.product(formats, [1, 3]):
        add(f'lossless-{chroma}-{depth}-{pattern}', 32, 24, 'medium', ['lossless=1'], chroma, depth, pattern)
    for chroma, depth in rext:
        add(f'lossless-tskip-{chroma}-{depth}', 32, 24, 'slow', ['lossless=1', 'tskip=1'], chroma, depth, 5)
        add(f'cu-lossless-{chroma}-{depth}', 48, 32, 'slower', ['qp=22', 'cu-lossless=1'], chroma, depth, 5)
    tools = [['sao=0'], ['deblock=0'], ['deblock=-6:6'], ['deblock=3:-2'], ['ctu=16'], ['ctu=32'], ['min-cu-size=16'],
             ['tu-intra-depth=4'], ['max-tu-size=4'], ['max-tu-size=8'], ['strong-intra-smoothing=0'],
             ['signhide=0'], ['cbqpoffs=-12', 'crqpoffs=12'], ['cbqpoffs=12', 'crqpoffs=-12'], ['constrained-intra=1'],
             ['tskip=1'], ['rdoq-level=0'], ['rdoq-level=2'], ['rd=6', 'psy-rd=0'], ['aq-mode=0'], ['aq-mode=3'],
             ['wpp=0'], ['rect=0', 'amp=0'], ['limit-tu=4'], ['fast-intra=1'], ['b-intra=1']]
    # Not slices: x265 4.1 writes one-picture multi-slice streams through this API whose
    # later slices are a few bytes long and fail its own MD5 picture hash in any decoder.
    for extra, (chroma, depth) in itertools.product(tools, [(444, 10), (422, 8), (420, 12)]):
        label = '_'.join(e.replace('=', '').replace(':', '_') for e in extra)
        pattern = 5 if 'tskip' in label else 4
        add(f'tool-{label}-{chroma}-{depth}', 64, 48, 'medium', ['qp=27', *extra], chroma, depth, pattern)
    for preset, (chroma, depth) in itertools.product(['ultrafast', 'superfast', 'veryfast', 'faster', 'fast', 'medium',
                                                      'slow', 'slower', 'veryslow', 'placebo'], [(444, 10), (422, 8)]):
        add(f'preset-{preset}-{chroma}-{depth}', 96, 64, preset, ['crf=23'], chroma, depth, 4)
    # Colour description, range and SAR in the VUI.
    add('vui-bt2020-pq-444-12', 48, 32, 'medium', ['qp=27', 'colorprim=bt2020', 'transfer=smpte2084',
        'colormatrix=bt2020nc', 'range=full', 'sar=4:3'], 444, 12, 4)
    add('vui-bt709-422-10', 48, 32, 'medium', ['qp=27', 'colorprim=bt709', 'transfer=bt709', 'colormatrix=bt709',
        'range=limited'], 422, 10, 4)
    # Above libheif's 65536-pixel floor for the ispe-padded limit (resource limits).
    add('limits-272x256-444-10', 272, 256, 'fast', ['qp=30'], 444, 10, 4)
    result = dict(generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), x265_revision=revision,
                  helper_sha256=hashlib.sha256(helper_source.read_bytes()).hexdigest(),
                  encoder_sha256=hashlib.sha256(library.read_bytes()).hexdigest(),
                  dec265_sha256=hashlib.sha256(dec265.read_bytes()).hexdigest(),
                  fixtures=fixtures, encoder_failures=failures)
    target = Path('tests/fixtures/hevc-rext-generated.json')
    temporary = target.with_suffix('.tmp')
    temporary.write_text(json.dumps(result, indent=1) + '\n')
    temporary.replace(target)
    print(json.dumps({'fixtures': len(fixtures), 'encoder_failures': len(failures), 'native_revision': revision,
                      'libde265_hash_mismatches': [f['name'] for f in fixtures if not f['libde265_hash_match']]}))


if __name__ == '__main__':
    main()
