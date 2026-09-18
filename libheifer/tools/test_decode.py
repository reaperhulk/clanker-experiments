#!/usr/bin/env python3
"""Exact original-header decode comparisons, including metadata, alpha and callbacks."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from test_hevc import FIXTURES


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build', required=True)
    p.add_argument('--source', default='tests/upstream')
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--output', default='.build/decode-report.json')
    p.add_argument('--modes', default=','.join(map(str, range(23))))
    args = p.parse_args()
    source = Path(args.source).resolve()
    reference = Path(args.reference_build).resolve()
    work = Path('.build/decode').resolve()
    include = work / 'include/libheif'
    include.mkdir(parents=True, exist_ok=True)
    (include/'heif_version.h').write_bytes((reference/'libheif/heif_version.h').read_bytes())
    libraries = {'reference': reference/'libheif/libheif.so', 'candidate': Path(args.candidate).resolve()}
    for name, library in libraries.items():
        subprocess.run(['cc','-std=c11','-O2','-Werror',f'-I{source/"libheif/api"}',f'-I{include.parent}',
                        'tests/decode.c',str(library),f'-Wl,-rpath,{library.parent}','-o',str(work/name)],check=True)
    results=[]
    for fixture in FIXTURES:
        for mode in map(int,args.modes.split(',')):
            data={}
            for name in libraries:
                out=work/f'{Path(fixture).stem}-{mode}-{name}.bin'
                out.unlink(missing_ok=True)
                run=subprocess.run([str(work/name),str(source/fixture),str(out),str(mode)],capture_output=True,timeout=120)
                if run.returncode:raise SystemExit(f'{name}: {fixture} mode {mode}: {run.returncode} {run.stderr.decode(errors="replace")}')
                data[name]=out.read_bytes()
            match=data['reference']==data['candidate']
            record={'fixture':fixture,'mode':mode,'match':match, **{name+'_sha256':hashlib.sha256(value).hexdigest() for name,value in data.items()}}
            if not match:
                at=next((i for i,(a,b) in enumerate(zip(data['reference'],data['candidate'])) if a!=b),min(map(len,data.values())))
                record.update(offset=at,reference=data['reference'][max(0,at-16):at+80].hex(),candidate=data['candidate'][max(0,at-16):at+80].hex())
            results.append(record)
            print(f'{fixture} mode={mode}: {"match" if match else "DIFFERENT"}',flush=True)
    report={'scope':__doc__,'cases':len(results),'mismatches':sum(not r['match'] for r in results),
            'client_sha256':hashlib.sha256(Path('tests/decode.c').read_bytes()).hexdigest(),
            **{name+'_sha256':hashlib.sha256(path.read_bytes()).hexdigest() for name,path in libraries.items()},'results':results}
    Path(args.output).write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k!='results'},indent=2))
    if report['mismatches']:raise SystemExit(1)


if __name__=='__main__':main()
