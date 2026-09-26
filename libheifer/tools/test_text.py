#!/usr/bin/env python3
"""Original-header text items, attachments, content bytes, language properties and retained lookup state."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
import brotli_fixtures
from pathlib import Path
from test_context import box,full,synthetic
from test_hevc import FIXTURES

def corpus():
    from item_fixtures import item_file,ispe
    cases=[]
    def fixture(data=b'hello',content=b'text/plain',encoding=b'',targets=(1,),props=(),kind=b'mime',duplicate=False,raw_info=None,as_metadata=False):
        items=[dict(id=1,kind=b'mski',data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',b'\10')]),dict(id=2,kind=b'mski',data=bytes(range(64)),props=[ispe(8,8),full(b'mskC',b'\10')]),dict(id=3,kind=kind,data=data,props=list(props),refs={b'text':list(targets)} if targets is not None else {})]
        if as_metadata:items[2].setdefault("refs",{})[b"cdsc"]=[1]
        source=item_file(items)
        def rewrite(data,prefix=0):
            out=data[:prefix];at=prefix
            while at<len(data):
                size=int.from_bytes(data[at:at+4],'big');k=data[at+4:at+8];body=data[at+8:at+size]
                if k==b'meta':body=rewrite(body,4)
                elif k==b'iinf':body=rewrite(body,6)
                elif k==b'infe' and body[4:6]==b'\0\3':
                    if raw_info is not None:body=body[:12]+raw_info
                    elif kind==b'mime':body+=content+b'\0'+encoding+b'\0'
                elif k==b'iref' and duplicate:body+=body[4:]
                out+=box(k,body);at+=size
            return out
        return rewrite(source)
    base=fixture()
    def add(name,file=base,text=b'new text',content=b'text/plain',language=b'en-GB',flags=0,block=1,total=1,reload=None):
        reload=file if reload is None else reload
        cases.append((name,struct.pack('=8I',len(file),len(text),len(content),len(language),flags,block,total,len(reload))+file+text+content+language+reload))
    for flags in range(32):
        for text in [b'',b'Hello',b'ABC\0DEF',b'\xff\x80',bytes(range(256))]:add(f'create-{flags}-{text!r}',flags=flags,text=text)
    for ctype in [b'',b'text/plain',b'text/html',b'application/json',b'image/svg+xml',b'\xff\x80']:
        for content in [b'',b'ABC',b'ABC\0DEF',b'\xff\x80',bytes(range(256)),bytes(8193)]:
            for targets in [None,(),(0,),(1,),(2,),(1,2),(1,3),(3,),(4,),(1,1)]:
                add(f'read-{ctype!r}-{len(content)}-{targets}',file=fixture(content,ctype,targets=targets),content=ctype,text=content)
    for version in [0,1,2,255]:
        for language in [b'',b'en-GB',b'fr\0DE',b'\xff\x80',b'no-nul']:
            data=bytes([version,0,0,0])+language+(b'' if language==b'no-nul' else b'\0')
            add(f'language-{version}-{language!r}',file=fixture(props=[box(b'elng',data)]),language=language)
    for kind in [b'zzzz',b'Exif',b'uri ']:add(f'non-mime-{kind}',file=fixture(kind=kind))
    for duplicate in [False,True]:add(f'duplicate-references-{duplicate}',file=fixture(duplicate=duplicate))
    for encoding,window in [(b'',None),(b'identity',None),(b'compress_zlib',15),(b'deflate',-15),(b'unknown',None),(b'br',None),(b'br','brotli')]:
        for n in [0,1,17,8191,8192,8193,20000]:
            data=(b'abcdefg'*((n+6)//7))[:n]
            if window=='brotli':data=brotli_fixtures.compress(data)
            elif window:
                obj=zlib.compressobj(wbits=window);data=obj.compress(data)+obj.flush()
            for length in sorted(set([0,1,len(data)//2,len(data)-1,len(data)])):
                if length>=0:add(f'compressed-{encoding!r}-{window}-{n}-{length}',file=fixture(data[:length],encoding=encoding))
    for kind in [b'mime',b'uri ',b'zzzz']:
        for value in [b'',b'A',b'ABC',b'\0',b'A\0',b'A\0x',b'A\0text/plain',b'A\0text/plain\0identity',b'A\0text/plain\0identity\0']:
            add(f'unterminated-{kind}-{value!r}',file=fixture(kind=kind,raw_info=value,as_metadata=True))
    for original in [b'old',b'new',b'',b'OLD\0TAIL']:
        for replacement in [b'next',b'',b'NEXT\0TAIL']:
            add(f'changed-reload-{original!r}-{replacement!r}',file=fixture(original),reload=fixture(replacement))
    for n in range(len(base)+1):add(f'prefix-{n}',file=base[:n])
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/text-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/text-sanitized' if a.sanitize else '.build/text').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/text.c');client_hash=hashlib.sha256((client.read_bytes()+Path('tests/items.c').read_bytes())).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
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
