#!/usr/bin/env python3
"""Generate owned HTJ2K codestreams with a pinned test-only OpenJPH encoder."""
import argparse
import hashlib
import itertools
import json
import subprocess
from pathlib import Path

REVISION = '8c2826fdaaac3b0334ff5bc2ed2a8ec153c99a35'


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--encoder', required=True, help='ojph_compress built from the pinned OpenJPH revision')
    p.add_argument('--source', required=True, help='OpenJPH checkout')
    p.add_argument('--work', default='.build/htj2k-generated')
    a = p.parse_args()
    source = Path(a.source).resolve()
    revision = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if revision != REVISION:
        raise SystemExit('wrong OpenJPH revision')
    encoder = Path(a.encoder).resolve()
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    fixtures, failures = [], []

    def add(name, w, h, components=3, depth=8, signed=False, sampling=(1, 1), extra=(), pattern=0, ppm=False):
        scales = [(1, 1)] + [sampling] * (components - 1)
        raw = bytearray()
        for c, (dx, dy) in enumerate(scales):
            for y in range((h + dy - 1) // dy):
                for x in range((w + dx - 1) // dx):
                    if pattern == 0:
                        value = (x * 73 + y * 151 + x * y * 17 + c * 113) % (1 << depth)
                    elif pattern == 1:  # smooth gradient
                        value = ((x + y + c * 5) * ((1 << depth) - 1)) // max(1, w + h + 10)
                    elif pattern == 3:  # sparse low-amplitude spikes: room for scaled coefficients and SPP candidates
                        value = (1 << (depth - 1)) + (((x * 5 + y * 3) % (1 << (depth - 5))) if (x * 7 + y * 3 + c) % 11 == 0 else 0)
                    else:  # sparse detail on a flat field
                        value = (1 << (depth - 1)) + ((1 << (depth - 2)) if (x * 7 + y * 3 + c) % 29 == 0 else 0)
                    if signed:
                        value -= 1 << (depth - 1)
                    raw.extend(value.to_bytes(1 if depth <= 8 else 2, 'little', signed=signed))
        out = work / (name + '.j2c')
        if ppm:
            # Interleaved big-endian samples; OpenJPH only colour-transforms PPM input.
            size = 1 if depth <= 8 else 2
            planes = [raw[c * w * h * size:(c + 1) * w * h * size] for c in range(3)]
            body = bytearray()
            for i in range(w * h):
                for c in range(3):
                    body.extend(planes[c][i * size:(i + 1) * size][::-1])
            inp = work / (name + '.ppm')
            inp.write_bytes(f'P6\n{w} {h}\n{(1 << depth) - 1}\n'.encode() + body)
            command = [str(encoder), '-i', str(inp), '-o', str(out), *extra]
        else:
            inp = work / (name + '.yuv')
            inp.write_bytes(raw)
            command = [str(encoder), '-i', str(inp), '-o', str(out), '-dims', f'{{{w},{h}}}',
                       '-num_comps', str(components), '-signed', ','.join(['true' if signed else 'false'] * components),
                       '-bit_depth', ','.join([str(depth)] * components),
                       '-downsamp', ','.join(f'{{{dx},{dy}}}' for dx, dy in scales), *extra]
        run = subprocess.run(command, capture_output=True)
        if run.returncode or not out.exists():
            failures.append(dict(name=name, command=command[1:], returncode=run.returncode,
                                 stdout=run.stdout.decode(errors='replace'), stderr=run.stderr.decode(errors='replace')))
            return
        data = out.read_bytes()
        out.unlink()
        fixtures.append(dict(name=name, width=w, height=h, components=components, depth=depth, signed=signed,
                             sampling=scales, command=[Path(command[0]).name] + [str(x) for x in command[1:]][4:],
                             input_sha256=hashlib.sha256(raw).hexdigest(), sha256=hashlib.sha256(data).hexdigest(),
                             hex=data.hex()))

    def decomps(w, h):
        n, m = 0, min(w, h)
        while m > 1 and n < 5:
            m = (m + 1) // 2
            n += 1
        return str(n)

    for w, h in [(1, 1), (2, 3), (7, 5), (16, 16), (17, 19), (65, 31), (100, 37)]:
        for components, depth, signed, reversible in itertools.product([1, 3], [8, 10, 12, 16], [False, True], [True, False]):
            add(f'{w}x{h}-{components}-{depth}-{int(signed)}-{int(reversible)}', w, h, components, depth, signed,
                extra=['-reversible', 'true' if reversible else 'false', '-num_decomps', decomps(w, h)])
    for (bw, bh), reversible, pattern in itertools.product(
            [(4, 4), (8, 8), (16, 16), (32, 32), (64, 64), (4, 1024), (1024, 4), (128, 32), (32, 128), (8, 512)],
            [True, False], [0, 1, 2]):
        add(f'block-{bw}x{bh}-{int(reversible)}-{pattern}', 97, 83, pattern=pattern,
            extra=['-reversible', 'true' if reversible else 'false', '-block_size', f'{{{bh},{bw}}}'])
    for n, reversible in itertools.product(range(6), [True, False]):
        add(f'decomps-{n}-{int(reversible)}', 61, 45, extra=['-reversible', 'true' if reversible else 'false', '-num_decomps', str(n)])
    for order, tile, parts in itertools.product(['LRCP', 'RLCP', 'RPCL', 'PCRL', 'CPRL'], [None, (32, 24)], [None, 'R', 'C', 'RC']):
        extra = ['-reversible', 'true', '-prog_order', order, '-block_size', '{8,8}']
        if tile:
            extra += ['-tile_size', f'{{{tile[0]},{tile[1]}}}']
        if parts:
            extra += ['-tileparts', parts]
        add(f'order-{order}-{int(bool(tile))}-{parts or "none"}', 70, 50, extra=extra)
    for precincts, reversible in itertools.product(['{16,16}', '{32,32},{16,16}', '{64,64},{32,32},{8,8}'], [True, False]):
        add(f'precincts-{precincts.count("{")}-{int(reversible)}', 90, 70,
            extra=['-reversible', 'true' if reversible else 'false', '-precincts', precincts, '-block_size', '{8,8}'])
    for qstep in ['0.5', '0.1', '0.01', '0.001', '0.0001']:
        add(f'qstep-{qstep}', 64, 48, pattern=1, extra=['-reversible', 'false', '-qstep', qstep])
        add(f'qstep-detail-{qstep}', 64, 48, depth=12, pattern=2, extra=['-reversible', 'false', '-qstep', qstep])
    for sampling, reversible in itertools.product([(2, 1), (2, 2), (1, 2), (4, 1)], [True, False]):
        add(f'sampling-{sampling[0]}x{sampling[1]}-{int(reversible)}', 33, 29, sampling=sampling,
            extra=['-reversible', 'true' if reversible else 'false'])
    for reversible, depth in itertools.product([True, False], [8, 16]):
        add(f'colour-{int(reversible)}-{depth}', 40, 30, depth=depth, ppm=True,
            extra=['-reversible', 'true' if reversible else 'false', '-colour_trans', 'true'])
    for offset, tile_offset in [((3, 5), None), ((17, 9), (1, 2)), ((9, 11), (5, 7))]:
        extra = ['-reversible', 'true', '-image_offset', f'{{{offset[0]},{offset[1]}}}', '-tile_size', '{24,20}', '-block_size', '{8,8}']
        if tile_offset:
            extra += ['-tile_offset', f'{{{tile_offset[0]},{tile_offset[1]}}}']
        add(f'offset-{offset[0]}-{offset[1]}-{tile_offset}', 45, 38, extra=extra)
    add('tlm', 50, 40, extra=['-reversible', 'true', '-tlm_marker', 'true', '-tileparts', 'C', '-tile_size', '{25,20}'])
    add('comment', 20, 20, extra=['-reversible', 'true', '-com', 'libheifer HTJ2K'])
    for depth, reversible in itertools.product([1, 2, 4, 7], [True, False]):
        add(f'lowdepth-{depth}-{int(reversible)}', 23, 21, components=1, depth=depth,
            extra=['-reversible', 'true' if reversible else 'false'])

    # Single code-block bases for constructed refinement-pass packets.
    for (w, h), reversible, depth in itertools.product(
            [(8, 8), (13, 7), (16, 5), (31, 9), (64, 64), (6, 2), (33, 33), (64, 22), (21, 13), (40, 1), (1, 40), (17, 4)],
            [True, False], [8, 12]):
        extra = ['-reversible', 'true' if reversible else 'false', '-num_decomps', '0', '-block_size', '{64,64}']
        if not reversible:
            extra += ['-qstep', '0.002']
        add(f'single-{w}x{h}-{int(reversible)}-{depth}', w, h, components=1, depth=depth, extra=extra)
    for (w, h), reversible in itertools.product([(64, 64), (13, 7), (33, 33), (64, 22), (21, 13), (17, 4)], [True, False]):
        extra = ['-reversible', 'true' if reversible else 'false', '-num_decomps', '0', '-block_size', '{64,64}']
        if not reversible:
            extra += ['-qstep', '0.002']
        add(f'single-lowamp-{w}x{h}-{int(reversible)}', w, h, components=1, depth=12, pattern=3, extra=extra)
    add('single-128x32-1-8', 128, 32, components=1, extra=['-reversible', 'true', '-num_decomps', '0', '-block_size', '{128,32}'])

    result = dict(generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), openjph_revision=revision,
                  encoder_sha256=hashlib.sha256(encoder.read_bytes()).hexdigest(), fixtures=fixtures, encoder_failures=failures)
    target = Path('tests/fixtures/htj2k-generated.json')
    temporary = target.with_suffix('.tmp')
    temporary.write_text(json.dumps(result, indent=2) + '\n')
    temporary.replace(target)
    print(json.dumps({'fixtures': len(fixtures), 'failures': len(failures)}))


if __name__ == '__main__':
    main()
