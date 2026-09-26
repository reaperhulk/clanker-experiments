#!/usr/bin/env python3
"""Decodes libheifer's High-profile AVC encoder output (examples/avc_high_roundtrip.rs:
4:0:0 to 4:4:4, 8 and 10 bits, lossy and lossless, with and without deblocking) with the
test-only JM reference decoder and FFmpeg's H.264 decoder (tools/build_avc_decoders.py).
Without deblocking both must reproduce the encoder's reconstruction exactly; lossless
streams must reproduce the input; with deblocking the decoders must agree.
"""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--jm', default='.build/jm-install/bin/ldecod')
    p.add_argument('--ffmpeg', default='.build/ffmpeg-install/bin/ffmpeg')
    p.add_argument('--work', default='.build/avc-high-roundtrip')
    p.add_argument('--output', default='.build/avc-high-roundtrip-report.json')
    # The example next to the candidate library is used (the mutation harness builds it
    # in its tree); for the default candidate it is built here.
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--reference-build', help='unused (mutation harness interface)')
    a = p.parse_args()
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    example = Path(a.candidate).resolve().parent / 'examples/avc_high_roundtrip'
    if Path(a.candidate).resolve() == Path('target/release/libheifer.so').resolve():
        subprocess.run(['cargo', 'build', '--locked', '--release', '--features', 'avc', '--example', 'avc_high_roundtrip'],
                       check=True)
    for old in work.glob('*'):
        old.unlink()
    generated = subprocess.run([str(example), str(work)], capture_output=True)
    (work / 'manifest.txt').touch()
    jm, ffmpeg = Path(a.jm).resolve(), Path(a.ffmpeg).resolve()
    results = []
    for line in (work / 'manifest.txt').read_text().split('\n'):
        if not line:
            continue
        name, chroma, depth, w, h, deblock, lossless = line.split()
        chroma, depth, w, h = int(chroma), int(depth), int(w), int(h)
        deblock, lossless = deblock == '1', lossless == '1'
        bps = 2 if depth > 8 else 1
        sx, sy = {0: (1, 1), 1: (2, 2), 2: (2, 1), 3: (1, 1)}[chroma]
        luma = w * h * bps
        size = luma + (0 if chroma == 0 else 2 * ((w + sx - 1) // sx) * ((h + sy - 1) // sy) * bps)
        rec = (work / f'{name}.rec').read_bytes()
        src = (work / f'{name}.src').read_bytes()
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
        expected = src if lossless else jm_out if deblock else rec
        record['jm'] = len(jm_out) == size and jm_out == (ff_out if deblock and not lossless else expected)
        record['ffmpeg'] = len(ff_out) == size and ff_out == expected
        if lossless and rec != src:
            record['jm'] = record['ffmpeg'] = False
        if not (record['jm'] and record['ffmpeg']):
            record['ffmpeg_error'] = run.stderr.decode(errors='replace')[-300:]
        results.append(record)
        print(name, 'jm', record['jm'], 'ffmpeg', record['ffmpeg'], flush=True)
    mismatches = sum(not (r['jm'] and r['ffmpeg']) for r in results) + int(generated.returncode != 0)
    report = dict(scope=__doc__, cases=len(results), mismatches=mismatches,
                  generator_failure=generated.stderr.decode(errors='replace')[-500:] if generated.returncode else None,
                  jm_mismatches=sum(not r['jm'] for r in results),
                  ffmpeg_mismatches=sum(not r['ffmpeg'] for r in results),
                  jm_sha256=hashlib.sha256(jm.read_bytes()).hexdigest(),
                  ffmpeg_sha256=hashlib.sha256(ffmpeg.read_bytes()).hexdigest(), results=results)
    Path(a.output).write_text(json.dumps(report, indent=1) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k not in ('results', 'scope')}))
    if report['mismatches']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
