#!/usr/bin/env python3
"""Original-header item-property IDs, raw/UUID bytes, descriptions, transforms and insertion."""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
from item_fixtures import item_file,ispe
from test_context import box,full

def children(data):
    out=[]
    while data:
        n=struct.unpack('>I',data[:4])[0]
        assert n>=8 and n<=len(data)
        out.append((data[4:8],data[8:n]));data=data[n:]
    return out

def rewrite(data,path,change):
    result=[]
    for kind,body in children(data):
        if kind==path[0]:
            if len(path)==1:
                replacement=change(body)
                if replacement is not None:result.append(box(kind,replacement))
                continue
            prefix=body[:4] if kind==b'meta' else b''
            body=prefix+rewrite(body[len(prefix):],path[1:],change)
        result.append(box(kind,body))
    return b''.join(result)

def corpus():
    cases=[]
    def add(name,props):
        data=item_file([dict(id=1,kind=b'mski',props=[ispe(8,8),full(b'mskC',bytes([8]))]+props,data=bytes(64)),dict(id=2,kind=b'mski',props=[ispe(8,8),full(b'mskC',bytes([8]))],data=bytes(64))])
        cases.append((name,data))
    add('empty',[])
    for n in [0,1,2,15,16,17,64,128]:add(f'raw-{n}',[box(b'raw!',bytes(range(n))),box(b'uuid',bytes(range(16))+bytes(range(n)))])
    add('raw-duplicate',[box(b'raw!',b'a'),box(b'raw!',b'a'),box(b'raw!',b'b')])
    for kind in [b'irot',b'imir']:
        for value in range(256):add(f'{kind!r}-{value}',[box(kind,bytes([value])),box(kind,bytes([value^1])),box(b'raw!',b'xyz')])
    for version in range(256):add(f'udes-version-{version}',[full(b'udes',b'en\0name\0description\0tags\0',version)])
    text=bytes(4)+b'en\0name\0description\0tags\0'
    for n in range(len(text)+1):add(f'udes-prefix-{n}',[box(b'udes',text[:n])])
    for a,b in [(1,1),(3,2),(8,1),(17,2),(65536,1)]:
        for x,y in [(0,0),(-5,5),(3,-3)]:
            add(f'crop-{a}-{b}-{x}-{y}',[box(b'clap',struct.pack('>IIIIiIiI',a,b,a,b,x,2,y,2))])
    add('mixed',[box(b'irot',b'\1'),box(b'imir',b'\1'),full(b'udes',b'\0n\0\0t\0'),box(b'uuid',bytes(16)+b'data'),box(b'raw!',b'x')])
    base=cases[-1][1]
    for path in [(b'iinf',),(b'pitm',),(b'iprp',),(b'iprp',b'ipco'),(b'iprp',b'ipma'),(b'iloc',),(b'hdlr',)]:
        cases.append((f'missing-{path}',rewrite(base,(b'meta',)+path,lambda _:None)))
    cases.append(('non-image-handler',rewrite(base,(b'meta',b'hdlr'),lambda p:p[:8]+b'meta'+p[12:])))
    def association(entries,version=0,wide=True):
        data=struct.pack('>I',len(entries))
        for item,indices in entries:
            data+=struct.pack('>IB' if version else '>HB',item,len(indices))
            data+=b''.join(struct.pack('>H' if wide else '>B',index) for index in indices)
        return bytes([version,0,0,int(wide)])+data
    for version in [0,1]:
        for wide in [False,True]:
            for entries in [[],[(1,[])],[(99,[3,4,5])],[(1,[1,2,0,3,0,4,5])],[(1,[1,2,127])],[(1,[1,2,3]),(1,[4,5])],[(2,[1,2]),(1,[1,2,5])]]:
                cases.append((f'associations-{version}-{wide}-{entries}',rewrite(base,(b'meta',b'iprp',b'ipma'),lambda _,e=entries,v=version,w=wide:association(e,v,w))))
    for n in [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]:
        cases.append((f'ipma-prefix-{n}',rewrite(base,(b'meta',b'iprp',b'ipma'),lambda p,n=n:p[:n])))
    for wide in [False,True]:
        flag=32768 if wide else 128
        for prop in [3,4,5,6,7]:
            for order in [[prop|flag],[prop,prop|flag],[prop|flag,prop]]:
                entries=[(1,[1,2]+order),(2,[1,2])]
                cases.append((f'essential-{wide}-{order}',rewrite(base,(b'meta',b'iprp',b'ipma'),lambda _,e=entries,w=wide:association(e,0,w))))
    for kind in [b'irot',b'imir']:
        add(f'empty-{kind!r}',[box(kind,b'')])
    for n in range(32):add(f'clap-prefix-{n}',[box(b'clap',struct.pack('>IIIIiIiI',4,1,4,1,0,1,0,1)[:n])])
    cases.extend([('empty-input',b''),('ftyp-only',box(b'ftyp',b'heic'+bytes(4)+b'heic'))])
    return cases,b''.join(struct.pack('=I',len(data))+data for _,data in cases)
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/properties-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/properties-sanitized' if a.sanitize else '.build/properties').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/properties.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=120,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases)+17 for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    cases=[(f'in-memory-{i}',b'') for i in range(16)]+cases+[('null-context',b'')]
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+200].decode(),candidate=y[max(0,at-60):at+200].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
