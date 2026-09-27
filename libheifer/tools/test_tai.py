#!/usr/bin/env python3
"""Original-header TAI timestamps, clock properties, versioned copies, ownership and decoded propagation."""
import argparse,hashlib,json,os,struct,subprocess,random
from pathlib import Path

def corpus(without_hevc=False):
    from item_fixtures import item_file,ispe
    from test_context import box,full,synthetic
    cases=[]
    def add(name,action=0,source=1,dest=1,value=0x123456789abcdef0,resolution=0xffffffff,drift=0x80000000,kind=255,flags=0x81feff,file=b'',mode=0):
        fields=[action,source,dest,value&0xffffffff,value>>32,resolution,drift&0xffffffff,kind,flags,len(file),mode,0]
        cases.append((name,struct.pack('=12I',*fields)+file))
    for source in range(256):
        for dest in range(256):add(f'copy-{source}-{dest}',source=source,dest=dest)
    for version in [0,1,2,255]:
        for flag in range(256):add(f'owned-{version}-{flag}',action=1,source=version,kind=flag,flags=flag|((255-flag)<<8)|(flag<<16))
        for value in [0,1,0x7fffffffffffffff,0x8000000000000000,0xffffffffffffffff]:
            add(f'boundary-{version}-{value}',action=1,source=version,value=value,resolution=value&0xffffffff,drift=value&0xffffffff)
    clock=struct.pack('>QIIB',0x123456789abcdef0,0x87654321,0xffffffff,0xc3)
    stamp=struct.pack('>QB',0x987654321abcdef0,0xe3)
    def fixture(props):return item_file([dict(id=1,kind=b'mski',data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',b'\10'),*props])])
    add('file-empty-properties',action=2,file=fixture([]))
    for kind,data in [(b'taic',clock),(b'itai',stamp)]:
        for version in range(256):add(f'box-version-{kind}-{version}',action=2,file=fixture([full(kind,data,version)]))
        for n in range(len(data)+5):add(f'box-prefix-{kind}-{n}',action=2,file=fixture([box(kind,(bytes(4)+data)[:n])]))
        for flags in range(256):add(f'box-flags-{kind}-{flags}',action=2,file=fixture([full(kind,data[:-1]+bytes([flags]))]))
    props=[full(b'taic',clock),full(b'itai',stamp)]
    for extra in [[],[full(b'taic',bytes(17))],[full(b'itai',bytes(9))],[box(b'irot',b'\1')],[box(b'imir',b'\1')],[box(b'clap',struct.pack('>IIIIiiii',6,1,6,1,0,1,0,1))]]:
        add(f'combined-{extra!r}',action=2,file=fixture(props+extra))
    for n in ([] if without_hevc else [0,1,3,7,8,11,12,20]):add(f'failed-synthetic-{n}',action=2,file=synthetic(extra=[box(b'itai',bytes(n))]))
    from test_decode_overlay import overlay
    for kind in [b'iden',b'grid',b'iovl']:
        for root_props in [[],props]:
            for child_props in [[],[full(b'itai',stamp)],props]:
                for alpha_props in [None,[],[full(b'itai',bytes(9))]]:
                    child=dict(id=2,kind=b'mski',data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',b'\10')]+child_props)
                    root=dict(id=1,kind=kind,data=bytes([0,0,0,0])+struct.pack('>HH',8,8) if kind==b'grid' else overlay(8,8) if kind==b'iovl' else b'',props=[ispe(8,8)]+root_props,refs={b'dimg':[2]})
                    items=[root,child]
                    if alpha_props is not None:items.append(dict(id=3,kind=b'mski',data=bytes([255])*64,props=[ispe(8,8),full(b'mskC',b'\10'),full(b'auxC',b'urn:mpeg:hevc:2015:auxid:1\0')]+alpha_props,refs={b'auxl':[1]}))
                    add(f'derived-{kind}-{root_props!r}-{child_props!r}-{alpha_props!r}',action=2,file=item_file(items))
    for mode in [1,2]:
        add(f'context-mode-{mode}',action=2,file=fixture(props),mode=mode)
    for version in [0,1,255]:
        for flags in [0,1,2,0xffffff]:
            for mode in [1,2]:add(f'raw-dedup-{version}-{flags}-{mode}',action=1,source=version,flags=flags,mode=mode)
    if not without_hevc:
        from test_decode_geometry import children
        top=dict(children(Path('tests/upstream/fuzzing/data/corpus/colors-no-alpha.heic').read_bytes()))
        meta=dict(children(top[b'meta'][4:]));config=dict(children(dict(children(meta[b'iprp']))[b'ipco']))[b'hvcC']
        for transform in [[],[box(b'irot',b'\1')],[box(b'imir',b'\0')]]:
            for mode in [0,1,2]:
                add(f'hevc-{transform!r}-{mode}',action=2,mode=mode,file=item_file([dict(id=1,kind=b'hvc1',props=[ispe(64,64),box(b'hvcC',config)]+props+transform,data=top[b'mdat'])]))
    return cases,b''.join(data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/tai-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--without-hevc',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/tai-sanitized' if a.sanitize else '.build/tai').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/tai.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus(a.without_hevc);lines={};env=dict(os.environ)
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
    report=dict(scope=__doc__,without_hevc=a.without_hevc,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
