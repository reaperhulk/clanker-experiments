#!/usr/bin/env python3
"""Original-header sequence parsing and sample reads from independent native fixtures and malformed box tables."""
import argparse,hashlib,json,os,subprocess
from pathlib import Path

def boxes(data,start=0,end=None):
    end=len(data) if end is None else end
    while start+8<=end:
        size=int.from_bytes(data[start:start+4],'big');kind=data[start+4:start+8]
        if size<8 or start+size>end:return
        yield kind,start,size
        off=16 if kind==b'stsd' else 12 if kind in [b'meta'] else 8
        if kind in [b'moov',b'trak',b'mdia',b'minf',b'stbl',b'stsd',b'meta',b'edts']:yield from boxes(data,start+off,start+size)
        start+=size

def corpus():
    cases=[]
    for fixture in sorted(Path('tests/fixtures/sequences').glob('*.hex')):
        data=bytes.fromhex(fixture.read_text());name=fixture.stem;cases.append((name,data))
        for kind,at,size in boxes(data):
            k=kind.decode('latin1');
            if kind in [b'ftyp',b'mdat']:continue
            b=bytearray(data);b[at+4:at+8]=b'zzzz';cases.append((name+'-missing-'+k,bytes(b)))
            if kind in [b'mvhd',b'tkhd',b'mdhd',b'hdlr',b'stsd',b'stts',b'stsc',b'stsz',b'stco',b'nmhd',b'vmhd']:
                for v in [1,2,255]:
                    b=bytearray(data);b[at+8]=v;cases.append((name+'-'+k+'-version-'+str(v),bytes(b)))
            if kind in [b'stts',b'stsc',b'stco']:
                for count in [0,1,4,0xffffffff]:
                    b=bytearray(data);b[at+12:at+16]=count.to_bytes(4,'big');cases.append((name+'-'+k+'-count-'+str(count),bytes(b)))
        for n in [0,1,7,8,15,24,31,32,len(data)-1,len(data)-8]:cases.append((name+'-prefix-'+str(n),data[:n]))
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/sequence-reading-report.json');p.add_argument('--work',default='.build/sequence-reading');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args();work=Path(a.work).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}','tests/sequence_reading.c',str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(work/name)],check=True)
    cases=corpus();diff=[];fail=[]
    for i,(name,data) in enumerate(cases):
        path=work/(str(i)+'.heif');path.write_bytes(data);out={}
        for side in libs:
            r=subprocess.run([str(work/side),str(path)],capture_output=True,env=env,timeout=15);(work/f'{i}-{side}.txt').write_bytes(r.stdout);(work/f'{i}-{side}.stderr').write_bytes(r.stderr)
            if r.returncode:fail.append(dict(case=name,side=side,returncode=r.returncode));out[side]=None
            else:out[side]=r.stdout
        if None in out.values():continue
        if out['reference']!=out['candidate']:
            x,y=out['reference'],out['candidate'];at=next((j for j,(u,v) in enumerate(zip(x,y)) if u!=v),min(len(x),len(y)));diff.append(dict(case=name,index=i,offset=at,reference=x[max(0,at-30):at+250].decode(),candidate=y[max(0,at-30):at+250].decode()))
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(diff),process_failures=fail,differences=diff,client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=hashlib.sha256(Path('tests/sequence_reading.c').read_bytes()+Path('tests/sequences.c').read_bytes()).hexdigest(),corpus_sha256=hashlib.sha256(b''.join(data for _,data in cases)).hexdigest(),**{k+'_sha256':hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()});Path(a.output).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(diff[:6],indent=2));
    if diff or fail:raise SystemExit(1)
if __name__=='__main__':main()
