#!/usr/bin/env python3
"""Interleaved AVC decode timings: libheif + OpenH264 against libheifer, same C client.

Bench streams are encoded with the pinned test-only x264. Every sample checks the
full decoded-plane digest of both libraries; timings are refused on any mismatch.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys

from generate_avc_fixtures import REVISION, picture
from test_avc import item

STREAMS = {
    'high-cabac-qp22': (1920, 1080, 3, ['--profile', 'high', '--qp', '22']),
    'high-cabac-qp32': (1920, 1080, 0, ['--profile', 'high', '--qp', '32']),
    'high-cavlc-qp22': (1920, 1080, 3, ['--profile', 'high', '--no-cabac', '--qp', '22']),
    'baseline-qp27': (1920, 1080, 4, ['--profile', 'baseline', '--qp', '27']),
}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inputs():
    files = [Path('Cargo.toml'), Path('Cargo.lock')]
    for directory in ('src', 'crates', 'vendor'):
        files.extend(Path(directory).rglob('*.rs'))
        files.extend(Path(directory).rglob('Cargo.toml'))
    files += [Path(p) for p in ('tools/bench_avc.py', 'tests/bench_avc.c')]
    return {str(p): sha(p) for p in sorted(set(files))}


def stream(work, name, x264):
    w, h, pattern, options = STREAMS[name]
    heif = work / f'{name}.heif'
    if not heif.exists():
        raw = work / f'{name}.yuv'
        out = work / f'{name}.264'
        raw.write_bytes(picture(w, h, 'i420', 8, pattern))
        subprocess.run([str(x264), '--quiet', '--no-progress', '--threads', '1', '--frames', '1', '--keyint', '1',
                        '--input-res', f'{w}x{h}', '--input-csp', 'i420', *options, '-o', str(out), str(raw)], check=True)
        data = out.read_bytes()
        heif.write_bytes(item(dict(hex=data.hex(), width=w, height=h)))
        raw.unlink()
    return heif


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build', default='.build/reference-avc')
    p.add_argument('--stream', action='append', choices=STREAMS)
    p.add_argument('--samples', type=int, default=7)
    p.add_argument('--iterations', type=int, default=5)
    p.add_argument('--candidate', help='prebuilt libheifer.so (default: build an isolated release)')
    p.add_argument('--output', default='.build/avc-bench.json')
    args = p.parse_args()
    if args.samples < 3 or args.iterations < 1:
        p.error('at least three sample pairs and one measured iteration are required')
    source = Path('tests/upstream').resolve()
    work = Path('.build/avc-bench').resolve()
    work.mkdir(parents=True, exist_ok=True)
    x264_source = Path('.build/x264-source').resolve()
    if subprocess.check_output(['git', '-C', str(x264_source), 'rev-parse', 'HEAD'], text=True).strip() != REVISION:
        raise SystemExit('wrong native x264 revision')
    x264 = Path('.build/x264-install/bin/x264').resolve()
    reference = Path(args.reference_build).resolve() / 'libheif/libheif.so'
    source_hashes = inputs()
    build_command = None
    if args.candidate:
        candidate = Path(args.candidate).resolve()
    else:
        target = Path('.build/benchmark-target').resolve()
        build_command = ['cargo', 'build', '--locked', '--release', '-p', 'libheifer-capi']
        subprocess.run(build_command, env=dict(os.environ, CARGO_TARGET_DIR=str(target)), check=True)
        candidate = target / 'release/libheifer.so'
    include = work / 'include/libheif'
    include.mkdir(parents=True, exist_ok=True)
    (include / 'heif_version.h').write_bytes((reference.parent / 'heif_version.h').read_bytes())
    clients = {}
    for name, library in (('reference', reference), ('candidate', candidate)):
        binary = work / f'bench-{name}'
        subprocess.run(['cc', '-O3', '-std=c11', '-Werror', f'-I{source / "libheif/api"}', f'-I{include.parent}',
                        'tests/bench_avc.c', str(library), f'-Wl,-rpath,{library.parent}', '-o', str(binary)], check=True)
        clients[name] = binary
    hashes = {'reference_sha256': sha(reference), 'candidate_sha256': sha(candidate)}
    results = {}
    for name in args.stream or list(STREAMS):
        heif = stream(work, name, x264)
        samples = []
        for i in range(args.samples):
            pair = {}
            for side in (['reference', 'candidate'] if i % 2 == 0 else ['candidate', 'reference']):
                run = subprocess.run([str(clients[side]), str(heif), str(args.iterations)], check=True, capture_output=True, timeout=600)
                pair[side] = json.loads(run.stdout)
            if pair['candidate']['digest'] != pair['reference']['digest'] or pair['candidate']['checksum'] != pair['reference']['checksum']:
                raise SystemExit(f'{name}: decoded output differs; refusing timings')
            samples.append(pair)
        summary = {}
        for side in ('reference', 'candidate'):
            values = [s[side]['ns'] / s[side]['iterations'] / 1e6 for s in samples]
            summary[side] = {'median_ms': statistics.median(values), 'min_ms': min(values), 'max_ms': max(values), 'stdev_ms': statistics.stdev(values)}
        summary['candidate_over_reference'] = summary['candidate']['median_ms'] / summary['reference']['median_ms']
        results[name] = {'heif_sha256': sha(heif), 'options': STREAMS[name][3], 'size': STREAMS[name][:2], 'summary': summary, 'samples': samples}
        print(f"{name}: reference {summary['reference']['median_ms']:.2f} ms, candidate {summary['candidate']['median_ms']:.2f} ms "
              f"({summary['candidate_over_reference']:.2f}x)", flush=True)
    if source_hashes != inputs() or hashes != {'reference_sha256': sha(reference), 'candidate_sha256': sha(candidate)}:
        raise SystemExit('Source or binaries changed during measurement; results discarded')
    report = {'scope': 'memory parse + primary-item AVC decode to native YUV + teardown, through the same C client; I/O excluded',
              'reference': 'libheif 1.23.4 + OpenH264 2.6.0 (USE_ASM=No, Release)',
              'platform': platform.platform(), 'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
              'cpu': next((l.split(':', 1)[1].strip() for l in Path('/proc/cpuinfo').read_text().splitlines() if l.startswith('model name')), 'unknown'),
              'build_command': build_command, **hashes, 'source_hashes': source_hashes, 'results': results}
    Path(args.output).write_text(json.dumps(report, indent=1) + '\n')


if __name__ == '__main__':
    main()
