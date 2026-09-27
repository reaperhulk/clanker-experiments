#!/usr/bin/env python3
"""Full-byte original-header uncompressed pixels, component IDs, errors, callbacks and retained handles."""
import argparse,hashlib,json,os,random,struct,subprocess
from pathlib import Path
from test_context import box,full
from item_fixtures import item_file,ispe

def cmpd(types):return box(b'cmpd',struct.pack('>I',len(types))+b''.join(struct.pack('>H',t)+(b'urn:test\0' if t>=32768 else b'') for t in types))
def config(components=((0,8,0,0),),sampling=0,interleave=0,block=0,flags=0,pixel=0,row=0,tile=0,cols=0,rows=0,version=0,profile=bytes(4)):
    data=profile
    if version==0:
        data+=struct.pack('>I',len(components))+b''.join(struct.pack('>HBBB',idx,bits-1,form,align) for idx,bits,form,align in components)
        data+=struct.pack('>BBBBIIIII',sampling,interleave,block,flags,pixel,row,tile,cols,rows)
    return full(b'uncC',data,version)
def corpus():
    cases=[]
    def add(name,data,modes=(2,),required=False):
        for mode in modes:cases.append((f'{name}-mode{mode}',struct.pack('=II',len(data),mode)+data,required))
    for p in sorted(Path('tests/upstream/tests/data').glob('uncompressed*.heif')):add(p.name,p.read_bytes(),(0,1,2,3,11),True)
    def fixture(name,types=(0,),components=((0,8,0,0),),size=(4,4),data=None,props=(),modes=(2,),**opts):
        payload=bytes((i*37+11)&255 for i in range(32768)) if data is None else data
        pp=([ispe(*size)] if size is not None else [])+([cmpd(types)] if types is not None else [])+[config(components,**opts)]+list(props)
        add(name,item_file([dict(id=1,kind=b'unci',data=payload,props=pp)]),modes)
    for interleave in [0,1,2,3,4,5]:
        for depth in [1,2,3,4,5,6,7,8,9,10,12,16,24,32,64,128,129,256]:
            for form in range(4):fixture(f'layout-{interleave}-depth-{depth}-format-{form}',components=[(0,depth,form,0)],interleave=interleave)
        for align in [0,1,2,3,4,8,16]:
            for flags in [0,0x80]:fixture(f'align-{interleave}-{align}-{flags}',components=[(0,8,0,align)],flags=flags,interleave=interleave)
    for layout in [0,1,2,3,4]:
        for sample in [0,1,2,3]:
            for types in [(1,2,3),(3,2,1),(4,5,6),(7,6,5,4),(0,0),(4,5,12,6)]:
                fixture(f'channels-{layout}-{sample}-{types}',types=types,components=[(i,8,0,0) for i in range(len(types))],interleave=layout,sampling=sample)
        for n in [0,1,2,3,7,8,15,16,17,31,32,63,64,65]:fixture(f'truncated-{layout}-{n}',data=bytes(range(n)),interleave=layout)
        for row in [0,1,3,4,17,0xffffffff]:
            for tile in [0,1,17,32]:fixture(f'padding-{layout}-{row}-{tile}',row=row,tile=tile,interleave=layout,cols=1,rows=1)
    for layout in [0,1]:
        for block in [0,1,2,3,4,8,9]:
            for flags in [0,16,32,48,64,80,96,112,128,255]:
                for components,types in [([(0,10,0,0)],[0]), ([(0,5,0,0),(1,6,0,0),(2,5,0,0)],[4,5,6])]:
                    fixture(f'block-{layout}-{block}-{flags}-{types}',types=types,components=components,block=block,flags=flags,interleave=layout,pixel=block if layout==1 else 0,cols=1,rows=1,row=7,tile=19)
    for depth in range(1,17):
        for align in [0,1,2,3]:
            for pixel in [0,1,2,3,7,9]:fixture(f'pixel-bits-{depth}-{align}-{pixel}',components=[(0,depth,0,align)],interleave=1,pixel=pixel,row=7,cols=1,rows=1)
        for types in [(0,), (4,5,6)]:fixture(f'conversion-depth-{depth}-{types}',types=types,components=[(i,depth,0,0) for i in range(len(types))],modes=(3,4,5,6,7,8,12,13,14,15))
    for depth in [8,16,32,64,128]:
        for form in range(4):
            for align in [0,depth//8,depth//8+1]:
                for flags in [0,128]:fixture(f'wide-endian-{depth}-{form}-{align}-{flags}',components=[(0,depth,form,align)],flags=flags,cols=1,rows=1,row=19,tile=31)
    for types in [[],[0],[1],[2],[3],[7],[8],[9],[10],[11],[12],[13],[16],[32768],[4,5,6,7],[1,2,3,7]]:fixture(f'types-{types}',types=types,components=[(i,8,0,0) for i in range(len(types))])
    for idx in [0,1,2,65535]:fixture(f'index-{idx}',components=[(idx,8,0,0)])
    for size in [(0,0),(0,4),(4,0),(1,1),(3,5),(6,4),None]:
        for cols,rows in [(0,0),(1,1),(1,2),(9,9)]:fixture(f'size-{size}-{cols}-{rows}',size=size,cols=cols,rows=rows)
    for profile in [b'rgb3',b'rgba',b'abgr',b'2vuy',b'yuv2',b'yvyu',b'vyuy',b'yuv1',b'v308',b'v408',b'y210',b'v410',b'v210',b'i420',b'nv12',b'nv21',b'yu22',b'yv22',b'yv20']:
        for defs in [None,[],[4,5,6]]:fixture(f'profile-{profile}-{defs}',types=defs,version=1,profile=profile)
    from test_decode_overlay import overlay
    for kind in [b'iden',b'grid',b'iovl']:
        for profile in [b'rgb3',b'rgba',b'2vuy']:
            for root_id,child_id in [(1,2),(2,1)]:
                child=dict(id=child_id,kind=b'unci',data=bytes(range(256)),props=[ispe(4,4),config(version=1,profile=profile)])
                parent=dict(id=root_id,kind=kind,props=[ispe(4,4)],refs={b'dimg':[child_id]},data=bytes(4)+struct.pack('>HH',4,4) if kind==b'grid' else overlay(4,4) if kind==b'iovl' else b'')
                add(f'derived-{kind}-{profile}-{root_id}',item_file([parent,child],root_id))
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/uncompressed-pixels-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/uncompressed-pixels-sanitized' if a.sanitize else '.build/uncompressed-pixels')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/decode_uncompressed.c');client_bytes=lambda:client.read_bytes()+Path('tests/decode.c').read_bytes();client_hash=hashlib.sha256(client_bytes()).hexdigest();cases=corpus();payload=b''.join(data for _,data,_ in cases);records={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    decoded={};transcripts={}
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.bin').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]} (bytes={len(run.stdout)})')
        transcripts[name]=hashlib.sha256(run.stdout).hexdigest();records[name]=[];counts=[];offset=0
        for case,_,required in cases:
            n,success=struct.unpack_from('=II',run.stdout,offset);offset+=8;data=run.stdout[offset:offset+n];offset+=n
            if len(data)!=n:raise SystemExit('Truncated client transcript')
            if name=='reference' and required and success!=2:raise SystemExit(f'{case}: expected two successful reference decodes, got {success}')
            counts.append(success);records[name].append(data)
        if offset!=len(run.stdout):raise SystemExit('Unexpected client transcript length')
        decoded[name]=sum(counts)
    differences=[]
    for i,(x,y) in enumerate(zip(records['reference'],records['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],index=i,offset=at,reference=x[max(0,at-40):at+140].hex(),candidate=y[max(0,at-40):at+140].hex()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,successful_decodes=decoded,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),transcript_sha256=transcripts,differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
