#!/usr/bin/env python3
"""Decodes libheifer's High-profile AVC encoder output (examples/avc_high_roundtrip.rs:
4:0:0 to 4:4:4, 8 and 10 bits, lossy and lossless, deblocking off) with the test-only
JM reference decoder and FFmpeg's H.264 decoder, and requires both to reproduce the
encoder's reconstruction (the input, for lossless streams) exactly. Streams with the
deblocking filter on must decode identically in both decoders.
"""
import argparse
import hashlib
import json
import os
import subprocess
from pathlib import Path


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--jm', default='.build/jm-source/bin/ldecod_static')
    p.add_argument('--ffmpeg', default='.build/ffmpeg-install/bin/ffmpeg')
    p.add_argument('--work', default='.build/avc-high-roundtrip')
    p.add_argument('--output', default='.build/avc-high-roundtrip-report.json')
    p.add_argument('--skip-build', action='store_true')
    a = p.parse_args()
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    if not a.skip_build:
        subprocess.run(['cargo', 'build', '--release', '--features', 'avc', '--example', 'avc_high_roundtrip'], check=True)
    subprocess.run(['target/release/examples/avc_high_roundtrip', str(work)], check=True)
    jm, ffmpeg = Path(a.jm).resolve(), Path(a.ffmpeg).resolve()
    results = []
    for line in (work / 'manifest.txt').read_text().split('\n'):
        if not line:
            continue
        name, chroma, depth, w, h, deblock = line.split()
        chroma, depth, w, h, deblock = int(chroma), int(depth), int(w), int(h), deblock == '1'
        bps = 2 if depth > 8 else 1
        sx, sy = {0: (1, 1), 1: (2, 2), 2: (2, 1), 3: (1, 1)}[chroma]
        luma = w * h * bps
        size = luma + (0 if chroma == 0 else 2 * ((w + sx - 1) // sx) * ((h + sy - 1) // sy) * bps)
        rec = (work / f'{name}.rec').read_bytes()
        stream = work / f'{name}.264'
        record = dict(name=name, stream_sha256=hashlib.sha256(stream.read_bytes()).hexdigest())
        out = work / f'{name}.jm.yuv'
        out.unlink(missing_ok=True)
        run = subprocess.run([str(jm), '-i', str(stream), '-o', str(out)], capture_output=True, cwd=work, timeout=120)
        jm_out = out.read_bytes()[:size] if out.exists() and run.returncode == 0 else b''
        out.unlink(missing_ok=True)
        run = subprocess.run([str(ffmpeg), '-v', 'error', '-i', str(stream), '-f', 'rawvideo', '-y', '-'],
                             capture_output=True, timeout=120)
        ff_out = run.stdout[:size] if run.returncode == 0 else b''
        # JM writes 4:0:0 as 4:2:0 with neutral chroma: only the luma prefix counts.
        expected = jm_out if deblock else rec
        record['jm'] = len(jm_out) == size and jm_out == (ff_out if deblock else rec)
        record['ffmpeg'] = len(ff_out) == size and ff_out == expected
        if not (record['jm'] and record['ffmpeg']):
            record['ffmpeg_error'] = run.stderr.decode(errors='replace')[-300:]
        results.append(record)
        print(name, 'jm', record['jm'], 'ffmpeg', record['ffmpeg'], flush=True)
    report = dict(scope=__doc__, cases=len(results),
                  jm_mismatches=sum(not r['jm'] for r in results),
                  ffmpeg_mismatches=sum(not r['ffmpeg'] for r in results),
                  jm_sha256=hashlib.sha256(jm.read_bytes()).hexdigest(),
                  ffmpeg_sha256=hashlib.sha256(ffmpeg.read_bytes()).hexdigest(), results=results)
    Path(a.output).write_text(json.dumps(report, indent=1) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k not in ('results', 'scope')}))
    if report['jm_mismatches'] or report['ffmpeg_mismatches']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
