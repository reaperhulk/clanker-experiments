#!/usr/bin/env python3
"""Rate/distortion of libheifer's High-profile AVC encoder against libheif's x264 plugin (default
preset slow, tune ssim) in 4:0:0, 4:2:2, 4:4:4 and 10-bit 4:2:0: both encode the same YCbCr samples
(BT.601 full range from Kodak photographs) through libheif at each quality; FFmpeg's H.264 decoder
(test-only; libheif's OpenH264 cannot decode these profiles) decodes both, and luma PSNR gives
Bjontegaard rate differences. Not run in CI (needs network access for the Kodak images)."""
import argparse
import hashlib
import json
import math
import struct
import subprocess
from pathlib import Path

import numpy as np

from bench_hevc_encoding import KODAK, bd_rate

QUALITIES = [10, 20, 30, 40, 50, 60, 70, 80, 90, 95]
FORMATS = [(400, 8), (422, 8), (444, 8), (420, 10), (422, 10), (444, 10)]


def boxes(data, start=0, end=None):
    at, end = start, len(data) if end is None else end
    while at + 8 <= end:
        size, kind = struct.unpack('>I4s', data[at:at + 8])
        header = 8
        if size == 1:
            size, header = struct.unpack('>Q', data[at + 8:at + 16])[0], 16
        elif size == 0:
            size = end - at
        yield kind, at + header, at + size
        at += size


def annexb(heif):
    """The primary AVC item's parameter sets and slice data as an Annex B stream."""
    meta = next((s, e) for k, s, e in boxes(heif) if k == b'meta')
    children = {k: (s, e) for k, s, e in boxes(heif, meta[0] + 4, meta[1])}
    ipco = next((s, e) for k, s, e in boxes(heif, *children[b'iprp']) if k == b'ipco')
    avcc = next(heif[s:e] for k, s, e in boxes(heif, *ipco) if k == b'avcC')
    out = b''
    at = 6
    for _ in range(avcc[5] & 31):
        n = int.from_bytes(avcc[at:at + 2], 'big')
        out += b'\0\0\0\1' + avcc[at + 2:at + 2 + n]
        at += 2 + n
    for _ in range(avcc[at]):
        n = int.from_bytes(avcc[at + 1:at + 3], 'big')
        out += b'\0\0\0\1' + avcc[at + 3:at + 3 + n]
        at += 2 + n
    s, e = children[b'iloc']
    iloc = heif[s:e]
    version = iloc[0]
    offset_size, length_size = iloc[4] >> 4, iloc[4] & 15
    base_size, index_size = iloc[5] >> 4, (iloc[5] & 15) if version else 0
    pos = 6
    count = struct.unpack('>H', iloc[pos:pos + 2])[0] if version < 2 else struct.unpack('>I', iloc[pos:pos + 4])[0]
    pos += 2 if version < 2 else 4
    read = lambda n: (int.from_bytes(iloc[pos:pos + n], 'big'), n)
    item = b''
    for _ in range(count):
        pos += 2 if version < 2 else 4
        if version:
            pos += 2
        pos += 2
        base, n = read(base_size); pos += n
        extents = struct.unpack('>H', iloc[pos:pos + 2])[0]; pos += 2
        for _ in range(extents):
            if version and index_size:
                pos += index_size
            off, n = read(offset_size); pos += n
            length, n = read(length_size); pos += n
            item += heif[base + off:base + off + length]
        break
    at = 0
    while at + 4 <= len(item):
        n = int.from_bytes(item[at:at + 4], 'big')
        out += b'\0\0\0\1' + item[at + 4:at + 4 + n]
        at += 4 + n
    return out


def planes(rgb, chroma, depth):
    """BT.601 full-range YCbCr, subsampled by averaging; 8-bit or 16-bit little-endian samples."""
    r, g, b = (rgb[..., i].astype(float) for i in range(3))
    top = (1 << depth) - 1
    y = 0.299 * r + 0.587 * g + 0.114 * b
    cb = 128 + (b - y) / 1.772
    cr = 128 + (r - y) / 1.402
    scale = top / 255
    out = [y * scale]
    if chroma != 400:
        for c in (cb, cr):
            c = c * scale
            if chroma in (420, 422):
                c = (c[:, 0::2] + c[:, 1::2]) / 2
            if chroma == 420:
                c = (c[0::2] + c[1::2]) / 2
            out.append(c)
    dtype = '<u2' if depth > 8 else 'u1'
    return [np.clip(np.round(p), 0, top).astype(dtype) for p in out]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build', default='.build/reference-x264')
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--ffmpeg', default='.build/ffmpeg-install/bin/ffmpeg')
    p.add_argument('--work', default='.build/avc-high-rd')
    p.add_argument('--output', default='.build/avc-high-rd-report.json')
    p.add_argument('--images', default='01,05,13,23')
    a = p.parse_args()
    from PIL import Image
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    ref = Path(a.reference_build).resolve()
    libs = {'x264': ref / 'libheif/libheif.so', 'candidate': Path(a.candidate).resolve()}
    ids = {'x264': 'x264', 'candidate': 'libheifer-avc'}
    for name, lib in libs.items():
        subprocess.run(['cc', '-O2', f'-I{ref}', '-Itests/upstream/libheif/api', 'tests/avc_high_rd.c', str(lib),
                        f'-Wl,-rpath,{lib.parent}', '-o', str(work / name)], check=True)
    images = {}
    for n in a.images.split(','):
        png = work / f'kodim{n}.png'
        if not png.exists():
            subprocess.run(['curl', '-fsSL', '--retry', '4', '-o', str(png),
                            f'https://r0k.us/graphics/kodak/kodak/kodim{n}.png'], check=True)
        images[f'kodim{n}'] = (np.asarray(Image.open(png).convert('RGB')), hashlib.sha256(png.read_bytes()).hexdigest())
    results = {}
    for chroma, depth in FORMATS:
        key = f'{chroma}-{depth}'
        for image, (rgb, _) in images.items():
            h, w = rgb.shape[:2]
            src = planes(rgb, chroma, depth)
            raw = work / 'input.yuv'
            raw.write_bytes(b''.join(p.tobytes() for p in src))
            top = (1 << depth) - 1
            for name in libs:
                points = []
                for q in QUALITIES:
                    heif = work / f'{name}.heif'
                    subprocess.run([str(work / name), str(w), str(h), str(q), str(chroma), str(depth), str(raw),
                                    str(heif), ids[name]], check=True, capture_output=True)
                    stream = work / f'{name}.264'
                    stream.write_bytes(annexb(heif.read_bytes()))
                    run = subprocess.run([a.ffmpeg, '-v', 'error', '-i', str(stream), '-f', 'rawvideo', '-'],
                                         capture_output=True, check=True)
                    got = np.frombuffer(run.stdout[:src[0].nbytes], src[0].dtype).reshape(h, w).astype(float)
                    mse = ((got - src[0].astype(float)) ** 2).mean()
                    psnr = 10 * math.log10(top ** 2 / mse) if mse else 100.0
                    points.append((q, heif.stat().st_size, psnr))
                results.setdefault(key, {}).setdefault(image, {})[name] = points
    rates = {}
    for key, per in results.items():
        rates[key] = {}
        for image, r in per.items():
            x = [t for t in r['x264'] if 28 < t[2] < 50]
            c = [t for t in r['candidate'] if 28 < t[2] < 50]
            rates[key][image] = bd_rate([t[1] for t in x], [t[2] for t in x], [t[1] for t in c], [t[2] for t in c])
    report = dict(scope=__doc__, qualities=QUALITIES, images={k: v[1] for k, v in images.items()},
                  bd_rate_percent=rates, mean_bd_rate_percent={k: float(np.mean(list(v.values()))) for k, v in rates.items()},
                  points=results)
    Path(a.output).write_text(json.dumps(report, indent=1) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k not in ('points', 'scope')}, indent=1))


if __name__ == '__main__':
    main()
