#!/usr/bin/env python3
"""Original-header tile geometry, ordered transformations, displayed coordinates, security limits and output sentinels."""
import argparse,hashlib,json,os,struct,subprocess,random
from pathlib import Path

def corpus():
    from item_fixtures import item_file,ispe
    from test_context import box,full
    from test_uncompressed_pixels import config,cmpd
    cases=[]
    def add(name,data,x,y,process,limit=0): cases.append((name,struct.pack('=5I',len(data),process,x,y,limit)+data))
    transformations=[[],[box(b'irot',bytes([r])) for r in [1,3]]]
    for r in range(4):
        for m in [None,0,1]:transformations.append([box(b'irot',bytes([r]))]+([] if m is None else [box(b'imir',bytes([m]))]))
    for a,b in [(1,1),(1,3),(3,1),(1,2),(2,3)]:transformations.append([box(b'irot',bytes([a])),box(b'imir',b'\1'),box(b'irot',bytes([b]))])
    for crop in [(3,1,2,1,0,1,0,1),(0,1,0,1,0,1,0,1),(3,0,2,1,0,1,0,1)]:
        for r in range(4):transformations.append([full(b'clap',b'') if False else box(b'clap',struct.pack('>8i',*crop)),box(b'irot',bytes([r]))])
    for kind in [b'mski',b'unci',b'grid']:
        for rows,cols in [(1,1),(2,3),(3,2)]:
            for index,props in enumerate(transformations):
                w,h=4*cols,3*rows
                if kind==b'mski': items=[dict(id=1,kind=kind,data=bytes(range(w*h)),props=[ispe(w,h),full(b'mskC',b'\10')]+props)]
                elif kind==b'unci':items=[dict(id=1,kind=kind,data=bytes(range(w*h)),props=[ispe(w,h),cmpd([0]),config(cols=cols-1,rows=rows-1)]+props)]
                else:
                    ids=list(range(10,10+rows*cols));w-=1;h-=1
                    items=[dict(id=1,kind=kind,data=bytes([0,0,rows-1,cols-1])+struct.pack('>HH',w,h),props=[ispe(w,h)]+props,refs={b'dimg':ids})]
                    items += [dict(id=i,kind=b'mski',data=bytes(range(12)),props=[ispe(4,3),full(b'mskC',b'\10')]) for i in ids]
                data=item_file(items)
                for process in [0,1]:
                    for x,y in [(0,0),(1,0),(0,1),(2,1),(1,2),(3,3),(0xffffffff,0)]:add(f'{kind}-{rows}-{cols}-{index}-{process}-{x}-{y}',data,x,y,process)
                for limit in [1,11,12,24]:add(f'{kind}-{rows}-{cols}-{index}-limit{limit}',data,0,0,1,limit)
    from test_uncompressed_units import corpus as unit_corpus
    for name,framed,_ in unit_corpus():
        for x,y in [(0,0),(1,0),(1,1)]:add(f'compressed-{name}-{x}-{y}',framed[8:],x,y,1)
    cases.append(('null-handles',b''))
    return cases,b''.join(data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/tiling-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/tiling-sanitized' if a.sanitize else '.build/tiling')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/tiling.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary),str(work/(name+'-output.heif'))],input=payload,capture_output=True,timeout=300,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k not in ('differences','oracle_limitations')},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
