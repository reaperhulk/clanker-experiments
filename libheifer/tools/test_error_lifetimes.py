#!/usr/bin/env python3
"""Original-header error-buffer lifetimes across unrelated objects and aliases."""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
from item_fixtures import item_file,ispe
from test_context import full

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/error-lifetimes-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/error-lifetimes-sanitized' if a.sanitize else '.build/error-lifetimes').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes())
    libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/error_lifetimes.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest()
    # Truncated mask payloads cause owned decode errors without optional codecs.
    data=item_file([dict(id=i,kind=b'mski',props=[ispe(17,9),full(b'mskC',bytes([8]))],data=bytes(5+i)) for i in [1,2]])
    payload=struct.pack('=I',len(data))+data;lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name
        subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=60,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=144 for v in lines.values()):raise SystemExit('Incomplete transcript')
    differences=[dict(line=i,reference=x.decode(),candidate=y.decode()) for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)) if x!=y]
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=144,mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()})
    output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
