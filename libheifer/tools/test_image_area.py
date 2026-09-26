#!/usr/bin/env python3
"""Original-header extraction, zero extension and replicated padding: pixels, metadata, components and resource limits."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
from pathlib import Path

def corpus():
    cases=[]
    def add(name,op=0,cs=2,ch=0,depth=8,w=7,h=5,layout=0,x=0,y=0,tw=9,th=8,repeat=1,lm=0,pixels=0,block=0,total=0):
        cases.append((name,struct.pack('=16I',op,cs,ch,depth,w,h,layout,x,y,tw,th,repeat,lm,pixels,block,total)))
    configs=[(2,0),(0,1),(0,2),(0,3),(1,3),(1,10),(1,11),(1,12),(1,13),(1,14),(1,15)]
    for cs,ch in configs:
        for depth in [8,10,12,16]:
            if (ch in [10,11] and depth!=8) or (ch>=12 and depth<=8):continue
            for w,h in [(1,1),(7,5)]:
                for x,y in [(0,0),(1,0),(0,1),(1,1),(w-1,h-1),(w,h),(0xffffffff,0)]:
                    for tw,th in [(0,0),(1,1),(3,4),(w+2,h+2)]:
                        add(f'extract-{cs}-{ch}-{depth}-{w}-{h}-{x}-{y}-{tw}-{th}',cs=cs,ch=ch,depth=depth,w=w,h=h,x=x,y=y,tw=tw,th=th)
            for op in [1,2]:
                for tw,th in [(7,5),(8,6),(15,17),(64,64),(65,65),(66,67),(67,66),(7,0)]:
                    add(f'extend-{op}-{cs}-{ch}-{depth}-{tw}-{th}',op=op,cs=cs,ch=ch,depth=depth,tw=tw,th=th,repeat=th!=0)
    for layout in range(1,9):
        for depth in [1,8,12,16,17,32,64,128]:
            if layout in [2,7] and depth>16:continue
            for op in range(3):
                for tw,th in [(0,0),(7,5),(9,8),(65,66)]:
                    if op==2 and tw<8:continue
                    if op==1 and (tw<7 and th>64):continue
                    add(f'layout-{layout}-{depth}-{op}-{tw}-{th}',layout=layout,depth=depth,op=op,tw=tw,th=th,repeat=th!=0)
    for w,h in [(63,64),(64,63),(65,66)]:
        for op in range(3):
            for delta in [0,1,2,3]:
                add(f'physical-{w}-{h}-{op}-{delta}',w=w,h=h,op=op,tw=w+delta,th=h+delta)
    for lm in [0,1,2,3]:
        for pixels,block,total in [(1,0,0),(35,0,0),(36,0,0),(72,0,0),(0,1,0),(0,4110,0),(0,4111,0),(0,4112,0),(0,0,4111),(0,0,4112),(0,0,8222),(0,0,16000),(0,0,100000)]:
            for ch in [0,1]:
                for tw,th in [(7,5),(9,8),(65,66)]:
                    add(f'limits-{lm}-{pixels}-{block}-{total}-{ch}-{tw}-{th}',cs=2 if ch==0 else 0,ch=ch,lm=lm,pixels=pixels,block=block,total=total,tw=tw,th=th)
    for layout in [0,1,5]:
        for op in range(3):
            add(f'alignment-{layout}-{op}',op=op,layout=layout,tw=0xffffffff,th=5,lm=1,block=1)
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/image-area-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/image-area-sanitized' if a.sanitize else '.build/image-area').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/image_area.c');client_hash=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary),str(work/f'{name}-payloads.bin')],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest():raise SystemExit('Binaries/client changed')
    payloads_equal=filecmp.cmp(work/'reference-payloads.bin',work/'candidate-payloads.bin',shallow=False)
    if not payloads_equal and not differences:differences.append(dict(case='binary-payload-stream',error='Full byte comparison differs despite matching text fingerprints'))
    payload_hashes={}
    for name in libs:
        with (work/f'{name}-payloads.bin').open('rb') as f:payload_hashes[name]=hashlib.file_digest(f,'sha256').hexdigest()
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),full_payload_bytes_equal=payloads_equal,payload_sha256=payload_hashes,client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
    for name in libs:(work/f'{name}-payloads.bin').unlink()
if __name__=='__main__':main()
