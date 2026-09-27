#!/usr/bin/env python3
"""Original-header OMAF projections, 5-bit properties, retained descriptions, decoded pixels and transforms."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
from pathlib import Path

def corpus():
    from item_fixtures import item_file,ispe
    from test_context import box,full
    cases=[]
    def fixture(props,kind=b'mski',child_props=()):
        image=dict(id=1,kind=kind,data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',bytes([8])),*props])
        items=[image]
        if kind in [b'iden',b'grid',b'iovl']:
            from test_decode_overlay import overlay
            image['data']=bytes(4)+struct.pack('>HH',8,8) if kind==b'grid' else overlay(8,8) if kind==b'iovl' else b'';image['refs']={b'dimg':[2]}
            items.append(dict(id=2,kind=b'mski',data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',bytes([8])),*child_props]))
        return item_file(items)
    def add(name,props=(),seed=0x12345678,flags=0,reload=None,kind=b'mski',child_props=()):
        file=fixture(props,kind,child_props);reload=fixture([]) if reload is None else reload
        cases.append((name,struct.pack('=4I',len(file),len(reload),flags,seed)+file+reload))
    for seed in list(range(256))+[0x7fffffff,0x80000000,0xffffffff]:
        for flags in range(4):add(f'set-{flags}-{seed}',seed=seed,flags=flags)
    for value in range(256):add(f'file-value-{value}',props=[full(b'prfr',bytes([value]))],seed=value)
    for version in range(256):add(f'file-version-{version}',props=[full(b'prfr',bytes([17]),version)])
    for n in range(6):add(f'truncated-{n}',props=[box(b'prfr',bytes(n))])
    for value in [0,1,17,31,32,255]:
        for tail in [b'',bytes(4),b'trailing']:
            for flag in [0,1,0x7fffff,0xffffff]:
                data=bytes([0])+flag.to_bytes(3,'big')+bytes([value])+tail
                add(f'flags-tail-{value}-{flag}-{len(tail)}',props=[box(b'prfr',data)],seed=value)
        for value2 in [0,1,31,255]:add(f'duplicate-{value}-{value2}',props=[full(b'prfr',bytes([value])),full(b'prfr',bytes([value2]))],seed=value2)
    for kind in [b'iden',b'grid',b'iovl']:
        for parent in [[],[full(b'prfr',bytes([1]))]]:
            for child in [[],[full(b'prfr',bytes([17]))]]:
                add(f'derived-{kind}-{len(parent)}-{len(child)}',props=parent,kind=kind,child_props=child)
    for first in [[],[full(b'prfr',bytes([1]))]]:
        for second in [[],[full(b'prfr',bytes([17]))]]:
            for flags in range(4):add(f'reload-{len(first)}-{len(second)}-{flags}',props=first,reload=fixture(second),flags=flags,seed=31)
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/omaf-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/omaf-sanitized' if a.sanitize else '.build/omaf')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/omaf.c');client_hash=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    decoded_counts={k:sum(line.count(b'decoded:') for line in v) for k,v in lines.items()}
    if decoded_counts['reference']!=len(cases)*10:raise SystemExit(f'Unexpected successful reference decodes: {decoded_counts}')
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,successful_decodes=decoded_counts,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
