#!/usr/bin/env python3
"""Exact original-header decode comparisons, including metadata, alpha and callbacks."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import subprocess
from test_hevc import FIXTURES


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build', required=True)
    p.add_argument('--source', default='tests/upstream')
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--work', default='.build/decode')
    p.add_argument('--output', default='.build/decode-report.json')
    p.add_argument('--modes', default=','.join(map(str, range(23))))
    p.add_argument('--sanitize', action='store_true')
    p.add_argument('--no-leak-check', action='store_true')
    args = p.parse_args()
    if args.no_leak_check and not args.sanitize:
        p.error('--no-leak-check requires --sanitize')
    Path(args.output).unlink(missing_ok=True)
    source = Path(args.source).resolve()
    reference = Path(args.reference_build).resolve()
    work = Path(args.work).resolve()
    include = work / 'include/libheif'
    include.mkdir(parents=True, exist_ok=True)
    (include/'heif_version.h').write_bytes((reference/'libheif/heif_version.h').read_bytes())
    libraries = {'reference': reference/'libheif/libheif.so', 'candidate': Path(args.candidate).resolve()}
    binary_hashes={name:hashlib.sha256(path.read_bytes()).hexdigest() for name,path in libraries.items()}
    client_hash=hashlib.sha256(Path('tests/decode.c').read_bytes()).hexdigest()
    sanitizer_flags = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer'] if args.sanitize else []
    env = dict(os.environ)
    if args.sanitize:
        env['UBSAN_OPTIONS'] = env.get('UBSAN_OPTIONS', '') + ':halt_on_error=1'
    if args.no_leak_check:
        env['ASAN_OPTIONS'] = env.get('ASAN_OPTIONS', '') + ':detect_leaks=0'
    for name, library in libraries.items():
        subprocess.run(['cc','-std=c11','-O2','-Werror',*sanitizer_flags,f'-I{source/"libheif/api"}',f'-I{include.parent}',
                        'tests/decode.c',str(library),f'-Wl,-rpath,{library.parent}','-o',str(work/name)],check=True)
    def case(fixture, mode):
        data={}
        for name in libraries:
            out=work/f'{Path(fixture).stem}-{mode}-{name}.bin'
            out.unlink(missing_ok=True)
            run=subprocess.run([str(work/name),str(source/fixture),str(out),str(mode)],capture_output=True,timeout=120,env=env)
            if run.returncode:raise SystemExit(f'{name}: {fixture} mode {mode}: {run.returncode} {run.stderr.decode(errors="replace")}')
            data[name]=out.read_bytes()
        match=data['reference']==data['candidate']
        record={'fixture':fixture,'fixture_sha256':hashlib.sha256((source/fixture).read_bytes()).hexdigest(),'mode':mode,'match':match, **{name+'_sha256':hashlib.sha256(value).hexdigest() for name,value in data.items()}}
        if not match:
            at=next((i for i,(a,b) in enumerate(zip(data['reference'],data['candidate'])) if a!=b),min(map(len,data.values())))
            record.update(offset=at,reference=data['reference'][max(0,at-16):at+80].hex(),candidate=data['candidate'][max(0,at-16):at+80].hex())
        return record
    # Cases are independent when output names are distinct; results keep corpus
    # order. Later suites read these outputs, so colliding stems run serially.
    cases=[(fixture,mode) for fixture in FIXTURES for mode in map(int,args.modes.split(','))]
    distinct=len({Path(f).stem for f in FIXTURES})==len(FIXTURES)
    results=[]
    with concurrent.futures.ThreadPoolExecutor(max_workers=(os.cpu_count() or 1) if distinct else 1) as pool:
        for (fixture,mode),record in zip(cases,pool.map(lambda c: case(*c),cases)):
            results.append(record)
            print(f'{fixture} mode={mode}: {"match" if record["match"] else "DIFFERENT"}',flush=True)
    if binary_hashes != {name:hashlib.sha256(path.read_bytes()).hexdigest() for name,path in libraries.items()} or client_hash != hashlib.sha256(Path('tests/decode.c').read_bytes()).hexdigest():
        raise SystemExit('Decode binaries or client changed during comparison; no report accepted')
    report={'scope':__doc__,'cases':len(results),'mismatches':sum(not r['match'] for r in results),
            'client_sanitizers':args.sanitize,'leak_check':args.sanitize and not args.no_leak_check,
            'client_sha256':client_hash,
            **{name+'_sha256':value for name,value in binary_hashes.items()},'results':results}
    Path(args.output).write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k!='results'},indent=2))
    if report['mismatches']:raise SystemExit(1)


if __name__=='__main__':main()
