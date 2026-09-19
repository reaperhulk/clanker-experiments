#!/usr/bin/env python3
"""Original-header GIMI handle IDs, shared component properties, owned C strings, decoded metadata and reload/free lifetimes."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
from pathlib import Path

def corpus():
    from item_fixtures import item_file,ispe
    from test_context import box,full
    content_uuid=bytes.fromhex('261ef3741d975bbaacbd9d2c8ea73522');component_uuid=bytes.fromhex('9db9dd6e373c5a4e811021fc83a911fd')
    def content(value):return box(b'uuid',content_uuid+value)
    def component(values):return box(b'uuid',component_uuid+struct.pack('>I',len(values))+b''.join(v+b'\0' for v in values))
    def fixture(props=(),other=None,kind=b'mski'):
        p=[ispe(8,8),full(b'mskC',bytes([8]))]
        items=[dict(id=1,kind=kind,data=bytes(range(64)),props=p+list(props)),dict(id=2,kind=b'mski',data=bytes(range(64)),props=p+list(props if other is None else other))]
        if kind in [b'iden',b'grid',b'iovl']:
            from test_decode_overlay import overlay
            items[0]['refs']={b'dimg':[2]};items[0]['data']=bytes(4)+struct.pack('>HH',8,8) if kind==b'grid' else overlay(8,8) if kind==b'iovl' else b''
        return item_file(items)
    reload=fixture([content(b'reloaded'),component([b'new',b'file'])]);cases=[]
    def add(name,props=(),other=None,kind=b'mski',index=1,text=b'changed',flags=0,limit=256):
        file=fixture(props,other,kind);cases.append((name,struct.pack('=6I',len(file),len(reload),index,flags,len(text),limit)+file+reload+text))
    strings=[b'',b'a',b'hello',b'\0',b'\0tail',b'before\0after',bytes(range(1,256)),b'long'*1024]
    for i,value in enumerate(strings):
        for index in [0,1,2,7,31]:
            for flags in [0,2]:add(f'string-{i}-{index}-{flags}',[content(value),component([value,b'second'])],index=index,text=value,flags=flags)
    for first in [[],[component([])],[component([b'first',b'second'])]]:
        for second in [None,[],[component([b'other'])]]:
            add(f'sharing-{first}-{second}',first,second)
    for first in strings[:6]:
        for second in strings[:6]:add(f'duplicate-{first}-{second}',[content(first),content(second),component([first]),component([second])])
    payload=struct.pack('>I',4)+b'alpha\0beta\0\0delta\0'
    for length in range(len(payload)+1):add(f'truncate-{length}',[box(b'uuid',component_uuid+payload[:length])])
    for count in [0,1,2,3,255,256,257,0xffffffff]:
        for limit in [0,1,2,3,255,256,257]:
            add(f'limit-{count}-{limit}',[box(b'uuid',component_uuid+struct.pack('>I',count)+b'ID\0'*min(count,257))],flags=1,limit=limit)
    for n in range(256):add(f'byte-{n}',[content(bytes([n])),component([bytes([n])])])
    for kind in [b'iden',b'grid',b'iovl']:
        for parent in [[],[content(b'parent')]]:
            for child in [[],[content(b'child')]]:add(f'derived-{kind}-{len(parent)}-{len(child)}',parent,child,kind)
    for tail in [b'',b'ignored',bytes(16)]:add(f'trailing-{len(tail)}',[box(b'uuid',component_uuid+payload+tail)])
    add('second-image-malformed',[content(b'first-good')],[box(b'uuid',component_uuid+bytes([1]))])
    for index in range(17):
        add(f'index-{index}',[content(b'abc'),component([b'one',b'two'])],index=index,text=b'changed')
    for value in [b'',b'invalid',struct.pack('>I',0xffffffff)]:add(f'unknown-ccid-{len(value)}',[box(b'ccid',value)])
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/gimi-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/gimi-sanitized' if a.sanitize else '.build/gimi')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/gimi.c');client_hash=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
