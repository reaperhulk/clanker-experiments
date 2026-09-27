#!/usr/bin/env python3
"""Original-header camera matrices, exact floating-point bits, parse warnings and lifetimes."""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
from item_fixtures import item_file,ispe
from test_context import box,full

UUIDS={b'cmin':bytes.fromhex('22cc04c7d6d94e079d904eb6ecbaf3a3'),b'cmex':bytes.fromhex('4363e9145b7d4aab97aebea69803b434')}
def cmin(flags=0,values=(2,3,4,5,6),version=0):return bytes([version])+flags.to_bytes(3,'big')+struct.pack('>5i',*values)
def cmex(flags=8,values=(0,0,0),version=0):
    body=bytes([version])+flags.to_bytes(3,'big')
    for bit in [1,2,4]:
        if flags&bit:body+=struct.pack('>i',-bit*123)
    if flags&8:body+=struct.pack('>3i' if flags&16 else '>3h',*values)
    if flags&32:body+=struct.pack('>I',1234567)
    return body
def corpus():
    cases=[]
    def add(name,props,size=(17,9),omit_ispe=False):
        properties=([] if omit_ispe else [ispe(*size)])+[full(b'mskC',b'\10')]+props
        data=item_file([dict(id=1,kind=b'mski',props=properties,data=bytes(17*9))])
        cases.append((name,data))
    add('empty',[])
    for kind,create in [(b'cmin',cmin),(b'cmex',cmex)]:
        for uuid in [False,True]:
            wrap=lambda data,k=kind,u=uuid:box(b'uuid',UUIDS[k]+data) if u else box(k,data)
            for version in range(256):add(f'{kind!r}-uuid{uuid}-version{version}',[wrap(create(version=version))])
            valid=create(flags=0x1f1f01 if kind==b'cmin' else 63)
            for n in range(len(valid)+1):add(f'{kind!r}-uuid{uuid}-prefix{n}',[wrap(valid[:n])])
    for shift in range(32):
        for skew in [0,1,15,31]:
            for anisotropic in [0,1]:
                flags=anisotropic|(shift<<8)|(skew<<16)
                add(f'cmin-fixed-{flags}',[box(b'cmin',cmin(flags,(-2147483648,2147483647,-1,1,-123456)))])
    for flags in range(64):
        add(f'cmex-flags-{flags}',[box(b'cmex',cmex(flags,(1,-2,3)))])
    for wide in [False,True]:
        scale=1073741824 if wide else 16384
        for field in range(3):
            for n in [-2*scale,-scale-1,-scale,-scale+1,-1,0,1,scale-1,scale,scale+1,2*scale-1]:
                values=[0,0,0];values[field]=n
                add(f'quaternion-{wide}-{field}-{n}',[box(b'cmex',cmex(24 if wide else 8,values))])
        for values in [(scale//2,scale//2,scale//2),(scale,1,0),(scale//3,-scale//4,scale//5)]:
            add(f'quaternion-mixed-{wide}-{values}',[box(b'cmex',cmex(24 if wide else 8,values))])
    intrinsic=box(b'cmin',cmin(0x401,(100,20,30,200,17)))
    extrinsic=box(b'cmex',cmex(8,(4096,-8192,0)))
    for rotation in range(4):
        for mirror in [0,1]:
            for crop in [None,(7,2,9,2,1,2,-1,3),(7,1,5,1,-5,1,3,1)]:
                transforms=[box(b'irot',bytes([rotation])),box(b'imir',bytes([mirror]))]
                if crop:transforms.append(box(b'clap',struct.pack('>IIIIiIiI',*crop)))
                for order in [transforms,list(reversed(transforms))]:
                    for matrices_first in [False,True]:
                        props=[intrinsic,extrinsic]+order if matrices_first else order+[intrinsic,extrinsic]
                        add(f'transforms-{rotation}-{mirror}-{crop}-{order!r}-{matrices_first}',props)
    for size in [(1,1),(65535,1),(2147483647,1),(2147483648,1),(4294967295,1)]:add(f'size-{size}',[intrinsic,extrinsic],size)
    add('missing-ispe',[intrinsic,extrinsic],omit_ispe=True)
    add('missing-ispe-bad-matrix',[full(b'cmin',b'',1),extrinsic],omit_ispe=True)
    add('duplicate-matrices',[intrinsic,box(b'cmin',cmin()),extrinsic,box(b'cmex',cmex())])
    add('bad-before-good',[full(b'cmin',b'',1),intrinsic,full(b'cmex',b'',1),extrinsic])
    add('good-before-bad',[intrinsic,full(b'cmin',b'',1),extrinsic,full(b'cmex',b'',1)])
    add('duplicate-ispe',[intrinsic,extrinsic,ispe(23,11)])
    return cases,b''.join(struct.pack('=I',len(data))+data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/camera-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/camera-sanitized' if a.sanitize else '.build/camera').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/camera.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
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
