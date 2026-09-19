#!/usr/bin/env python3
"""Original-header dynamic module ABI, paths, repeated loading, versions, registry callbacks, bulk limits and initialization errors."""
import argparse,hashlib,json,os,subprocess
from pathlib import Path

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--work',default='.build/dynamic-plugins');p.add_argument('--output',default='.build/dynamic-plugins-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
 work=Path(a.work).resolve();work.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());compile_flags=['-std=c11','-O2','-Werror','-Itests/upstream/libheif/api',f'-I{inc.parent}',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else [])]
 module=work/'probe.so';subprocess.run(['cc',*compile_flags,'-fPIC','-shared','tests/dynamic_plugin.c','-o',str(module)],check=True)
 no_info=work/'no-info.so';source=work/'no-info.c';source.write_text('void no_plugin_info(void) {}\n');subprocess.run(['cc',*compile_flags,'-fPIC','-shared',str(source),'-o',str(no_info)],check=True)
 empty=work/'empty';empty.mkdir(exist_ok=True);bad=work/'bad';bad.mkdir(exist_ok=True);(bad/'bad.so').write_bytes(b'not an ELF module');(empty/'ignored.txt').write_text('ignored');(empty/'directory.so').mkdir(exist_ok=True)
 valid=[]
 for number in [1,2,3]:
  directory=work/f'valid-{number}';directory.mkdir(exist_ok=True)
  for i in range(number):
   link=directory/f'alias-{i}.so'
   if not link.exists():link.symlink_to(module)
  valid.append(directory)
 mixed=work/'mixed';mixed.mkdir(exist_ok=True)
 (mixed/'bad.so').write_bytes(b'not an ELF module')
 link=mixed/'valid.so'
 if not link.exists():link.symlink_to(module)
 missing=work/'missing.so';cases=[]
 def add(mode,path,kind=0,version=4,minimum=0,repeat=1,cap=4,flags=0,iv=1,env=''):
  args=[str(v) for v in [mode,path,kind,version,minimum,repeat,cap,flags,iv]];cases.append((args,env))
 for env in [None,'',':','::','a','a:',':a','a::b:','a;b','a:b:c',':/no/such/path:'] :add(0,module,env=env)
 for kind in [0,1,2,99]:
  for version in [-1,0,1,3,4,5,6,7]:
   for repeat in [1,2,3]:add(1,module,kind,version,repeat=repeat)
 for kind in [0,1,2]:
  for version in [3,4,5,6,7]:add(4,module,kind,version)
 for kind in [0,1]:
  for minimum in [0,0x01170300,0x01170400,0x01170401,0xffffffff]:
   for iv in [-1,0,1,2,2147483647]:add(1,module,kind,4 if kind==0 else 6,minimum,2,iv=iv)
 for path in [missing,bad/'bad.so',empty,no_info]:
  for repeat in [1,2]:add(1,path,repeat=repeat)
 for path in [empty,bad,missing,*valid,mixed]:
  for cap in [-2,0,1,4]:
   for flags in range(8):add(2,path,cap=cap,flags=flags)
 for path in [empty,bad,missing,*valid,mixed]:
  for repeat in [1,2,3]:add(3,path,repeat=repeat,flags=8)
 libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};clients=[Path('tests/dynamic_plugins.c'),Path('tests/dynamic_plugin.c')];client_hash=hashlib.sha256(b''.join(v.read_bytes() for v in clients)).hexdigest();transcripts={};failures=[];env=dict(os.environ)
 if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
 if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
 for name,lib in libs.items():
  binary=work/name;subprocess.run(['cc',*compile_flags,str(clients[0]),str(lib),'-ldl',f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True);lines=[]
  for i,(args,pathenv) in enumerate(cases):
   runenv=dict(env);runenv.pop('LIBHEIF_PLUGIN_PATH',None)
   if pathenv is not None:runenv['LIBHEIF_PLUGIN_PATH']=pathenv
   try:r=subprocess.run([str(binary),*args],capture_output=True,timeout=15,env=runenv)
   except subprocess.TimeoutExpired:failures.append(dict(library=name,case=i,timeout=True));lines.append(b'TIMEOUT');continue
   (work/f'{name}-{i}.txt').write_bytes(r.stdout);(work/f'{name}-{i}.stderr').write_bytes(r.stderr);lines.append(r.stdout+b'\nSTDERR\n'+r.stderr)
   if r.returncode:failures.append(dict(library=name,case=i,returncode=r.returncode,stderr=r.stderr.decode(errors='replace')[:2000]))
  transcripts[name]=lines
 safety=[]
 for kind,version in [(0,4),(1,6)]:
  r=subprocess.run([str(work/'candidate'),'5',str(module),str(kind),str(version),'0','1','4','0','1'],capture_output=True,env={**env,'LIBHEIF_PLUGIN_PATH':''},timeout=15);safety.append(dict(kind=kind,returncode=r.returncode,stdout=r.stdout.decode(errors='replace'),stderr=r.stderr.decode(errors='replace')))
  if r.returncode:failures.append(dict(candidate_safety=safety[-1]))
 differences=[dict(case=i,args=cases[i],reference=x.decode(errors='replace'),candidate=y.decode(errors='replace')) for i,(x,y) in enumerate(zip(transcripts['reference'],transcripts['candidate'],strict=True)) if x!=y]
 assert hashes=={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()}
 report=dict(scope=__doc__,candidate_safety_probes=safety,oracle_undefined_behavior=['Modules are pinned for oracle comparisons: native unload can dereference module data after dlclose, and decoder records remain registered. Unpinned candidate safety probes are separate and not parity counts.'],cases=len(cases),mismatches=len(differences),process_failures=failures,differences=differences,client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(json.dumps([[args[0],str(Path(args[1]).relative_to(work)),*args[2:],e] for args,e in cases]).encode()).hexdigest(),**{k+'_sha256':v for k,v in hashes.items()});Path(a.output).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k not in ('differences','process_failures')},indent=2));print(json.dumps(differences[:4],indent=2));print(json.dumps(failures[:4],indent=2))
 if differences or failures:raise SystemExit(1)
if __name__=='__main__':main()
