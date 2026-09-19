#!/usr/bin/env python3
"""Original-header sparse input: skip 16-GiB boxes under a 256-MiB address-space limit, then decode relocated idat pixels."""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
from item_fixtures import item_file,ispe
from test_context import full

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--work',default='.build/file-sparse');p.add_argument('--output',default='.build/file-sparse-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args();work=Path(a.work).resolve();work.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());data=item_file([dict(id=1,kind=b'mski',data=bytes((i*31+7)&255 for i in range(35)),props=[ispe(5,7),full(b'mskC',b'\x08')])]);ftyp=int.from_bytes(data[:4],'big');span=1<<34;cases=[];payload=[]
 for kind in [b'free',b'mdat',b'zzzz']:
  path=work/(kind.decode()+'.heif');head=data[:ftyp]+struct.pack('>I4sQ',1,kind,span)
  with path.open('wb') as out:out.write(head);out.seek(ftyp+span);out.write(data[ftyp:])
  cases.append(path);payload.append(head+data[ftyp:])
 libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/file_sparse.c');lines={};env=dict(os.environ)
 if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
 if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
 for name,lib in libs.items():
  binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True);lines[name]=[]
  for path in cases:
   r=subprocess.run([str(binary),str(path),str(int(not a.sanitize))],capture_output=True,timeout=15,env=env);(work/f'{name}-{path.stem}.txt').write_bytes(r.stdout);(work/f'{name}-{path.stem}.stderr').write_bytes(r.stderr)
   if r.returncode:raise SystemExit(f'{name} {path.stem} exited {r.returncode}: {r.stdout!r} {r.stderr!r}')
   lines[name].append(r.stdout)
 diffs=[dict(case=cases[i].stem,reference=x.decode(),candidate=y.decode()) for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)) if x!=y];report=dict(scope=__doc__,cases=len(cases),mismatches=len(diffs),differences=diffs,client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,address_space_limit=None if a.sanitize else 256*1024*1024,skipped_box_size=span,client_sha256=hashlib.sha256(client.read_bytes()).hexdigest(),corpus_sha256=hashlib.sha256(b''.join(payload)).hexdigest(),**{k+'_sha256':v for k,v in hashes.items()});Path(a.output).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
 if diffs:raise SystemExit(1)
if __name__=='__main__':main()
