#!/usr/bin/env python3
"""Original-header image encoding, retained handles, direct decoding and exact serialized image bytes."""
import argparse,hashlib,json,os,struct,subprocess,random
from pathlib import Path

def corpus():
    cases=[]
    def add(v):cases.append(('-'.join(map(str,v)),struct.pack('=8I',*(x&0xffffffff for x in v))))
    for width,height in [(1,1),(2,3),(3,2),(5,7),(16,9)]:
        for orientation in range(10):
            for version in [0,1,4,5,8,9,264]:add([9,width,height,2,0,8,orientation,version])
    for depth in [0,1,7,8,9,16]:
        for space,chroma in [(2,0),(0,3),(1,3)]:add([9,3,2,space,chroma,depth,1,8])
    for width,height in [(1,1),(2,3),(5,7),(16,9)]:
        for depth in [1,7,8,9,10,12,16]:
            for space,chroma in [(2,0),(0,1),(0,2),(0,3),(1,3)]:
                for flags in [8,520,8|(3<<16),8|(4<<16),8|(5<<16)]:add([8,width,height,space,chroma,depth,1,flags])
        for chroma in [10,11,12,13,14,15]:
            for depth in ([8] if chroma<=11 else [10,12,14,16]):add([8,width,height,1,chroma,depth,6,8])
    for space,chroma in [(2,0),(0,1),(1,3),(1,10)]:add([8,3,2,space,chroma,0,1,8])
    for format in [8,9]:
        for flags in [1024,2048,3072,3072|16384,3072|32768,1024|256,4096,8192,12288,15360]:
            for orientation in [1,5,6,8]:add([format,3,2,2,0,8,orientation,8|flags])
    for format in [8,9]:
        for mode in [256,1024]:
            for orientation in [1,5,6,8]:add([format|mode,3,2,2,0,8,orientation,8])
        for bbox in [1,2,3,4,6,10,12]:
            for width,height in [(8,6),(6,8),(9,3)]:add([format|512,width,height,2,0,8,bbox,8])
    for format in [8,9]:
        for mode in [4096,8192]:add([format|mode,3,2,2,0,8,1,8])
    for format in [0,1,4,7,10,99]:add([format,3,2,2,0,8,1,8])
    return cases,b''.join(data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/encoding-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/encoding-sanitized' if a.sanitize else '.build/encoding')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/encoding.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary),str(work/(name+'-output.heif'))],input=payload,capture_output=True,timeout=120,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    # Safety probes for two isolated reference defects. Never count these as matches.
    failures=[]
    probe=Path('tests/encoding_oracle_failures.c')
    for name,lib in libs.items():
        binary=work/(name+'-failures')
        subprocess.run(['cc','-std=c11','-O1','-g','-fsanitize=address,undefined','-fno-omit-frame-pointer','-Itests/upstream/libheif/api',f'-I{inc.parent}',str(probe),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        for mode in [0,1]:
            run=subprocess.run([str(binary),str(mode)],capture_output=True,timeout=30,env=dict(env,ASAN_OPTIONS='detect_leaks=0'))
            stderr=run.stderr.decode(errors='replace')
            (work/f'{name}-failure-{mode}.stderr').write_text(stderr)
            failures.append(dict(library=name,case='encoded_uncompressed_alpha' if mode==0 else 'encoded_timestamp_ownership',exit=run.returncode,diagnostic=stderr))
            if name=='candidate' and run.returncode:raise SystemExit(f'Candidate safety probe failed: {stderr}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,oracle_limitations=failures,safety_probe_sha256=hashlib.sha256(probe.read_bytes()).hexdigest(),cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k not in ('differences','oracle_limitations')},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
