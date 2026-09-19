#!/usr/bin/env python3
"""Original-header grid and uncompressed tile construction, replacement, orientations, exact file bytes and decoded pixels."""
import argparse,hashlib,json,os,struct,subprocess,random
from pathlib import Path

def corpus():
    cases=[]
    def add(v):cases.append(('-'.join(map(str,v)),struct.pack('=10I',*v)))
    for kind in [0,1,2]:
        for format in ([8,9] if kind<2 else [8]):
            for cols,rows in [(1,1),(2,3),(3,2)]:
                for rotation in range(1,9):
                    for flags in [0,1,2,8,16,8192]:add([format,kind,cols,rows,4,3,rotation,0,flags,8])
    for kind in [0,1,2]:
        for compression in [3,4,5]:
            for flags in [0,1,2,4,64,256|64,512,1024,16384]:add([8,kind,2,2,4,3,1,compression,flags,8])
    for kind in [0,1,2]:
        for flags in [4,8|16384,16384,512,1024,64,64|16,64|256,32]:add([8,kind,2,3,4,3,6,0,flags,8])
    for kind in [0,1,2]:
        for flags in [2048,65536,65536|1,2048|1]:add([8,kind,2,3,4,3,6,0,flags,8])
        for cols,rows in [(1,1),(2,3)]:
            for tw,th in [(1,1),(3,5),(7,2)]:
                for depth in [1,7,9,10,12,16]:add([8,kind,cols,rows,tw,th,6,0,0,depth])
    for flags in [128,129,128|8192,8|16384]:
        for compression in [0,3,4]:add([8,2,2,2,4,3,6,compression,flags,8])
    for flags in [4096,4096|1]:add([8,2,2,2,4,3,1,0,flags,8])
    for cols,rows in [(0,1),(1,0),(65536,1),(65535,2)]:add([8,0,cols,rows,4,3,1,0,0,8])
    return cases,b''.join(data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/tile_encoding-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/tile_encoding-sanitized' if a.sanitize else '.build/tile_encoding')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/tile_encoding.c');client_bytes=lambda:client.read_bytes()+Path('tests/encoding.c').read_bytes();client_hash=hashlib.sha256(client_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary),str(work/(name+'-output.heif'))],input=payload,capture_output=True,timeout=120,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k not in ('differences','oracle_limitations')},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
