#!/usr/bin/env python3
"""Generate owned H.266/VVC intra streams with the pinned test-only native vvenc library.

vvenc is never a candidate dependency. tests/vvc_fixture_encoder.c drives it the
way libheif's vvenc plugin does (vvenc_init_default with a preset and QP,
explicit bit depths, 4:0:0 through m_internChromaFormat) and writes one intra
picture as an Annex B stream. Decoded pixels are compared between the native
vvdec/libheif oracle and the Rust candidate. Configurations vvenc refuses
(4:2:2, 4:4:4, odd 4:2:0 sizes) are recorded as encoder failures.
"""
import argparse
import hashlib
import itertools
import json
import subprocess
from pathlib import Path

REVISION = '9428ea8636ae7f443ecde89999d16b2dfc421524'  # v1.14.0, tools/build_reference.py VVENC

# Plane subsampling shifts, matching vvenc_get_width/height_of_component.
CHROMA = {400: None, 420: (1, 1), 422: (1, 0), 444: (0, 0)}


def picture(w, h, chroma, depth, pattern):
    """Deterministic planar 16-bit input; patterns exercise flat, directional and noisy intra modes."""
    state = 0x1234567 + pattern * 7919 + w * 31 + h

    def noise():
        nonlocal state
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        return state >> 16

    top = (1 << depth) - 1
    planes = [(w, h)]
    if CHROMA[chroma]:
        sx, sy = CHROMA[chroma]
        planes += [(w >> sx, h >> sy)] * 2
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
                elif pattern == 5:
                    # Screen-content-like: sharp text strokes on a flat background.
                    v = 16 + 200 * (((x * 5 + y * 3) % 11) < 2) + c * 30 if (x // 8 + y // 8) & 1 else 128 + c * 20
                else:
                    v = 128 + ((x - pw // 2) * (y - ph // 2) * (c + 1) >> 3) + (noise() & 31) - 16
                v = max(0, min(255, v))
                if depth > 8:
                    # Use the low bits too, so high bit depths are not just scaled 8-bit data.
                    v = min(top, v * top // 255 + (noise() & ((1 << (depth - 8)) - 1)))
                raw += v.to_bytes(2, 'little')
    return bytes(raw)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--install', default='.build/vvenc-install')
    p.add_argument('--source', default='.build/vvenc-source')
    p.add_argument('--work', default='.build/vvc-generated')
    a = p.parse_args()
    source = Path(a.source).resolve()
    revision = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if revision != REVISION:
        raise SystemExit('wrong native vvenc revision')
    install = Path(a.install).resolve()
    library = install / 'lib/libvvenc.so'
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    helper_source = Path('tests/vvc_fixture_encoder.c')
    encoder = work / 'vvc_fixture_encoder'
    subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Werror', f'-I{install / "include"}', str(helper_source),
                    str(library), f'-Wl,-rpath,{library.parent}', '-o', str(encoder)], check=True)
    fixtures = []
    failures = []
    names = set()

    def add(name, w, h, preset='medium', qp=32, options=(), chroma=420, depth=8, pattern=1):
        assert name not in names, name
        names.add(name)
        raw = picture(w, h, chroma, depth, pattern)
        inp = work / f'{name}.yuv'
        out = work / f'{name}.266'
        inp.write_bytes(raw)
        out.unlink(missing_ok=True)
        command = [str(encoder), str(w), str(h), str(chroma), str(depth), preset, str(qp), str(inp), str(out), *options]
        run = subprocess.run(command, capture_output=True)
        record = dict(name=name, width=w, height=h, chroma=chroma, depth=depth, preset=preset, qp=qp,
                      options=list(options), pattern=pattern,
                      command=[encoder.name, *command[1:7], f'{name}.yuv', f'{name}.266', *options],
                      input_sha256=hashlib.sha256(raw).hexdigest())
        if run.returncode:
            failures.append(dict(record, returncode=run.returncode, stderr=run.stderr.decode(errors='replace')))
            return
        data = out.read_bytes()
        fixtures.append(dict(record, sha256=hashlib.sha256(data).hexdigest(), hex=data.hex()))

    presets = ['faster', 'fast', 'medium', 'slow', 'slower']
    formats = [(420, 8), (420, 10), (400, 8), (400, 10)]
    # Every preset (tool set) per format; libheif itself always uses medium.
    for preset, (chroma, depth), pattern in itertools.product(presets, formats, [0, 1, 4]):
        add(f'preset-{preset}-{chroma}-{depth}-{pattern}', 64, 48, preset, 32, chroma=chroma, depth=depth, pattern=pattern)
    # QP sweep, including the extremes of the 0..63 range.
    for (chroma, depth), qp, pattern in itertools.product(formats, [0, 1, 4, 12, 22, 37, 45, 51, 57, 63], [1, 3]):
        add(f'qp-{chroma}-{depth}-{qp}-{pattern}', 32, 32, 'fast', qp, chroma=chroma, depth=depth, pattern=pattern)
    # Sizes: tiny, non-multiple-of-8 (automatic conformance window), thin and larger pictures.
    sizes = [(8, 8), (16, 16), (18, 14), (26, 34), (66, 34), (8, 72), (72, 8), (130, 66), (136, 136), (256, 160), (250, 162)]
    for (w, h), (chroma, depth) in itertools.product(sizes, formats):
        add(f'size-{w}x{h}-{chroma}-{depth}', w, h, 'medium', 37, chroma=chroma, depth=depth, pattern=4)
    for (w, h), depth in itertools.product([(17, 13), (9, 31), (33, 1), (1, 1), (129, 67)], [8, 10]):
        add(f'size-{w}x{h}-400-{depth}', w, h, 'medium', 37, chroma=400, depth=depth, pattern=4)
    # vvenc's "lossless" cost mode. vvenc 1.14 has no true VVC lossless coding: this
    # only forces slice QP 0 with lossless RD costs, so decoded samples differ from the input.
    for (chroma, depth), pattern in itertools.product(formats, [1, 3]):
        add(f'costmode-lossless-{chroma}-{depth}-{pattern}', 32, 24, 'medium', 32, ['CostMode=lossless'], chroma=chroma, depth=depth, pattern=pattern)
    # Individual coding tools toggled against the medium preset.
    tools = [['ALF=0'], ['CCALF=0'], ['ALF=1', 'CCALF=1', 'UseNonLinearAlfLuma=1'], ['SAO=0'], ['LoopFilterDisable=1', 'EDO=0'],
             ['LoopFilterOffsetInPPS=0', 'LoopFilterBetaOffset_div2=-6', 'LoopFilterTcOffset_div2=6'],
             ['LoopFilterBetaOffset_div2=4', 'LoopFilterTcOffset_div2=-3'],
             ['LMCSEnable=1'], ['LMCSEnable=0'], ['DualITree=0'], ['MIP=0'], ['ISP=0'], ['ISP=1'], ['LFNST=0'],
             ['MTS=0'], ['MTS=1'], ['MRL=0'], ['LMChroma=0'], ['JointCbCr=0'], ['TransformSkip=0', 'BDPCM=0'], ['TransformSkip=1'],
             ['TransformSkip=1', 'BDPCM=1'], ['ChromaTS=1'], ['DepQuant=0'], ['DepQuant=0', 'SignHideFlag=1'],
             ['CbQpOffset=-6', 'CrQpOffset=6'], ['CTUSize=32'], ['CTUSize=64'], ['MaxMTTDepthI=0'],
             ['WaveFrontSynchro=1'], ['IBC=1'], ['Tiles=2x2', 'Level=6.2'], ['Tiles=1x2', 'Level=6.2'], ['TreatAsSubPic=1']]
    for extra, (chroma, depth) in itertools.product(tools, [(420, 8), (400, 10)]):
        label = '_'.join(e.replace('=', '').replace('_div2', '').replace('.', '') for e in extra)
        pattern = 5 if extra in (['IBC=1'], ['TransformSkip=1', 'BDPCM=1']) else 4
        w, h = (160, 96) if 'Tiles' in label else (64, 48)
        add(f'tool-{label}-{chroma}-{depth}', w, h, 'medium', 27, extra, chroma=chroma, depth=depth, pattern=pattern)
    # Formats and sizes vvenc refuses: retained as encoder failures.
    for chroma, depth in itertools.product([422, 444], [8, 10]):
        add(f'format-{chroma}-{depth}', 32, 32, 'medium', 32, chroma=chroma, depth=depth)
    add('format-420-odd-17x13', 17, 13, 'medium', 32)
    add('format-420-12', 32, 32, 'medium', 32, depth=12)
    result = dict(generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), vvenc_revision=revision,
                  helper_sha256=hashlib.sha256(helper_source.read_bytes()).hexdigest(),
                  encoder_sha256=hashlib.sha256(library.read_bytes()).hexdigest(),
                  fixtures=fixtures, encoder_failures=failures)
    target = Path('tests/fixtures/vvc-generated.json')
    temporary = target.with_suffix('.tmp')
    temporary.write_text(json.dumps(result, indent=1) + '\n')
    temporary.replace(target)
    print(json.dumps({'fixtures': len(fixtures), 'encoder_failures': len(failures), 'native_revision': revision}))


if __name__ == '__main__':
    main()
