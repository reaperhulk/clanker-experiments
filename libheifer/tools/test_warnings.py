#!/usr/bin/env python3
"""Independent warning ownership, diagnostic text, pagination and thread controls."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build',required=True)
    p.add_argument('--candidate',default='target/release/libheifer.so')
    p.add_argument('--source',default='tests/upstream')
    p.add_argument('--output',default='.build/warnings-report.json')
    p.add_argument('--sanitize',action='store_true')
    p.add_argument('--no-leak-check',action='store_true',help='Disable LeakSanitizer when ptrace prevents it from running')
    args=p.parse_args()
    if args.no_leak_check and not args.sanitize:p.error('--no-leak-check requires --sanitize')
    Path(args.output).unlink(missing_ok=True)
    build=Path('.build/warnings'+('-sanitized' if args.sanitize else '')).resolve()
    include=build/'include/libheif';include.mkdir(parents=True,exist_ok=True)
    reference=Path(args.reference_build).resolve();source=Path(args.source).resolve()
    (include/'heif_version.h').write_bytes((reference/'libheif/heif_version.h').read_bytes())
    enums=json.loads(Path('compat/api.json').read_text())['enums']
    codes=sorted(set(enums['heif_error_code']['values'].values()))
    subcodes=sorted(set(enums['heif_suberror_code']['values'].values()))
    (include.parent/'warning_values.h').write_text('static const int codes[]={'+','.join(map(str,codes))+'};\nstatic const int subcodes[]={'+','.join(map(str,subcodes))+'};\n')
    libs={'reference':reference/'libheif/libheif.so','candidate':Path(args.candidate).resolve()}
    transcripts={}
    for name,lib in libs.items():
        binary=build/name
        subprocess.run(['cc','-std=c11','-Werror','-O1' if args.sanitize else '-O2',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer','-no-pie'] if args.sanitize else []),f'-I{source/"libheif/api"}',f'-I{include.parent}','tests/warnings.c',str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],capture_output=True,timeout=120,env={**os.environ,'ASAN_OPTIONS':'detect_leaks=0' if args.no_leak_check else 'detect_leaks=1','UBSAN_OPTIONS':'halt_on_error=1'})
        (build/(name+'.txt')).write_bytes(run.stdout)
        (build/(name+'.stderr')).write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")}')
        transcripts[name]=run.stdout.splitlines()
    expected=len(codes)*len(subcodes)+130+2+11
    if len(transcripts['reference'])!=expected:raise SystemExit('Incomplete reference transcript')
    mismatches=[{'case':i,'reference':a.decode(),'candidate':b.decode()} for i,(a,b) in enumerate(zip(transcripts['reference'],transcripts['candidate'],strict=True)) if a!=b]
    report={'scope':__doc__,'cases':expected,'mismatches':len(mismatches),'sanitized_client':args.sanitize,'leak_check':args.sanitize and not args.no_leak_check,'client_sha256':hashlib.sha256(Path('tests/warnings.c').read_bytes()).hexdigest(),**{k+'_sha256':hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()},'examples':mismatches[:10]}
    Path(args.output).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
    if mismatches:raise SystemExit(1)


if __name__=='__main__':main()
