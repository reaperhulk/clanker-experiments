#!/usr/bin/env python3
"""Original-header entity groups, filters, ordering, parser errors, resource limits, caller-owned arrays and reload/free lifetimes."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
from pathlib import Path

def corpus():
    from item_fixtures import item_file,ispe
    from test_context import box,full
    base=item_file([dict(id=1,kind=b'mski',data=bytes(range(48)),props=[ispe(8,6),full(b'mskC',bytes([8]))])])
    def group(kind=b'altr',ids=(1,2),ident=19,version=0,flags=0,tail=b''):
        p=struct.pack('>II',ident,len(ids))+b''.join(struct.pack('>I',i) for i in ids)+tail
        if kind==b'pymd':p+=bytes(4+6*len(ids))
        return box(kind,bytes([version])+flags.to_bytes(3,'big')+p)
    def fixture(groups):
        if groups is None:return base
        n=int.from_bytes(base[:4],'big');size=int.from_bytes(base[n:n+4],'big');return base[:n]+box(b'meta',base[n+8:n+size]+groups)+base[n+size:]
    reload=fixture(box(b'grpl',group(ident=71,ids=(3,1))))
    cases=[]
    def add(name,groups=None,flags=0,limit=0,reload_file=reload):
        f=fixture(groups);cases.append((name,struct.pack('=4I',len(f),len(reload_file),flags,limit)+f+reload_file))
    add('absent');add('empty',box(b'grpl',b''));add('unknown-only',box(b'grpl',box(b'nope',b'')))
    for kind in [b'altr',b'pymd',b'ster',b'eqiv',b'brst',b'tsyn',b'stem',b'aebr',b'wbbr',b'fobr',b'afbr',b'dobr',b'albc',b'favc',b'pano',b'slid',b'prgr']:
        add(f'kind-{kind}',box(b'grpl',group(kind)))
    for kind in [b'altr',b'pymd',b'ster']:
        for version in range(256):add(f'version-{kind}-{version}',box(b'grpl',group(kind,version=version)))
        for ids in [(),(0,),(1,),(2,),(1,1),(1,2),(2,1),(0xffffffff,1,7),(1,2,3,4)]:
            for ident in [0,1,19,0xffffffff]:add(f'ids-{kind}-{ids}-{ident}',box(b'grpl',group(kind,ids,ident)))
        complete=group(kind,ids=(1,2,3) if kind!=b'ster' else (1,2))
        for length in range(len(complete)-8+1):add(f'truncate-{kind}-{length}',box(b'grpl',box(kind,complete[8:8+length])))
        for count in [0,1,2,3,63,64,65,257]:
            for limit in [0,1,2,63,64,65]:add(f'limit-{kind}-{count}-{limit}',box(b'grpl',group(kind,tuple(range(count)))),1,limit)
        for flags in [1,0x10000,0xffffff]:add(f'flags-{kind}-{flags}',box(b'grpl',group(kind,flags=flags)))
    for count in [1,2,3,64,100,101,102]:
        groups=b''.join(group(ident=i,ids=(i,i+1)) for i in range(count))
        for limit in [0,1,2,7,100]:add(f'children-{count}-{limit}',box(b'grpl',groups),2,limit)
    add('duplicate-empty-first',box(b'grpl',b'')+box(b'grpl',group()))
    add('duplicate-first',box(b'grpl',group())+box(b'grpl',group(ident=991)))
    add('ordered-mixed',box(b'grpl',group()+box(b'nope',bytes(12))+group(b'ster',ids=(2,1))+group(b'pymd',ids=(1,2,7))))
    for length in range(1,8):add(f'bad-header-{length}',box(b'grpl',bytes(length)))
    for trailing in [b'',b'x',bytes(4),b'trailing bytes']:add(f'trailing-{len(trailing)}',box(b'grpl',group(tail=trailing)))
    for count in [0x7fffffff,0x80000000,0xffffffff]:add(f'count-overflow-{count}',box(b'grpl',full(b'altr',struct.pack('>IIII',1,count,1,2))))
    add('failed-reload',box(b'grpl',group()),reload_file=b'invalid')
    add('absent-reload',box(b'grpl',group()),reload_file=base)
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/entity_groups-report.json');p.add_argument('--work');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path(a.work or ('.build/entity_groups-sanitized' if a.sanitize else '.build/entity_groups')).resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/entity_groups.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
