#!/usr/bin/env python3
"""Independent resource accounting across decoder caching, releases and file reloads."""
import argparse,hashlib,json,struct,subprocess
from pathlib import Path
from item_fixtures import item_file,ispe
from test_decode_geometry import children
from test_context import box,full

def corpus():
    original=Path('tests/upstream/fuzzing/data/corpus/colors-no-alpha.heic').read_bytes();top=dict(children(original));meta=dict(children(top[b'meta'][4:]));iprp=dict(children(meta[b'iprp']));props=dict(children(iprp[b'ipco']))
    mask=lambda ident:dict(id=ident,kind=b'mski',props=[ispe(8,8),full(b'mskC',bytes([8]))],data=bytes(64))
    hevc=lambda ident:dict(id=ident,kind=b'hvc1',props=[ispe(64,64),box(b'hvcC',props[b'hvcC'])],data=top[b'mdat'])
    pairs=[]
    for codec in [mask,hevc]:
        for metadata in [False,True]:
            items=[codec(1),codec(2)]
            if metadata:items.extend([dict(id=i+2,kind=b'Exif',data=bytes(129),refs={b'cdsc':[i]}) for i in [1,2]])
            first=item_file(items);second=item_file([mask(1)])
            pairs.append(struct.pack('=II',len(first),len(second))+first+second)
    return b''.join(pairs)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/security-lifetimes-report.json');a=p.parse_args();out=Path(a.output);out.unlink(missing_ok=True);work=Path('.build/security-lifetimes').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);reference=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((reference/'libheif/heif_version.h').read_bytes());libs={'reference':reference/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={n:hashlib.sha256(l.read_bytes()).hexdigest() for n,l in libs.items()};client=hashlib.sha256(Path('tests/security_lifetimes.c').read_bytes()).hexdigest();data=corpus();lines={}
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror','-Itests/upstream/libheif/api',f'-I{inc.parent}','tests/security_lifetimes.c',str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True);r=subprocess.run([str(binary)],input=data,capture_output=True,timeout=180);(work/f'{name}.txt').write_bytes(r.stdout);(work/f'{name}.stderr').write_bytes(r.stderr)
        if r.returncode:raise SystemExit(f'{name} exited {r.returncode}: {r.stderr[:1000]}')
        lines[name]=r.stdout.splitlines()
    if any(len(v)!=276 for v in lines.values()):raise SystemExit('Incomplete transcript')
    diffs=[{'line':i,'reference':x.decode(),'candidate':y.decode()} for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)) if x!=y]
    if hashes!={n:hashlib.sha256(l.read_bytes()).hexdigest() for n,l in libs.items()}:raise SystemExit('Library changed during test')
    report={'scope':__doc__,'cases':276,'mismatches':len(diffs),'client_sha256':client,'corpus_sha256':hashlib.sha256(data).hexdigest(),**{n+'_sha256':h for n,h in hashes.items()},'differences':diffs};out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(diffs[:2],indent=2))
    if diffs:raise SystemExit(1)
if __name__=='__main__':main()
