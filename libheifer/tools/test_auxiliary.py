#!/usr/bin/env python3
"""Original-header auxiliary/depth APIs, SEI values, filters, ownership and reloads."""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
from item_fixtures import item_file,ispe
from test_context import box,full

DEPTH=b'urn:mpeg:hevc:2015:auxid:2'
ALPHA=b'urn:mpeg:hevc:2015:auxid:1'
def ue(n):
    b=bin(n+1)[2:];return '0'*(len(b)-1)+b
def element(sign,exponent,length,mantissa):return f'{sign:01b}{exponent:07b}{length-1:05b}{mantissa:0{length}b}'
def sei(flags=15,kind=0,view=0,values=None,header=39,payload=177):
    values=values or [(0,31,8,127),(1,0,32,2**32-1),(0,127,1,1),(1,5,17,10000)]
    bits=f'{flags:04b}'+ue(kind)+(ue(view) if flags&3 else '')
    for i,value in enumerate(values):
        if flags&(8>>i):bits+=element(*value)
    bits+='0'*(-len(bits)%8);body=int(bits,2).to_bytes(len(bits)//8,'big');nal=bytes([header<<1,1,payload,len(body)])+body
    return struct.pack('>II',len(nal)+4,len(nal))+nal
def mask(ident,kind=None,subtypes=b'',targets=None,version=0):
    props=[ispe(8,8),full(b'mskC',bytes([8]))]
    if kind is not None:props.append(full(b'auxC',kind+b'\0'+subtypes,version))
    return dict(id=ident,kind=b'mski',props=props,data=bytes(64),refs={} if targets is None else {b'auxl':targets})
def corpus():
    cases=[]
    def add(name,items):cases.append((name,item_file(items)))
    def depth(name,data):add(name,[mask(1),mask(2,DEPTH,data,[1])])
    add('no-aux',[mask(1)])
    kinds=[b'',b'custom',ALPHA,DEPTH,b'urn:mpeg:avc:2015:auxid:1',b'urn:mpeg:mpegB:cicp:systems:auxiliary:alpha',b'urn:mpeg:mpegB:cicp:systems:auxiliary:depth',b'\xff\xfe']
    for kind in kinds:
        for targets in [None,[1],[1,1],[99],[2],[1,99],[99,1]]:
            add(f'kind-{kind!r}-{targets}',[mask(1),mask(2,kind,b'',[1] if targets==[1] else targets)])
    add('mixed',[mask(1)]+[mask(i+2,k,b'', [1]) for i,k in enumerate(kinds[:6])])
    add('two-depth',[mask(1),mask(2,DEPTH,sei(8),[1]),mask(3,DEPTH,sei(4),[1])])
    add('depth-to-two',[mask(1),mask(2),mask(3,DEPTH,sei(),[1,2])])
    add('jpeg-child',[mask(1),dict(id=2,kind=b'jpeg',props=[ispe(8,8),full(b'auxC',DEPTH+b'\0')],data=bytes(4),refs={b'auxl':[1]})])
    add('missing-hevc-config-child',[mask(1),dict(id=2,kind=b'hvc1',props=[ispe(8,8),full(b'auxC',DEPTH+b'\0')],data=bytes(4),refs={b'auxl':[1]})])
    for version in range(256):add(f'version-{version}',[mask(1),mask(2,DEPTH,b'',[1],version)])
    for n in range(9):
        child=mask(2,targets=[1]);child['props'].append(box(b'auxC',(bytes(4)+b'type\0')[:n]));add(f'property-prefix-{n}',[mask(1),child])
    for flags in range(16):
        for kind in range(4):depth(f'flags-{flags}-{kind}',sei(flags,kind,127))
    for length in range(1,33):
        for sign in [0,1]:depth(f'mantissa-{length}-{sign}',sei(15,2,32,[(sign,e,length,(1<<length)-1) for e in [0,1,31,127]]))
    for view in [0,1,254,255,65535,2097150]:depth(f'view-{view}',sei(3,1,view))
    for kind in [4,255,2097150]:depth(f'invalid-kind-{kind}',sei(kind=kind))
    sample=sei()
    for n in range(len(sample)+1):depth(f'sei-prefix-{n}',sample[:n])
    for declared in [0,1,4,5,2**32-1]:depth(f'outer-size-{declared}',struct.pack('>I',declared)+sample[4:])
    for nal in [0,1,38,39,40,41,63]:depth(f'nal-kind-{nal}',sei(header=nal))
    for payload in [0,1,176,177,178,255]:depth(f'payload-{payload}',sei(payload=payload))
    depth('two-messages',sei(8)+sei(4));depth('zero-variable-code',struct.pack('>II',100,100)+b'\x4e\x01\xb1\xff'+bytes(8))
    replacement=item_file([mask(1)])
    return cases,b''.join(struct.pack('=II',len(data),len(replacement))+data+replacement for _,data in cases)
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/auxiliary-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/auxiliary-sanitized' if a.sanitize else '.build/auxiliary').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/auxiliary.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=120,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit('Incomplete transcript')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+200].decode(),candidate=y[max(0,at-60):at+200].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
