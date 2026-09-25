#!/usr/bin/env python3
"""Built-in VVC encoder quality against libheif with vvenc: deterministic synthetic RGB images are
encoded through libheif by both at several qualities, the oracle (vvdec) decodes both, and luma PSNR
against the source gives a piecewise-linear Bjontegaard rate difference per image, which must stay
under a bound. Catches encoder changes that keep files decodable but lose quality."""
import argparse
import hashlib
import json
import math
import os
import random
import subprocess
from pathlib import Path

QUALITIES = [10, 25, 40, 55, 70]
# Recorded per-image BD-rates against vvenc medium (percent); a run fails
# when one grows by more than MARGIN. Screen-like content ('blocks') lacks
# transform skip and BDPCM in the candidate; vvenc's perceptual QP
# adaptation spends more bits on smooth content.
EXPECTED = {'smooth': -2.1, 'texture': 13.6, 'blocks': 42.5}
MARGIN = 10.0


def images():
    """(name, width, height, RGB bytes) for three content types."""
    out = []
    w, h = 190, 126
    rnd = random.Random(1234)
    smooth = bytearray()
    for y in range(h):
        for x in range(w):
            d = math.hypot(x - 70, y - 60)
            smooth += bytes([int(128 + 100 * math.sin(d / 9)) & 255, (x * 255 // w), (y * 255 // h)])
    out.append(('smooth', w, h, bytes(smooth)))
    texture = bytearray()
    for y in range(h):
        for x in range(w):
            v = 128 + 50 * math.sin(x / 3.1) * math.cos(y / 4.7) + 30 * math.sin((x + 2 * y) / 7.3) + rnd.gauss(0, 8)
            g = 128 + 60 * math.cos(x / 11.0 + y / 5.0) + rnd.gauss(0, 6)
            texture += bytes(int(min(255, max(0, c))) for c in (v, g, 255 - v * 0.7))
    out.append(('texture', w, h, bytes(texture)))
    blocks = bytearray()
    for y in range(h):
        for x in range(w):
            cell = ((x // 13) * 7 + (y // 11) * 3) % 5
            edge = 255 if (x % 29 < 2 or y % 23 < 2) else 0
            blocks += bytes([(cell * 50) ^ edge, (cell * 90 + x) & 255, (y * 3 + cell * 20) & 255])
    out.append(('blocks', w, h, bytes(blocks)))
    return out


def luma_psnr(a, b):
    err = 0.0
    for i in range(0, len(a), 3):
        ya = 0.299 * a[i] + 0.587 * a[i + 1] + 0.114 * a[i + 2]
        yb = 0.299 * b[i] + 0.587 * b[i + 1] + 0.114 * b[i + 2]
        err += (ya - yb) ** 2
    mse = err / (len(a) // 3)
    return 99.0 if mse == 0 else 10 * math.log10(255 ** 2 / mse)


def bd_rate(ref, cand):
    """Mean log-rate difference over the common PSNR range, with piecewise
    linear interpolation of log(rate) against PSNR; points as (rate, psnr)."""
    def curve(points):
        pts = sorted((p, math.log(r)) for r, p in points)
        return pts

    def interp(pts, x):
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            if x0 <= x <= x1:
                return y0 + (y1 - y0) * (x - x0) / (x1 - x0) if x1 > x0 else y0
        return None

    r, c = curve(ref), curve(cand)
    lo, hi = max(r[0][0], c[0][0]), min(r[-1][0], c[-1][0])
    if hi <= lo:
        return float('inf')
    steps = 64
    diff = 0.0
    for k in range(steps + 1):
        x = lo + (hi - lo) * k / steps
        diff += interp(c, x) - interp(r, x)
    return (math.exp(diff / (steps + 1)) - 1) * 100


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build', default='.build/reference-vvc')
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--work', default='.build/vvc-encoder-quality')
    p.add_argument('--output', default='.build/vvc-encoder-quality-report.json')
    a = p.parse_args()
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    ref = Path(a.reference_build).resolve()
    libs = {'reference': ref / 'libheif/libheif.so', 'candidate': Path(a.candidate).resolve()}
    for name, lib in libs.items():
        subprocess.run(['cc', '-O2', f'-I{ref}', '-Itests/upstream/libheif/api', 'tests/hevc_rd.c', str(lib),
                        f'-Wl,-rpath,{lib.parent}', '-o', str(work / name)], check=True)
    env = dict(os.environ, RD_FORMAT='5')
    results = {}
    failures = []
    for image, w, h, rgb in images():
        raw = work / f'{image}.rgb'
        raw.write_bytes(rgb)
        points = {}
        errors = []
        for name in libs:
            points[name] = []
            for q in QUALITIES:
                heic, out = work / f'{image}-{name}-{q}.heic', work / f'{image}-{name}-{q}.rgb'
                enc = subprocess.run([str(work / name), 'enc', str(w), str(h), str(q), str(raw), str(heic)],
                                     capture_output=True, env=env)
                dec = enc.returncode or subprocess.run([str(work / 'reference'), 'dec', str(heic), str(out)],
                                                       capture_output=True)
                if enc.returncode or dec.returncode:
                    # A file the oracle cannot decode is a failure.
                    errors.append(dict(encoder=name, quality=q, stderr=(enc.stderr + getattr(dec, 'stderr', b''))
                                       .decode(errors='replace')[:500]))
                    continue
                points[name].append((heic.stat().st_size, luma_psnr(rgb, out.read_bytes())))
        if errors:
            results[image] = dict(errors=errors, points=points)
            failures.append(image)
            continue
        bd = bd_rate(points['reference'], points['candidate'])
        results[image] = dict(bd_rate_percent=bd, points=points)
        if EXPECTED[image] is not None and not bd < EXPECTED[image] + MARGIN:
            failures.append(image)
    report = dict(scope=__doc__, qualities=QUALITIES, expected_bd_rate_percent=EXPECTED, margin=MARGIN, images=results,
                  failures=failures, mismatches=len(failures), **{k + '_sha256': hashlib.sha256(v.read_bytes()).hexdigest() for k, v in libs.items()})
    Path(a.output).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: {'bd_rate_percent': round(v['bd_rate_percent'], 1)} if 'bd_rate_percent' in v else v['errors'][0] for k, v in results.items()}, indent=2))
    print(json.dumps(dict(failures=failures)))
    if failures:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
