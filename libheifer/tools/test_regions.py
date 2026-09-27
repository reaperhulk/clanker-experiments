#!/usr/bin/env python3
"""Original-header region geometry, parsing, transforms, masks, errors and retained object lifetimes."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
from pathlib import Path

def corpus():
    from item_fixtures import item_file,ispe
    from test_context import box,full
    cases=[]
    def fixture(payload=None,props=(),refs=None,mask=True):
        items=[dict(id=1,kind=b'mski',data=bytes(range(48)),props=[ispe(8,6),full(b'mskC',bytes([8])),*props])]
        if mask:items.append(dict(id=2,kind=b'mski',data=bytes(range(15)),props=[ispe(5,3),full(b'mskC',bytes([8]))]))
        if payload is not None:items.append(dict(id=3,kind=b'rgan',data=payload,refs={b'cdsc':[1],b'mask':[2]} if refs is None else refs))
        return item_file(items)
    def data(size=2,regions=(),width=13,height=17,version=0,flags=0):
        def scalar(v):return (v&((1<<(8*size))-1)).to_bytes(size,'big')
        return bytes([version,flags|(size==4)])+scalar(width)+scalar(height)+bytes([len(regions)])+b''.join(bytes([kind])+b''.join(scalar(v) for v in values)+tail for kind,values,tail in regions)
    all_regions=[(0,[-7,11],b''),(1,[-5,13,3,8],b''),(2,[21,-8,5,7],b''),(3,[3,-9,7,4,11,7,2],b''),(4,[-1,3,0,0],b''),(5,[-11,21,5,3],bytes([0,0xa3,0xe6])),(6,[2,3,-11,4,13],b'')]
    reload=fixture(data(regions=[(0,[91,-13],b'')],width=31,height=41))
    def add(name,payload=None,props=(),seed=19,flags=1,width=13,height=17,pattern=27,limit=0,refs=None,mask=True):
        f=fixture(payload,props,refs,mask);cases.append((name,struct.pack('=8I',len(f),len(reload),seed,flags,width,height,pattern,limit)+f+reload))
    for size in [2,4]:
        d=data(size,all_regions)
        add(f'complete-{size}',d)
        for length in range(len(d)):add(f'truncated-{size}-{length}',d[:length])
        for version in range(256):add(f'version-{size}-{version}',data(size,all_regions,version=version))
        for flags in [0,2,4,128,254]:add(f'flags-{size}-{flags}',data(size,all_regions,flags=flags))
        for kind in range(7,256):add(f'unknown-{size}-{kind}',data(size,[(kind,[],b''),(0,[-31,17],b'')]))
        for count in [0,1,2,3,4,17,257]:
            for kind in [3,6]:add(f'poly-{size}-{kind}-{count}',data(size,[(kind,[count]+list(range(count*2)),b'')]))
        for coding in range(256):add(f'coding-{size}-{coding}',data(size,[(5,[2,-4,3,5],bytes([coding,0xac,0x3f]))]))
        for w,h in [(0,1),(1,0),(0,0),(1,1),(3,5),(17,9),(65535,65535)]:
            add(f'mask-size-{size}-{w}-{h}',data(size,[(5,[3,7,w,h],bytes([0])+bytes(min((w*h+7)//8,32)))]))
        for limit in [0,1,7,8,15,16,23,24,47,48,64,128,256]:
            for flags in [4,8,16]:add(f'limit-{size}-{flags}-{limit}',d,flags=flags|1,limit=limit)
    for rot in range(4):
        for mirror in [None,0,1]:
            for order in range(2):
                props=[box(b'irot',bytes([rot]))]+([] if mirror is None else [box(b'imir',bytes([mirror]))])
                if order:props.reverse()
                for crop in [False,True]:
                    ps=props+[box(b'clap',struct.pack('>8I',4,1,3,1,0,1,0,1))] if crop else props
                    add(f'transform-{rot}-{mirror}-{order}-{crop}',data(regions=all_regions),props=ps,flags=0)
    for seed in [0,1,0x7fffffff,0x80000000,0xffffffff,0x12345678]:
        for flags in [0,2]:
            for pattern in range(0,221,11):add(f'writer-{seed}-{flags}-{pattern}',seed=seed,flags=flags,pattern=pattern)
    for w,h in [(0,0),(0,17),(13,0),(1,1),(65536,3),(0x7fffffff,0x80000000),(0xffffffff,0xffffffff)]:add(f'reference-{w}-{h}',data(regions=all_regions),flags=0,width=w,height=h)
    for refs in [{},{b'cdsc':[1]},{b'cdsc':[1,1],b'mask':[2]},{b'cdsc':[1,2],b'mask':[2]},{b'cdsc':[999],b'mask':[2]},{b'cdsc':[3],b'mask':[2]},{b'cdsc':[1],b'mask':[999]},{b'cdsc':[1],b'mask':[3]}]:
        # The uninitialized referenced_item field without a mask reference is
        # outside defined oracle behavior; use point-only regions for that case.
        regions=all_regions if b'mask' in refs else [all_regions[0]]
        add(f'refs-{refs}',data(regions=regions),refs=refs)
    rng=random.Random(0x7267616e)
    for trial in range(200):
        size=2 if trial%2 else 4;regions=[]
        for _ in range(rng.randrange(1,12)):
            kind=rng.randrange(7);x=rng.randrange(-32768,32768);y=rng.randrange(-32768,32768)
            if kind==0:values=[x,y];tail=b''
            elif kind in [3,6]:
                count=rng.randrange(2,16);values=[count]+[rng.randrange(-32768,32768) for _ in range(count*2)];tail=b''
            else:
                w=rng.randrange(1,20);h=rng.randrange(1,20);values=[x,y,w,h];tail=b''
                if kind==5:tail=bytes([0])+rng.randbytes((w*h+7)//8)
            regions.append((kind,values,tail))
        refs={b'cdsc':[1],b'mask':[2]*sum(k==4 for k,_,_ in regions)}
        if not refs[b'mask']:del refs[b'mask']
        add(f'random-{trial}',data(size,regions,width=rng.randrange(1,32768),height=rng.randrange(1,32768)),refs=refs)
    for w,h in [(0,0),(0,17),(13,0),(1,1),(0x80000000,0xffffffff)]:
        add(f'parsed-reference-{w}-{h}',data(4,all_regions,width=w,height=h))
    for w,h,x,y in [(0,0,0,0),(1,1,999,999),(99,99,0,0),(4,3,-1,1)]:
        prop=box(b'clap',struct.pack('>8I',w,1,h,1,x&0xffffffff,1,y&0xffffffff,1))
        add(f'clap-{w}-{h}-{x}-{y}',data(regions=all_regions),props=[prop])
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/regions-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/regions-sanitized' if a.sanitize else '.build/regions')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/regions.c');client_hash=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
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
