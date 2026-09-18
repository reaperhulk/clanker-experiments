#!/usr/bin/env python3
"""Original-header handle components, parse ordering, alpha, JPEG descriptions and decoded component ID reconciliation."""
import argparse,hashlib,json,os,struct,subprocess,random
from pathlib import Path

def corpus(descriptions_only=False):
    from item_fixtures import item_file,ispe
    from test_context import box,full
    from test_decode_geometry import children
    from test_decode_overlay import overlay
    cases=[]
    def add(name,items,primary=1,mode=0):
        data=item_file(items,primary) if isinstance(items,list) else items
        cases.append((name,struct.pack('=II',len(data),0 if descriptions_only else mode)+data))
    conf=bytes.fromhex('01016000000090000000000078f000fcfdf8f800000f00')
    def hevc(id=1,config=conf,props=None):return dict(id=id,kind=b'hvc1',props=[ispe(7,5),box(b'hvcC',config)]+(props or []))
    def mask(id=1,props=None):return dict(id=id,kind=b'mski',props=[ispe(7,5),full(b'mskC',bytes([8]))]+(props or []),data=bytes(range(35)))
    for ch in range(4):
        for luma in range(8):
            for chroma in [0,3,7]:
                c=bytearray(conf);c[16]=ch;c[17]=luma;c[18]=chroma;add(f'hevc-{ch}-{luma}-{chroma}',[hevc(config=bytes(c))])
    for kind in [b'grid',b'iden',b'iovl']:
        for root_id,child_id in [(1,2),(2,1),(10,3)]:
            for child_kind in [b'hvc1',b'mski']:
                child=hevc(child_id) if child_kind==b'hvc1' else mask(child_id)
                payload=bytes([0,0,0,0])+struct.pack('>HH',7,5) if kind==b'grid' else overlay(7,5) if kind==b'iovl' else b''
                root=dict(id=root_id,kind=kind,props=[ispe(7,5)],data=payload,refs={b'dimg':[child_id]})
                for order in [False,True]:add(f'order-{kind}-{root_id}-{child_id}-{child_kind}-{order}',[root,child] if order else [child,root],root_id,mode=2 if child_kind==b'mski' else 0)
    alpha=full(b'auxC',b'urn:mpeg:hevc:2015:auxid:1\0')
    for master_kind in [b'hvc1',b'mski',b'iden',b'iovl']:
        for alpha_kind in [b'hvc1',b'mski']:
            for master_id,alpha_id in [(1,2),(2,1)]:
                master=hevc(master_id) if master_kind==b'hvc1' else mask(master_id);master['kind']=master_kind
                if master_kind==b'iovl':master['data']=overlay(7,5);master['refs']={b'dimg':[3]}
                if master_kind==b'iden':master['refs']={b'dimg':[3]}
                aux=hevc(alpha_id,props=[alpha]) if alpha_kind==b'hvc1' else mask(alpha_id,[alpha]);aux['refs']={b'auxl':[master_id]}
                add(f'alpha-{master_kind}-{alpha_kind}-{master_id}',[master,aux,mask(3)],master_id,mode=2 if master_kind!=b'hvc1' and alpha_kind==b'mski' else 0)
    for aux_kind in [b'iden',b'grid',b'iovl']:
        for depth in [0,3,7]:
            c=bytearray(conf);c[17]=depth
            root=hevc(1);aux=dict(id=2,kind=aux_kind,props=[ispe(7,5),alpha],data=bytes([0,0,0,0])+struct.pack('>HH',7,5) if aux_kind==b'grid' else overlay(7,5) if aux_kind==b'iovl' else b'',refs={b'auxl':[1],b'dimg':[3]})
            add(f'derived-alpha-{aux_kind}-{depth}',[root,aux,hevc(3,bytes(c))])
    for kind in [b'hvc1',b'mski',b'iovl']:
        for dimensions in [(0,0),(0,5),(7,0),(7,5),(2147483648,5)]:
            x=hevc() if kind==b'hvc1' else mask();x['kind']=kind;x['props'][0]=ispe(*dimensions)
            if kind==b'iovl':x['data']=overlay(7,5);x['refs']={b'dimg':[2]}
            add(f'dimensions-{kind}-{dimensions}',[x,mask(2)])
    def jpeg(precision=8,components=3,sampling=0x22,marker=0xc0):return bytes([0xff,marker,0,17,precision,0,5,0,7,components,1,sampling,0,2,0x11,0,3,0x11,0,0xff,0xd9,0,0])
    def jpgcase(name,data,config=b''):
        add(name,[dict(id=1,kind=b'jpeg',props=[ispe(7,5)]+([box(b'jpgC',config)] if config else []),data=data)])
    for precision in range(256):jpgcase(f'jpeg-precision-{precision}',jpeg(precision))
    for components in range(6):jpgcase(f'jpeg-components-{components}',jpeg(components=components))
    for sampling in range(256):jpgcase(f'jpeg-sampling-{sampling}',jpeg(sampling=sampling))
    for marker in range(0xc0,0xd0):jpgcase(f'jpeg-marker-{marker}',jpeg(marker=marker))
    sample=jpeg()
    for n in range(len(sample)+1):jpgcase(f'jpeg-prefix-{n}',sample[:n]);jpgcase(f'jpeg-split-{n}',sample[n:],sample[:n])
    from test_hevc import FIXTURES
    for fixture in FIXTURES:
        for mode in [1,2,3]:add(f'real-{fixture}-{mode}',(Path('tests/upstream')/fixture).read_bytes(),mode=mode)
    path=Path('tests/upstream/fuzzing/data/corpus/colors-no-alpha.heic');data=path.read_bytes()
    for mode in [1,2,3]:add(f'real-colors-{mode}',data,mode=mode)
    top=dict(children(data));meta=dict(children(top[b'meta'][4:]));props=dict(children(dict(children(meta[b'iprp']))[b'ipco']))
    for kind in [b'grid',b'iden',b'iovl']:
        for root_id,child_id in [(1,2),(2,1)]:
            root=dict(id=root_id,kind=kind,props=[ispe(64,64)],data=bytes([0,0,0,0])+struct.pack('>HH',64,64) if kind==b'grid' else overlay(64,64) if kind==b'iovl' else b'',refs={b'dimg':[child_id]})
            child=dict(id=child_id,kind=b'hvc1',props=[ispe(64,64),box(b'hvcC',props[b'hvcC'])],data=top[b'mdat'])
            for mode in [1,2,3]:add(f'decode-{kind}-{root_id}-{mode}',[root,child],root_id,mode)
    return cases,b''.join(data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/component-handles-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--descriptions-only',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/component-handles-sanitized' if a.sanitize else '.build/component-handles').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/component_handles.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus(a.descriptions_only);lines={};env=dict(os.environ)
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
    report=dict(scope=__doc__,descriptions_only=a.descriptions_only,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
