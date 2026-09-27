#!/usr/bin/env python3
"""Original-header handle HDR/aspect metadata, property mutation, decoded values and retained state."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
from pathlib import Path

def corpus():
    from item_fixtures import item_file,ispe
    from test_context import box,full
    cases=[]
    def fixture(props,kind=b'mski',child_props=()):
        image=dict(id=1,kind=kind,data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',bytes([8])),*props])
        items=[image]
        if kind in [b'iden',b'grid',b'iovl']:
            from test_decode_overlay import overlay
            image['data']=bytes(4)+struct.pack('>HH',8,8) if kind==b'grid' else overlay(8,8) if kind==b'iovl' else b'';image['refs']={b'dimg':[2]}
            items.append(dict(id=2,kind=b'mski',data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',bytes([8])),*child_props]))
        return item_file(items)
    def add(name,props=(),seed=0x12345678,flags=0,reload=None,kind=b'mski',child_props=()):
        file=fixture(props,kind,child_props);reload=fixture([]) if reload is None else reload
        cases.append((name,struct.pack('=4I',len(file),len(reload),flags,seed)+file+reload))
    data={b'clli':struct.pack('>HH',0x5678,0x1234),b'mdcv':struct.pack('>8H2I',0x5678,0x567b,0x5679,0x567c,0x567a,0x567d,0x567e,0x567f,0x12345678,0x12345679),b'amve':struct.pack('>IHH',0x12345678,0x1234,0x5678),b'ndwt':bytes(4)+struct.pack('>I',0x12345678),b'pasp':struct.pack('>II',0x12345678,0x12345679)}
    for flags in range(4):
        for seed in [0,1,65535,65536,0x7fffffff,0x80000000,0xffffffff]:add(f'set-{flags}-{seed}',seed=seed,flags=flags)
    for kind,p in data.items():
        for n in range(len(p)+1):add(f'prefix-{kind}-{n}',props=[box(kind,p[:n])],flags=2)
        for tail in [b'',bytes([0]),bytes([255])*9]:add(f'trailing-{kind}-{len(tail)}',props=[box(kind,p+tail)])
        for second in [bytes(len(p)),p,p[::-1]]:
            for flags in range(4):add(f'duplicate-{kind}-{second.hex()}-{flags}',props=[box(kind,p),box(kind,second)],flags=flags)
        for n in range(len(p)):
            add(f'recovery-{kind}-{n}',props=[box(kind,p[:n]),box(kind,p)])
    for version in range(256):add(f'ndwt-version-{version}',props=[box(b'ndwt',bytes([version,0,0,0])+data[b'ndwt'][4:])])
    for flags in [0,1,0x7fffff,0xffffff]:add(f'ndwt-flags-{flags}',props=[box(b'ndwt',bytes([0])+flags.to_bytes(3,'big')+data[b'ndwt'][4:])])
    all_props=[box(k,p) for k,p in data.items()]
    for props in [[],all_props,all_props[::-1]]:
        for next_props in [[],all_props,all_props[::-1]]:
            for flags in range(4):add(f'reload-{len(props)}-{props==all_props}-{len(next_props)}-{next_props==all_props}-{flags}',props=props,reload=fixture(next_props),flags=flags)
    for kind in [b'iden',b'grid',b'iovl']:
        for props in [[],all_props]:
            for child in [[],all_props]:
                for flags in range(4):add(f'derived-{kind}-{len(props)}-{len(child)}-{flags}',props=props,kind=kind,child_props=child,flags=flags)
    rng=random.Random(73532)
    for j in range(160):
        kind=rng.choice(list(data));payload=rng.randbytes(len(data[kind]))
        if kind==b'ndwt':payload=bytes(4)+payload[4:]
        add(f'random-{j}-{kind}',props=[box(kind,payload)],seed=rng.getrandbits(32),flags=rng.randrange(4))
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/handle-color-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/handle-color-sanitized' if a.sanitize else '.build/handle-color').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/handle_color.c');client_hash=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
    if a.sanitize:env['UBSAN_OPTIONS']='halt_on_error=1'
    if a.no_leak_check:env['ASAN_OPTIONS']='detect_leaks=0'
    for name,lib in libs.items():
        binary=work/name;subprocess.run(['cc','-std=c11','-O2','-Werror',*(['-fsanitize=address,undefined','-fno-omit-frame-pointer'] if a.sanitize else []),'-Itests/upstream/libheif/api',f'-I{inc.parent}',str(client),str(lib),f'-Wl,-rpath,{lib.parent}','-o',str(binary)],check=True)
        run=subprocess.run([str(binary)],input=payload,capture_output=True,timeout=180,env=env);(work/f'{name}.txt').write_bytes(run.stdout);(work/f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name]=run.stdout.splitlines()
    decoded_counts={k:sum(line.count(b'decoded:') for line in v) for k,v in lines.items()}
    if decoded_counts['reference']!=len(cases)*10:raise SystemExit(f'Unexpected successful reference decodes: {decoded_counts}')
    if any(len(v)!=len(cases) for v in lines.values()):raise SystemExit(f'Incomplete transcript: {list(map(len,lines.values()))}')
    differences=[]
    for i,(x,y) in enumerate(zip(lines['reference'],lines['candidate'],strict=True)):
        if x==y:continue
        at=next((j for j,(a,b) in enumerate(zip(x,y)) if a!=b),min(len(x),len(y)))
        differences.append(dict(case=cases[i][0],line=i,offset=at,reference=x[max(0,at-60):at+220].decode(),candidate=y[max(0,at-60):at+220].decode()))
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256((client.read_bytes()+Path('tests/transforms.c').read_bytes())).hexdigest():raise SystemExit('Binaries/client changed')
    report=dict(scope=__doc__,successful_decodes=decoded_counts,cases=len(cases),mismatches=len(differences),client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
