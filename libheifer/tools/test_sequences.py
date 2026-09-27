#!/usr/bin/env python3
"""Original-header sequence track ownership, options, references, metadata samples, timing and all defined serialized bytes (two indeterminate urim bytes excluded)."""
import argparse,hashlib,json,os,struct,subprocess,random
from pathlib import Path

def corpus():
    cases=[]
    def add(v): cases.append(('-'.join(map(str,v)),struct.pack('=8I',*v)))
    for kind in range(4):
        for tracks in [0,1,3]:
            for samples in [0,1,3]:
                for flags in [0,1,2,8,16,24,32,64,128,256,512,1024,2048|1024,4096,8192,16384,32768|32,65536|8,131072]:
                    if kind==3 and flags&1024:continue
                    add([kind,tracks,samples,90000,1000,1,flags,40])
    for scale in [0,1,1000,0xffffffff]:
        for movie in [0,1,1000,0xffffffff]:
            for reps in [0,1,2,0xffffffff]:add([0,2,3,scale,movie,reps,1,0xffffffff])
    for kind in [1,2]:
        for frames in [1,3]:
            for flags in [0,128,1024,8192,16384,32,64,32|32768,524288,1048576,2097152,131072]:
                for reps in [1,2,0]:add([kind,1,frames,1000,1000,reps,flags|262144,40])
    for flags in [4194304,8388608,4194304|8388608]:
        for kind in [0,1,2]:
            for samples in [1,3]:add([kind,2,samples,1000,1000,1,flags|(262144 if kind else 0),40])
    return cases,b''.join(data for _,data in cases)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/sequences-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/sequences-sanitized' if a.sanitize else '.build/sequences')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/sequences.c');client_bytes=lambda:client.read_bytes();client_hash=hashlib.sha256(client_bytes()).hexdigest();cases,payload=corpus();lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary),str(work/(name+'-output.heif'))],input=payload,capture_output=True,timeout=120,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,undefined_oracle_fields=["Box_URIMetaSampleEntry::data_reference_index is uninitialized upstream; two bytes per urim entry are explicitly excluded, never counted as parity evidence."],client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k not in ('differences','oracle_limitations')},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
