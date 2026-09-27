#!/usr/bin/env python3
"""Original-header component definition/configuration parsing, profile expansion, limits and owned type URIs."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
from pathlib import Path
from test_context import box,full,synthetic
from test_hevc import FIXTURES

def corpus():
    from item_fixtures import item_file,ispe
    cases=[]
    def fixture(props=(),kind=b'mski'):
        return item_file([dict(id=1,kind=kind,data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',bytes([8])),*props])])
    base=fixture()
    def add(name,props=(),maximum=256,pixel=256,tiles=16777216,version=4,reload=1,kind=b'mski'):
        file=fixture(props,kind)
        cases.append((name,struct.pack('=7I',len(file),len(base),maximum,pixel,tiles,version,reload)+file+base))
    def cmpd(types,uris=None,count=None):
        out=struct.pack('>I',len(types) if count is None else count)
        for i,t in enumerate(types):
            out+=struct.pack('>H',t)
            if t>=32768:out+=(uris[i] if uris else b'urn:test')+bytes([0])
        return box(b'cmpd',out)
    def config(components=((0,8,0,0),),sampling=0,interleave=0,block=0,flags=0,pixel=0,row=0,tile=0,cols=0,rows=0,version=0,profile=bytes(4)):
        data=profile
        if version==0:
            data+=struct.pack('>I',len(components))+b''.join(struct.pack('>HBBB',idx,bits-1,form,align) for idx,bits,form,align in components)
            data+=struct.pack('>BBBBIIIII',sampling,interleave,block,flags,pixel,row,tile,cols,rows)
        return full(b'uncC',data,version)
    add('no-properties')
    for types in [[],[0],[1,2,3],[4,5,6,7],[0,0,0],[16,17,32767,32768,65535]]:
        for maximum in [0,1,2,3,4,256]:
            add(f'types-{types}-{maximum}',props=[cmpd(types)],maximum=maximum)
            add(f'typed-config-{types}-{maximum}',props=[cmpd(types),config()],maximum=maximum)
    for uri in [b'',b'abc',b'urn:example',bytes([255,128]),b'abc'+bytes([0])+b'tail']:
        value=cmpd([32768],[uri]);data=value[8:]
        for n in range(len(data)+1):add(f'uri-prefix-{uri!r}-{n}',props=[box(b'cmpd',data[:n])])
    profiles=[b'rgb3',b'rgba',b'abgr',b'2vuy',b'yuv2',b'yvyu',b'vyuy',b'yuv1',b'v308',b'v408',b'y210',b'v410',b'v210',b'i420',b'nv12',b'nv21',b'yu22',b'yv22',b'yv20',b'zzzz',bytes(4)]
    for profile in profiles:
        for version in [0,1,2,255]:
            for explicit in [None,[],[1,2,3],[32768]]:
                props=[] if explicit is None else [cmpd(explicit)]
                add(f'profile-{profile!r}-{version}-{explicit}',props=props+[config(version=version,profile=profile)])
    for version in range(256):add(f'version-{version}',props=[config(version=version,profile=b'rgb3')])
    for field in ['sampling','interleave','block','flags']:
        for value in range(256):add(f'{field}-{value}',props=[cmpd([0]),config(**{field:value})])
    for bits in [1,7,8,9,16,32,64,128,129,256]:
        for form in [0,1,2,3,4,255]:
            for align in [0,1,2,16,255]:add(f'component-{bits}-{form}-{align}',props=[cmpd([0]),config(components=[(0,bits,form,align)])])
    for version in [0,1,3,4,255]:
        for maximum in [0,1,255,256,257]:
            for pixel in [0,1,255,256,257,4294967295]:add(f'pixel-{version}-{maximum}-{pixel}',props=[config(pixel=pixel)],pixel=maximum,version=version)
    for cols,rows in [(0,0),(1,1),(1,2),(65535,65535),(4294967294,0),(4294967295,0),(0,4294967295)]:
        for limit in [0,1,3,4,16777216]:add(f'tiles-{cols}-{rows}-{limit}',props=[config(cols=cols,rows=rows)],tiles=limit)
    for fullbox in [config(),config(version=1,profile=b'rgb3'),cmpd([0,1,32768])]:
        for n in range(len(fullbox)-7):add(f'box-prefix-{fullbox[4:8]}-{n}',props=[box(fullbox[4:8],fullbox[8:8+n])])
    for types in [[0,1,2],[32768]]:
        for other in [[],[4,5,6],[65535]]:
            add(f'duplicate-{types}-{other}',props=[cmpd(types),cmpd(other)])
    for count in [0,1,2,256,257,4294967295]:
        for n in range(8):add(f'count-{count}-{n}',props=[box(b'cmpd',struct.pack('>I',count)+bytes(n))],maximum=0)
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/uncompressed-config-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/uncompressed-config-sanitized' if a.sanitize else '.build/uncompressed-config').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/uncompressed_config.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases)+1 for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    cases=cases+[('null-pointers',b'')];differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
