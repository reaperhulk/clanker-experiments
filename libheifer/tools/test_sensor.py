#!/usr/bin/env python3
"""Original-header imaging metadata, NaN bits, component IDs, copies and transform propagation."""
import argparse,hashlib,json,os,struct,subprocess,random
from pathlib import Path

def corpus():
    cases=[]
    def add(name,kind=2,flags=0,width=2,height=3,count=3,applied=0,bits=0x3f800000,chroma=0,colorspace=2):
        values=[kind,flags,width,height,count,applied&0xffffffff,bits,chroma,colorspace]
        cases.append((name,struct.pack('=9I',*values)))
    special=[0,0x80000000,1,0x80000001,0x7f800000,0xff800000,0x7f800001,0xff800001,0x7fc00000,0xffffffff,0x7fffffff,0xfffffffe,0x3f800000,0x43b40000]
    rng=random.Random(0x51e1507)
    for value in special+[rng.getrandbits(32) for _ in range(64)]:add(f'no-filter-{value:08x}',kind=0,bits=value)
    for value in range(256):add(f'chroma-{value}',kind=1,bits=value)
    formats=[(2,0),(0,1),(1,10),(1,11)]
    for cs,ch in formats:
        for bits in special:
            for applied in [0,-1,1,2147483647]:add(f'bits-{cs}-{ch}-{bits:08x}-{applied}',colorspace=cs,chroma=ch,bits=bits,applied=applied)
    for flags in range(64):
        for count in [0,3]:
            for width,height in [(0,1),(1,0),(2,3)]:add(f'null-{flags}-{count}-{width}-{height}',flags=flags,count=count,width=width,height=height,applied=flags)
    for cs,ch in formats:
        for width,height in [(0,0),(0,1),(1,0),(1,1),(2,3),(4,4)]:
            for count in [0,1,3,16]:add(f'shape-{cs}-{ch}-{width}-{height}-{count}',colorspace=cs,chroma=ch,width=width,height=height,count=count)
    return cases,b''.join(data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/sensor-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/sensor-sanitized' if a.sanitize else '.build/sensor').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/sensor.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
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
