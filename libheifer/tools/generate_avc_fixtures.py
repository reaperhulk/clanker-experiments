#!/usr/bin/env python3
"""Generate owned H.264 intra streams with a pinned test-only native x264 encoder.

x264 is never a candidate dependency. It only produces Annex B streams whose
decoded pixels are compared between the native OpenH264/libheif oracle and the
Rust candidate. Streams the oracle rejects (other chroma formats, high bit
depths, interlacing, lossless) are retained for error-behavior parity.
"""
import argparse
import hashlib
import itertools
import json
import subprocess
from pathlib import Path

REVISION = 'b35605ace3ddf7c1a5d67a2eb553f034aef41d55'

CSP = {'i420': (2, 2), 'i422': (2, 1), 'i444': (1, 1), 'i400': None}


def picture(w, h, csp, depth, pattern):
    """Deterministic planar input; patterns exercise flat, directional and noisy intra modes."""
    state = 0x1234567 + pattern * 7919 + w * 31 + h

    def noise():
        nonlocal state
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        return state >> 16

    top = (1 << depth) - 1
    planes = [(w, h)]
    if CSP[csp]:
        dx, dy = CSP[csp]
        planes += [((w + dx - 1) // dx, (h + dy - 1) // dy)] * 2
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
                elif pattern == 5:
                    # Alternating noisy and smooth macroblocks (I_PCM next to predicted ones).
                    mb = 16 if c == 0 or not CSP[csp] else 16 // CSP[csp][0]
                    v = noise() & 255 if ((x // mb) + (y // mb)) & 1 else (x * 4 + y * 2 + c * 40) & 255
                elif pattern == 3:
                    v = (x * x + y * 7 * (c + 1) + (noise() & 15)) & 255
                else:
                    v = 128 + ((x - pw // 2) * (y - ph // 2) * (c + 1) >> 3) + (noise() & 31) - 16
                v = max(0, min(255, v))
                if depth > 8:
                    raw += (v * top // 255).to_bytes(2, 'little')
                else:
                    raw.append(v)
    return bytes(raw)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--install', default='.build/x264-install')
    p.add_argument('--source', default='.build/x264-source')
    p.add_argument('--work', default='.build/avc-generated')
    a = p.parse_args()
    source = Path(a.source).resolve()
    revision = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if revision != REVISION:
        raise SystemExit('wrong native x264 revision')
    encoder = Path(a.install).resolve() / 'bin/x264'
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    fixtures = []
    failures = []
    names = set()

    def add(name, w, h, options=(), csp='i420', depth=8, pattern=1):
        assert name not in names, name
        names.add(name)
        raw = picture(w, h, csp, depth, pattern)
        inp = work / f'{name}.yuv'
        out = work / f'{name}.264'
        inp.write_bytes(raw)
        command = [str(encoder), '--quiet', '--no-progress', '--threads', '1', '--frames', '1', '--keyint', '1',
                   '--input-res', f'{w}x{h}', '--input-csp', csp, '--input-depth', str(depth),
                   '--output-csp', csp, '--output-depth', str(depth), *options, '-o', str(out), str(inp)]
        run = subprocess.run(command, capture_output=True)
        record = dict(name=name, width=w, height=h, csp=csp, depth=depth, pattern=pattern,
                      command=[Path(command[0]).name, *command[1:-3], '-o', f'{name}.264', f'{name}.yuv'],
                      input_sha256=hashlib.sha256(raw).hexdigest())
        if run.returncode:
            failures.append(dict(record, returncode=run.returncode, stderr=run.stderr.decode(errors='replace')))
            return
        data = out.read_bytes()
        fixtures.append(dict(record, sha256=hashlib.sha256(data).hexdigest(), hex=data.hex()))

    profiles = {
        'baseline': ['--profile', 'baseline'],
        'main': ['--profile', 'main'],
        'high': ['--profile', 'high'],
        'high-cavlc': ['--profile', 'high', '--no-cabac'],
    }
    for (w, h), (profile, options), pattern in itertools.product(
            [(2, 2), (16, 16), (18, 34), (64, 48), (130, 66), (8, 200), (200, 8)], profiles.items(), [0, 1, 3]):
        add(f'size-{w}x{h}-{profile}-{pattern}', w, h, ['--qp', '26', *options], pattern=pattern)
    for (profile, options), qp, pattern in itertools.product(profiles.items(), [1, 6, 12, 20, 30, 38, 45, 51], [1, 2, 4]):
        add(f'qp-{profile}-{qp}-{pattern}', 48, 32, ['--qp', str(qp), *options], pattern=pattern)
    cqm = work / 'custom.cfg'
    matrices = []
    for name, size in [('INTRA4X4_LUMA', 16), ('INTRA4X4_CHROMAU', 16), ('INTRA4X4_CHROMAV', 16),
                       ('INTER4X4_LUMA', 16), ('INTER4X4_CHROMAU', 16), ('INTER4X4_CHROMAV', 16),
                       ('INTRA8X8_LUMA', 64), ('INTER8X8_LUMA', 64)]:
        values = [4 + (i * 37 + len(name) * 11) % 90 for i in range(size)]
        matrices.append(f'{name} =\n' + ','.join(map(str, values)))
    cqm.write_text('\n'.join(matrices) + '\n')
    for (profile, options), matrix, qp in itertools.product(
            [('high', profiles['high']), ('high-cavlc', profiles['high-cavlc'])], ['flat', 'jvt', 'custom'], [10, 28, 36, 44, 50, 51]):
        extra = ['--cqmfile', str(cqm)] if matrix == 'custom' else ['--cqm', matrix]
        add(f'cqm-{profile}-{matrix}-{qp}', 64, 48, ['--qp', str(qp), *options, *extra], pattern=4)
    # x264 codes an I-frame at `--qp` minus its ipratio offset (about 3), so the
    # sweeps above never reach QP 49..51. `--ipratio 1` makes the slice QP exact.
    # QP 50/51 4x4 blocks with nonzero levels under the custom matrix (openh264's
    # uint16 factors and its uninitialized QP 51 row): noisy input, 4x4 only, no deadzone.
    for (profile, options), qp, pattern in itertools.product(
            [('high', profiles['high']), ('high-cavlc', profiles['high-cavlc'])], [50, 51], [1, 3]):
        add(f'cqm-{profile}-custom-i4x4-{qp}-{pattern}', 64, 48,
            ['--qp', str(qp), '--ipratio', '1', *options, '--cqmfile', str(cqm), '--no-8x8dct', '--partitions', 'i4x4',
             '--deadzone-intra', '0', '--trellis', '0'], pattern=pattern)
    for (profile, options), qp, pattern in itertools.product(profiles.items(), [49, 50, 51], [1, 3]):
        add(f'qp-exact-{profile}-{qp}-{pattern}', 48, 32, ['--qp', str(qp), '--ipratio', '1', *options], pattern=pattern)
    for (profile, options), deblock in itertools.product(profiles.items(), ['off', '-6:-6', '6:6', '3:-2', '-2:4']):
        extra = ['--no-deblock'] if deblock == 'off' else ['--deblock', deblock]
        add(f'deblock-{profile}-{deblock}', 64, 48, ['--qp', '36', *options, *extra], pattern=2)
    for (profile, options), slices in itertools.product(profiles.items(), [['--slices', '3'], ['--slice-max-mbs', '5'], ['--slice-max-size', '200']]):
        add(f'slices-{profile}-{slices[0][2:]}', 80, 64, ['--qp', '24', *options, *slices], pattern=3)
    for (profile, options), extra in itertools.product(profiles.items(), [
            ['--constrained-intra'], ['--chroma-qp-offset', '-12'], ['--chroma-qp-offset', '12'],
            ['--partitions', 'none'], ['--partitions', 'i4x4'], ['--partitions', 'i8x8', '--8x8dct'],
            ['--no-8x8dct'], ['--trellis', '2'], ['--no-psy'], ['--aq-mode', '0'],
            ['--colorprim', 'bt709', '--transfer', 'bt709', '--colormatrix', 'bt709', '--input-range', 'pc', '--range', 'pc', '--sar', '4:3'],
            ['--nal-hrd', 'vbr', '--vbv-maxrate', '1000', '--vbv-bufsize', '1000', '--bitrate', '800']]):
        label = '_'.join(e[2:] if e.startswith('--') else e for e in extra).replace(':', '_')
        add(f'option-{profile}-{label}', 48, 48, ['--qp', '22', *options, *extra], pattern=4)
    for preset, pattern in itertools.product(['ultrafast', 'superfast', 'veryfast', 'faster', 'fast', 'medium', 'slow', 'slower', 'veryslow', 'placebo'], [1, 4]):
        add(f'preset-{preset}-{pattern}', 96, 64, ['--preset', preset, '--crf', '23'], pattern=pattern)
    # Monochrome (chroma_format_idc 0): absent chroma syntax, 4:0:0 CBP mappings.
    for (profile, options), qp, pattern, extra in itertools.product(
            [('high', profiles['high']), ('high-cavlc', profiles['high-cavlc'])], [4, 16, 28, 40], [1, 3, 4],
            [[], ['--partitions', 'none'], ['--partitions', 'i4x4', '--no-8x8dct']]):
        label = '_'.join(e[2:] if e.startswith('--') else e for e in extra) or 'default'
        add(f'mono-{profile}-{qp}-{pattern}-{label}', 48, 32, ['--qp', str(qp), *options, *extra], csp='i400', pattern=pattern)
    # Above libheif's 65536-pixel floor for the ispe-padded limit (resource limits).
    add('limits-272x256-high', 272, 256, ['--qp', '30', *profiles['high']], pattern=4)
    add('limits-270x250-baseline', 270, 250, ['--qp', '30', *profiles['baseline']], pattern=4)
    # I_PCM macroblocks (x264 picks PCM under RD at low QP without psy-rd), alone and
    # mixed with predicted macroblocks: CABAC re-initialization, PCM neighbours.
    for (profile, options), qp, pattern in itertools.product(profiles.items(), [1, 6, 10], [3, 5]):
        add(f'pcm-{profile}-{qp}-{pattern}', 64, 48,
            ['--qp', str(qp), '--ipratio', '1', '--no-psy', '--subme', '9', *options], pattern=pattern)
    for (profile, options) in [('high', profiles['high']), ('high-cavlc', profiles['high-cavlc'])]:
        add(f'pcm-{profile}-crop-80x40', 80, 40, ['--qp', '6', '--ipratio', '1', '--no-psy', '--subme', '9', *options], pattern=5)
        add(f'pcm-mono-{profile}', 48, 32, ['--qp', '6', '--ipratio', '1', '--no-psy', '--subme', '9', *options], csp='i400', pattern=5)
    # Streams outside OpenH264 still-image support: retained for exact error parity.
    for csp in ['i400', 'i422', 'i444']:
        add(f'format-{csp}-8', 32, 32, ['--qp', '26'], csp=csp)
    add('format-i420-10', 32, 32, ['--qp', '26', '--profile', 'high10'], depth=10)
    add('format-lossless', 32, 32, ['--qp', '0'])
    add('format-interlaced', 32, 64, ['--qp', '26', '--interlaced'])
    add('format-fake-interlaced', 32, 64, ['--qp', '26', '--fake-interlaced'])
    result = dict(generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), x264_revision=revision,
                  encoder_sha256=hashlib.sha256(encoder.read_bytes()).hexdigest(), custom_cqm=cqm.read_text(),
                  fixtures=fixtures, encoder_failures=failures)
    target = Path('tests/fixtures/avc-generated.json')
    temporary = target.with_suffix('.tmp')
    temporary.write_text(json.dumps(result, indent=1) + '\n')
    temporary.replace(target)
    print(json.dumps({'fixtures': len(fixtures), 'encoder_failures': len(failures), 'native_revision': revision}))


if __name__ == '__main__':
    main()
