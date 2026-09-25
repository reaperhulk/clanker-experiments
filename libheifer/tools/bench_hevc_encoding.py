#!/usr/bin/env python3
"""Rate/distortion of a built-in encoder against libheif's native plugin (default preset and tune):
HEVC (hpvca against x265, read back by libde265) or AVC (rusty_h264 against x264, read back by
OpenH264). Both encode the same RGB photographs through libheif at each quality, the oracle
decodes both, and luma PSNR against the source gives Bjontegaard rate differences. Not run in CI
(needs network access for the Kodak test images)."""
import os
import argparse
import glob
import hashlib
import json
import math
import subprocess
from pathlib import Path

import numpy as np

KODAK = ['01', '03', '05', '07', '13', '15', '19', '23']
QUALITIES = [10, 20, 30, 40, 50, 60, 70, 80, 90, 95]


def luma_psnr(a, b):
    a = np.frombuffer(a, np.uint8).astype(float).reshape(-1, 3) @ [0.299, 0.587, 0.114]
    b = np.frombuffer(b, np.uint8).astype(float).reshape(-1, 3) @ [0.299, 0.587, 0.114]
    return 10 * math.log10(255 ** 2 / ((a - b) ** 2).mean())


def bd_rate(r1, p1, r2, p2):
    f1, f2 = np.polyfit(p1, np.log(r1), 3), np.polyfit(p2, np.log(r2), 3)
    lo, hi = max(min(p1), min(p2)), min(max(p1), max(p2))
    i1, i2 = np.polyint(f1), np.polyint(f2)
    d = ((np.polyval(i2, hi) - np.polyval(i2, lo)) - (np.polyval(i1, hi) - np.polyval(i1, lo))) / (hi - lo)
    return (math.exp(d) - 1) * 100


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--codec', choices=['hevc', 'avc'], default='hevc')
    p.add_argument('--reference-build')
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--work')
    p.add_argument('--output')
    a = p.parse_args()
    native, builtin, fmt = {'hevc': ('x265', 'libheifer-hevc', 1), 'avc': ('x264', 'libheifer-avc', 2)}[a.codec]
    a.reference_build = a.reference_build or f'.build/reference-{native}'
    a.work = a.work or f'.build/{a.codec}-rd'
    a.output = a.output or f'.build/{a.codec}-rd-report.json'
    env = dict(os.environ, RD_FORMAT=str(fmt))
    from PIL import Image
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    ref = Path(a.reference_build).resolve()
    libs = {native: ref / 'libheif/libheif.so', 'candidate': Path(a.candidate).resolve()}
    for name, lib in libs.items():
        subprocess.run(['cc', '-O2', f'-I{ref}', '-Itests/upstream/libheif/api', 'tests/hevc_rd.c', str(lib),
                        f'-Wl,-rpath,{lib.parent}', '-o', str(work / name)], check=True)
    images = []
    for n in KODAK:
        png = work / f'kodim{n}.png'
        if not png.exists():
            subprocess.run(['curl', '-fsSL', '--retry', '4', '-o', str(png),
                            f'https://r0k.us/graphics/kodak/kodak/kodim{n}.png'], check=True)
        im = Image.open(png).convert('RGB')
        raw = work / f'kodim{n}.rgb'
        raw.write_bytes(im.tobytes())
        images.append((raw, im.size, hashlib.sha256(png.read_bytes()).hexdigest()))
    results = {}
    for raw, (w, h), _ in images:
        for name in libs:
            points = []
            for q in QUALITIES:
                heic, out = work / f'{name}.heic', work / f'{name}.rgb'
                args = [str(work / name), 'enc', str(w), str(h), str(q), str(raw), str(heic)]
                subprocess.run(args + [native if name == native else builtin], check=True, capture_output=True, env=env)
                subprocess.run([str(work / native), 'dec', str(heic), str(out)], check=True, capture_output=True)
                points.append((q, heic.stat().st_size, luma_psnr(raw.read_bytes(), out.read_bytes())))
            results.setdefault(raw.stem, {})[name] = points
    rates = {}
    for image, r in results.items():
        x = [t for t in r[native] if 28 < t[2] < 50]
        c = [t for t in r['candidate'] if 28 < t[2] < 50]
        rates[image] = bd_rate([t[1] for t in x], [t[2] for t in x], [t[1] for t in c], [t[2] for t in c])
    report = dict(scope=__doc__, qualities=QUALITIES, images={raw.stem: sha for raw, _, sha in images},
                  bd_rate_percent=rates, mean_bd_rate_percent=float(np.mean(list(rates.values()))),
                  mean_psnr_difference=[float(np.mean([r['candidate'][i][2] - r[native][i][2] for r in results.values()]))
                                        for i in range(len(QUALITIES))],
                  points=results)
    Path(a.output).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k != 'points'}, indent=2))


if __name__ == '__main__':
    main()
