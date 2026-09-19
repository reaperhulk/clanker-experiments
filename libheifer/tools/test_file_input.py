#!/usr/bin/env python3
"""Original-header file input, missing-file diagnostics, retained handles after unlink/open failure and context release."""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
from test_context import corpus,box,full
from item_fixtures import item_file,ispe

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--work',default='.build/file-input');p.add_argument('--output',default='.build/file-input-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args();work=Path(a.work).resolve();work.mkdir(parents=True,exist_ok=True);inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());cases=corpus(Path('tests/upstream'))
 for depth in [8,16]:
  data=item_file([dict(id=1,kind=b'mski',data=bytes((i*31+7)&255 for i in range(5*7*(depth//8))),props=[ispe(5,7),full(b'mskC',bytes([depth]))])]);ftyp=int.from_bytes(data[:4],'big')
  for kind in [b'free',b'mdat',b'zzzz']:
   for size in [0,1,24,32,8192,32768]:cases.append((f'mask-{depth}-{kind}-{size}',data[:ftyp]+box(kind,bytes(size))+data[ftyp:]))
 payload=b''.join(struct.pack('=I',len(data))+data for _,data in cases);libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};clients=[Path('tests/file_input.c'),Path('tests/context.c')];client_hash=hashlib.sha256(b''.join(p.read_bytes() for p in clients)).hexdigest();lines={};env=dict(os.environ)
 if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
 if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
 for name,lib in libs.items():
  binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(clients[0]),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True);r=subprocess.run([str(binary),str(work/'input.heif')],input=payload,capture_output=True,timeout=120,env=env);(work/f'{name}.txt').write_bytes(r.stdout);(work/f'{name}.stderr').write_bytes(r.stderr)
  if r.returncode:raise SystemExit(f'{name} exited {r.returncode}: {r.stderr.decode(errors="replace")[:2000]}')
  lines[name]=r.stdout.splitlines()
 if len(lines['reference'])!=len(cases) or len(lines['candidate'])!=len(cases):raise SystemExit('Incomplete transcripts')
 diffs=[dict(line=i,reference=x.decode(errors='replace'),candidate=y.decode(errors='replace')) for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)) if x!=y]
 assert hashes=={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()}
 report=dict(scope=__doc__,cases=len(cases),transcript_lines=len(lines['reference']),mismatches=len(diffs),differences=diffs,client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),**{k+'_sha256':v for k,v in hashes.items()});Path(a.output).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(diffs[:4],indent=2))
 if diffs:raise SystemExit(1)
if __name__=='__main__':main()
