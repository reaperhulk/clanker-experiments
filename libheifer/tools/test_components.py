#!/usr/bin/env python3
"""Original-header component descriptions, typed storage, IDs, arbitrary datatypes, content IDs and transforms."""
import argparse,hashlib,json,os,struct,subprocess,random
from pathlib import Path

def corpus():
    cases=[]
    def add(name,family=0,kind=1,datatype=0,depth=8,width=7,height=5,colorspace=2,chroma=0,mode=0):
        values=[family,kind,datatype&0xffffffff,depth&0xffffffff,width&0xffffffff,height&0xffffffff,colorspace,chroma,mode]
        cases.append((name,struct.pack('=9I',*values)))
    for kind in range(65536):add(f'reference-type-{kind}',family=2,kind=kind)
    for datatype in [0,1,2,3,4,255,-1,2147483647]:
        for depth in [*range(1,129),0,129,256,-1,2147483647,-2147483648]:
            add(f'datatype-{datatype}-depth-{depth}',datatype=datatype,depth=depth)
    for cs,ch in [(0,1),(0,2),(0,3),(1,3),(1,10),(1,11),(1,12),(1,13),(1,14),(1,15),(2,0),(3,0),(4,0)]:
        for kind in [*range(17),255,1000,65535]:
            add(f'mixed-{cs}-{ch}-{kind}',family=1,colorspace=cs,chroma=ch,kind=kind,depth=16 if ch>=12 else 8,datatype=kind%4,mode=kind%2)
    for kind in range(17):
        for datatype in [0,1,2,3]:
            for depth in [8,16,32,64,128]:
                add(f'crop-kind-{kind}-{datatype}-{depth}',kind=kind,datatype=datatype,depth=depth)
    for width,height in [(0,0),(0,5),(7,0),(-1,5),(7,-1),(1,1),(63,65),(64,64),(65,63),(129,3)]:
        for depth in [0,1,8,9,16,17,32,33,64,65,128,129]:
            add(f'shape-{width}-{height}-{depth}',width=width,height=height,depth=depth)
    return cases,b''.join(data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/components-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/components-sanitized' if a.sanitize else '.build/components').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/components.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=120,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases)+1 for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    cases=cases+[('null-pointers',b'')];differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
