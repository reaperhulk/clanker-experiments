#!/usr/bin/env python3
"""Original-header registered decoder callback dispatch, ownership, errors, polling, historical records and complete output pixels."""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
from test_context import corpus,box,full
from item_fixtures import item_file,ispe
from test_decode_overlay import overlay
CLIENT=Path('tests/plugin_decoding.c')

def cases():
    tests=[]
    configs={1:(b'hvc1',box(b'hvcC',bytes.fromhex('01016000000090000000000078f000fcfdf8f800000f00'))),4:(b'av01',box(b'av1C',bytes.fromhex('81000c00')))}
    def add(label,config,format=1,payload=b'\x00\x00\x00\x03ABC',extra=(),property_override=None):
        kind,prop=configs[format];prop=prop if property_override is None else property_override;data=item_file([dict(id=1,kind=kind,data=payload,props=[ispe(5,7),prop,*extra])]);tests.append((label,struct.pack('=12I',*(x&0xffffffff for x in config))+struct.pack('=I',len(data))+data))
    for format in configs:
        base=[5,format,0,0,0,0,5,7,8,0,0,0]
        for version in [1,2,3,4,5,6]:
            v=base.copy();v[0]=version;add(f'version-{format}-{version}',v,format)
        for flags in range(64):
            v=base.copy();v[9]=flags;add(f'flags-{format}-{flags}',v,format)
        for stage in [1,2,3,4]:
            for subcode in [0,100,2006,3003,4000]:
                for prefixed in [0,1]:
                    v=base.copy();v[2:5]=[stage,subcode,prefixed];add(f'failure-{format}-{stage}-{subcode}-{prefixed}',v,format)
        for delay in [1,2,49,50,51]:
            v=base.copy();v[5]=delay;add(f'delay-{format}-{delay}',v,format)
        for w,h in [(1,1),(5,6),(6,7),(5,7)]:
            for depth in [8,10,12,16]:
                v=base.copy();v[6:9]=[w,h,depth];add(f'output-{format}-{w}-{h}-{depth}',v,format)
        for strict in [0,1,2,255]:
            for threads in [-1,0,1,8]:
                v=base.copy();v[10:12]=[strict,threads];add(f'options-{format}-{strict}-{threads}',v,format)
        for n in range(8):add(f'payload-{format}-{n}',base,format,bytes(range(n)))
        for angle in range(4):add(f'rotation-{format}-{angle}',base,format,extra=[box(b'irot',bytes([angle]))])
    base=[5,4,0,0,0,0,5,7,8,0,0,0]
    for flags in range(256):
        add(f'av1-flags-{flags}',base,4,property_override=box(b'av1C',bytes([129,0,flags,0])+b'HEADER'))
    for size in range(5):add(f'av1-prefix-{size}',base,4,property_override=box(b'av1C',bytes.fromhex('81000c00')[:size]))
    add('av1-missing-config',base,4,property_override=b'')
    base[1]=1
    for length in range(4):
        for count in [0,1,3]:
            config=bytearray(configs[1][1][8:]);config[21]=(config[21]&252)|length;config[22]=count
            for a in range(count):
                config+=struct.pack('>BH',32+a*2,3)
                for unit in [b'A',b'XYZ',b'']:
                    config+=struct.pack('>H',len(unit))+unit
            add(f'hevc-nals-{length}-{count}',base,1,property_override=box(b'hvcC',config))
    for format in [1,4]:
        base=[5,format,0,0,0,0,5,7,8,0,0,0]
        kind,prop=configs[format]
        tile=dict(id=2,kind=kind,data=b'compressed',props=[ispe(5,7),prop])
        for derived in [b'iden',b'grid',b'iovl']:
            for angle in range(4):
                payload=b'' if derived==b'iden' else bytes([0,0,0,0])+struct.pack('>HH',5,7) if derived==b'grid' else overlay(width=5,height=7,offsets=[(0,0)])
                data=item_file([dict(id=1,kind=derived,data=payload,props=[ispe(5,7),box(b'irot',bytes([angle]))],refs={b'dimg':[2]}),tile])
                tests.append((f'derived-{format}-{derived}-{angle}',struct.pack('=12I',*base)+struct.pack('=I',len(data))+data))
    return tests

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--work',default='.build/plugin-decoding');p.add_argument('--output',default='.build/plugin-decoding-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args();Path(a.output).unlink(missing_ok=True);work=Path(a.work).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());tests=cases();payload=b''.join(data for _,data in tests);libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};clients=[CLIENT];lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(clients[0]),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True);r=subprocess.run([str(binary),str(work/"input.heif")],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.txt').write_bytes(r.stdout);(work/f'{name}.stderr').write_bytes(r.stderr)
        if r.returncode:raise SystemExit(f'{name} exited {r.returncode}: {r.stderr.decode(errors="replace")[:2000]}')
        lines[name]=r.stdout.splitlines()
    if len(lines['reference'])!=len(tests) or len(lines['candidate'])!=len(tests):raise SystemExit('Incomplete transcripts')
    diffs=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x!=y:
            at=next((j for j,(u,v) in enumerate(zip(x,y)) if u!=v),min(len(x),len(y)));diffs.append(dict(case=tests[i][0],index=i,offset=at,reference=x[max(0,at-30):at+400].decode(errors='replace'),candidate=y[max(0,at-30):at+400].decode(errors='replace')))
    assert hashes=={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()}
    report=dict(scope=__doc__,cases=len(tests),mismatches=len(diffs),differences=diffs,client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=hashlib.sha256(b''.join(p.read_bytes() for p in clients)).hexdigest(),corpus_sha256=hashlib.sha256(payload).hexdigest(),**{k+'_sha256':v for k,v in hashes.items()});Path(a.output).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(diffs[:8],indent=2))
    if diffs:raise SystemExit(1)
if __name__=='__main__':main()
