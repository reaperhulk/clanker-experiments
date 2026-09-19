#!/usr/bin/env python3
"""Original-header defaults, all version pairs, old prefixes and alias copies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build', required=True)
    p.add_argument('--source', default='tests/upstream')
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--output', default='.build/decoding-options-report.json')
    p.add_argument('--sanitize', action='store_true')
    p.add_argument('--no-leak-check', action='store_true')
    args = p.parse_args()
    report_path = Path(args.output)
    report_path.unlink(missing_ok=True)
    work = Path('.build/decoding-options-sanitized' if args.sanitize else '.build/decoding-options').resolve()
    include = work / 'include/libheif'
    include.mkdir(parents=True, exist_ok=True)
    reference = Path(args.reference_build).resolve()
    source = Path(args.source).resolve()
    (include / 'heif_version.h').write_bytes((reference / 'libheif/heif_version.h').read_bytes())
    libraries = {'reference': reference / 'libheif/libheif.so', 'candidate': Path(args.candidate).resolve()}
    flags = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer'] if args.sanitize else []
    env = dict(os.environ, UBSAN_OPTIONS='halt_on_error=1')
    if args.no_leak_check:
        env['ASAN_OPTIONS'] = 'detect_leaks=0'
    transcripts = {}
    for name, library in libraries.items():
        binary = work / name
        subprocess.run(['cc', '-std=c11', '-O2', '-Werror', *flags,
                        f'-I{source / "libheif/api"}', f'-I{include.parent}',
                        'tests/decoding_options.c', str(library), f'-Wl,-rpath,{library.parent}',
                        '-o', str(binary)], check=True)
        run = subprocess.run([str(binary)], capture_output=True, timeout=120, env=env)
        (work / f'{name}.stderr').write_bytes(run.stderr)
        (work / f'{name}.txt').write_bytes(run.stdout)
        if run.returncode:
            raise SystemExit(f'{name} failed: {run.returncode}: {run.stderr.decode(errors="replace")[:4000]}')
        transcripts[name] = run.stdout.splitlines()
        if len(transcripts[name]) != 65549:
            raise SystemExit(f'Incomplete {name} transcript')
    differences = [{'case': n, 'reference': a.decode(), 'candidate': b.decode()}
                   for n, (a, b) in enumerate(zip(transcripts['reference'], transcripts['candidate'], strict=True)) if a != b]
    digest = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
    report = {'scope': __doc__, 'cases': len(transcripts['reference']), 'mismatches': len(differences),
              'client_sanitizers': args.sanitize, 'leak_check': args.sanitize and not args.no_leak_check,
              'reference_sha256': digest(libraries['reference']), 'candidate_sha256': digest(libraries['candidate']),
              'client_sha256': digest('tests/decoding_options.c'), 'examples': differences[:10]}
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    if differences:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
