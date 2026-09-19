#!/usr/bin/env python3
"""Original-header metadata writers: Exif offsets, XMP compression, URI behavior, references and handle visibility."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
from pathlib import Path
from test_context import box,full,synthetic
from test_hevc import FIXTURES

def corpus():
    from item_fixtures import item_file,ispe
    from test_context import synthetic
    cases=[]
    mask=item_file([dict(id=1,kind=b'mski',data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',b'\10')])])
    old_metadata=synthetic(metadata=b'existing metadata')
    def add(name,action=0,method=0,flags=0,data=b'ABC',kind=b'zzzz',content=b'text/plain',file=mask,block=1,total=1):
        values=[action,method&0xffffffff,flags,len(data),len(kind),len(content),len(file),block,total]
        cases.append((name,struct.pack('=9I',*values)+data+kind+content+file))
    for action in range(5):
        for flags in range(128):
            for data in [b'',b'ABC\0DEF']:
                add(f'args-{action}-{flags}-{len(data)}',action=action,flags=flags,data=data)
    for n in range(32):
        for signature in [b'MM\0*',b'II*\0',b'Exif',b'ABCD']:
            for suffix in [b'',b'x',bytes(32)]:add(f'exif-{n}-{signature!r}-{len(suffix)}',data=b'0'*n+signature+suffix)
    rng=random.Random(0x6d657461)
    for n in [0,1,2,3,4,5,127,128,256,8191,8192,8193,20000,65536]:
        for pattern,data in enumerate([bytes(n),rng.randbytes(n),(b'abcde'*((n+4)//5))[:n]]):
            for method in range(-2,9):add(f'xmp-{n}-{pattern}-{method}',action=2,method=method,data=data)
    for kind in [b'',b'x',b'abc',b'abcde',b'Exif',b'mime',b'uri ',b'zzzz',b'\xff\x80ab',b'ab\0d']:
        for content in [b'',b'abc',b'\xff\x80',b'application/rdf+xml']:
            add(f'generic-{kind!r}-{content!r}',action=3,kind=kind,content=content,file=old_metadata)
    for action in range(5):
        for flags in [0,1,2,3]:add(f'existing-{action}-{flags}',action=action,flags=flags,file=old_metadata)
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/add-metadata-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/add-metadata-sanitized' if a.sanitize else '.build/add-metadata').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/add_metadata.c');client_hash=hashlib.sha256((client.read_bytes()+Path('tests/items.c').read_bytes())).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary),str(work/f'{name}-payloads.bin')],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases)+1 for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    cases=cases+[('null-pointers',b'')];differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256((client.read_bytes()+Path('tests/items.c').read_bytes())).hexdigest():raise SystemExit('Binaries/client changed')
    payloads_equal=filecmp.cmp(work/'reference-payloads.bin',work/'candidate-payloads.bin',shallow=False)
    if not payloads_equal and not differences:differences.append(dict(case='binary-payload-stream',error='Full byte comparison differs despite matching text fingerprints'))
    payload_hashes={}
    for name in libs:
        with (work/f'{name}-payloads.bin').open('rb') as f:payload_hashes[name]=hashlib.file_digest(f,'sha256').hexdigest()
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),full_payload_bytes_equal=payloads_equal,payload_sha256=payload_hashes,client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
