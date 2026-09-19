#!/usr/bin/env python3
"""Original-header versioned security limits and operational boundary comparisons."""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
from test_context import synthetic,box
from item_fixtures import item_file,ispe
from test_context import full

def corpus():
    cases=[]
    fixtures=[('hevc',Path('tests/upstream/fuzzing/data/corpus/colors-no-alpha.heic').read_bytes()),('metadata',synthetic(metadata=bytes(129),extra=[box(b'colr',b'prof'+bytes(65))]))]
    for depth in [8,16]:
        fixtures.append((f'mask-{depth}',item_file([dict(id=1,kind=b'mski',data=bytes(17*9*(depth//8)),props=[ispe(17,9),full(b'mskC',bytes([depth]))])])))
    for family in ['grid-amplification-8','grid-amplification-11','overlay-depth-4']:
        fixtures.append((family,Path('.build/decode-graphs-inputs',family+'.heic').read_bytes()))
    for name,data in fixtures:
        for field in [0,1,3,4,5,6,7,9,10,14]:
            values={0:[0,1,152,153,154,4095,4096],1:[0,1,2],3:[0,1,2,3,24,25,34,1000,2**32-1],4:[0,1,64,65],5:[0,1,16,152,153,154,575,576,577,2048],6:[0,1,2],7:[0,1,2],9:[0,1,2,3,4,5],10:[0,1,575,576,577,2048],14:[0,1,2]}[field]
            for value in values:
                for phase in [0,1]:cases.append((f'{name}/{field}/{phase}/{value}',struct.pack('=IIIQ',len(data),field,phase,value)+data))
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/security-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    Path(a.output).unlink(missing_ok=True);work=Path('.build/security-sanitized' if a.sanitize else '.build/security').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes())
    libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={n:hashlib.sha256(v.read_bytes()).hexdigest() for n,v in libs.items()};client=hashlib.sha256(Path('tests/security.c').read_bytes()).hexdigest();cases=corpus();payload=b''.join(v for _,v in cases);out={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name
        subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}','tests/security.c',str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.stderr').write_bytes(run.stderr);(work/f'{name}.txt').write_bytes(run.stdout)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        out[name]=run.stdout.splitlines()
    expected=2+256*6+9+3*4*13*4+len(cases)
    if any(len(v)!=expected for v in out.values()):raise SystemExit(f'Incomplete transcript: expected {expected}, got {list(map(len,out.values()))}')
    differences=[{'line':i,'name':cases[i-(expected-len(cases))][0] if i>=expected-len(cases) else f'api-{i}','reference':x.decode(),'candidate':y.decode()} for i,(x,y) in enumerate(zip(out['reference'],out['candidate'],strict=True)) if x!=y]
    if hashes!={n:hashlib.sha256(v.read_bytes()).hexdigest() for n,v in libs.items()} or client!=hashlib.sha256(Path('tests/security.c').read_bytes()).hexdigest():raise SystemExit('Binaries/client changed; refusing mixed evidence')
    report={'scope':__doc__,'cases':expected,'mismatches':len(differences),'client_sanitizers':a.sanitize,'leak_check':a.sanitize and not a.no_leak_check,'client_sha256':client,'corpus_sha256':hashlib.sha256(payload).hexdigest(),**{n+'_sha256':h for n,h in hashes.items()},'differences':differences}
    Path(a.output).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:3],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
