#!/usr/bin/env python3
"""Original-header generic item creation, queries, payloads, references, languages and reloads."""
import argparse,filecmp,hashlib,json,os,random,struct,subprocess,zlib
import brotli_fixtures
from pathlib import Path
from test_context import box,full,synthetic
from test_hevc import FIXTURES

def generic(items,omit=(),extra=b'',version=2,iloc_version=1,method=1,offset=0,length=None,handler=b'null'):
    entries=[];locations=b'';payload=b''
    for ident,kind,name,content,encoding,data in items:
        ident_bytes=ident.to_bytes(4 if version==3 else 2,'big')
        body=ident_bytes+bytes(2)+(kind if version>=2 else b'')+name+b'\0'
        if version<2 or kind==b'mime':body+=content+b'\0'+encoding+b'\0'
        if kind==b'uri ':body+=content+b'\0'
        entries.append(full(b'infe',body,version))
        locations+=ident.to_bytes(4 if iloc_version==2 else 2,'big')+(method.to_bytes(2,'big') if iloc_version else b'')+bytes(2)+struct.pack('>HII',1,len(payload)+offset,len(data) if length is None else length)
        payload+=data
    table=full(b'iinf',len(items).to_bytes(2,'big')+b''.join(entries))
    loc=full(b'iloc',b'\x44\0'+len(items).to_bytes(4 if iloc_version==2 else 2,'big')+locations,iloc_version)
    parts=[full(b'hdlr',bytes(4)+handler+bytes(12)+b'\0'),full(b'pitm',b'\0\1'),table,loc,box(b'iprp',box(b'ipco')+full(b'ipma',bytes(4))),box(b'idat',payload)]
    return box(b'ftyp',b'mif1'+bytes(4)+b'mif1')+full(b'meta',b''.join(p for p in parts if p[4:8] not in omit)+extra)+box(b'free',bytes(32))

def corpus():
    cases=[]
    def add(name,action=5,method=0,flags=0,param=b'zzzz',data=b'',file=b'',size=None,block=0xffffffff,total=0xffffffff):
        fields=[action,method&0xffffffff,flags,len(param),len(data),len(file),(len(data) if size is None else size)&0xffffffff,block,total]
        cases.append((name,struct.pack('=9I',*fields)+param+data+file))
    add('empty')
    for action in range(1,5):
        for flags in range(16):
            for size in [-1,0,1]:add(f'args-{action}-{flags}-{size}',action,flags=flags,param=b'abcd',data=b'X',size=size)
        for param in [b'',b'a',b'abc',b'abcde',b'\xff\x80ab',b'abc\0d',b'application/rdf+xml',b'urn:example:item']:
            add(f'param-{action}-{param!r}',action,param=param,data=b'ABC\0DEF')
    payloads=[b'',b'X',b'hello world',bytes(range(256)),bytes(8191),bytes(8192),bytes(8193),bytes(20000),b'abcde'*5000]
    rng=random.Random(0x6974656d)
    payloads += [rng.randbytes(n) for n in [17,257,8191,8192,8193,20000]]
    for n,data in enumerate(payloads):
        for method in range(-2,9):add(f'compress-{n}-{method}',2,method,param=b'text/plain',data=data)
        for method,wb in [(2,15),(3,-15),(4,'brotli')]:
            if wb=='brotli':encoded=brotli_fixtures.compress(data)
            else:obj=zlib.compressobj(wbits=wb);encoded=obj.compress(data)+obj.flush()
            for length in sorted(set([0,1,2,3,len(encoded)//2,len(encoded)-1,len(encoded)])):
                add(f'decompress-{n}-{method}-{length}',3,method,param=b'text/plain',data=encoded[:length])
            for block in [0,1,7,8191,8192,8193,20000]:add(f'block-{n}-{method}-{block}',3,method,param=b'text/plain',data=encoded,block=block)
            for total in [0,1,8191,8192,8193,20000]:add(f'total-{n}-{method}-{total}',3,method,param=b'text/plain',data=encoded,total=total)
    for encoding in range(9):
        for data in [b'',b'abc',b'\x78\x9c',b'\x78\x9c\xff\xff',bytes(range(256))]:add(f'encoding-{encoding}-{data.hex()}',3,encoding,param=b'',data=data)
    # Exact encoder bytes across symbol-buffer and sliding-window boundaries.
    for length in [2,3,4,5,16,127,128,129,257,258,259,261,262,263,16382,16383,16384,32505,32506,32507,32767,32768,32769,65273,65274,65275,65535,65536,65537,98303,98304,131071,262144,400000]:
        patterns=[bytes(length),(b'abacabadabacaba'*((length+14)//15))[:length],rng.randbytes(length),bytes(rng.randrange(8) for _ in range(length)),bytes(i%256 for i in range(length))]
        for pattern,data in enumerate(patterns):
            for method in [3,4,5]:add(f'compress-boundary-{length}-{pattern}-{method}',2,method,param=b'application/octet-stream',data=data)
    for n in range(100):
        alphabet=rng.randrange(2,257);data=bytes(rng.randrange(alphabet) for _ in range(rng.randrange(65537)))
        for method in [3,4,5]:add(f'compress-random-{n}-{method}',2,method,param=b'',data=data)
    # Malformed streams keep full error text, including zlib's diagnostic.
    for wb,method in [(15,2),(-15,3)]:
        obj=zlib.compressobj(wbits=wb);encoded=obj.compress(bytes(range(256))*10)+obj.flush()
        for offset in range(len(encoded)):
            corrupt=bytearray(encoded);corrupt[offset]^=1<<rng.randrange(8)
            add(f'corrupt-{method}-{offset}',3,method,param=b'',data=bytes(corrupt))
    base_items=[(1,b'zzzz',b'item',b'',b'',b'AB\0CD')]
    base=generic(base_items)
    for n in range(len(base)+1):add(f'file-prefix-{n}',0,file=base[:n])
    for kind in [b'zzzz',b'Exif',b'mime',b'uri ',bytes(4)]:
        for version in [0,1,2,3,4,255]:
            for name in [b'',b'ABC',b'\xff\x80']:
                add(f'file-{kind}-{version}-{name}',0,file=generic([(1,kind,name,b'text/plain',b'identity',b'hello')],version=version))
    for omit in [(),(b'iinf',),(b'iloc',),(b'idat',),(b'pitm',),(b'iprp',),(b'hdlr',)]:
        for handler in [b'null',b'pict']:
            add(f'missing-{omit}-{handler}',0,file=generic(base_items,omit=omit,handler=handler))
    for ids in [[0],[1],[17],[65535],[65536],[0xffffffff],[1,1],[3,1],[1,0xffffffff]]:
        add(f'ids-{ids}',0,file=generic([(i,b'zzzz',str(i).encode(),b'',b'',b'X') for i in ids],version=3,iloc_version=2))
    for method in range(4):
        for offset in [0,1,4,5,7,8,9,15,255,0xffffffff]:
            for length in [0,1,4,5,6,13,100]:add(f'extent-{method}-{offset}-{length}',0,file=generic(base_items,method=method,offset=offset,length=length))
    for version in [0,1]:
        for targets in [[1],[2,2],[2,3]]:
            width=2 if version==0 else 4
            entry=box(b'dimg',(1).to_bytes(width,'big')+len(targets).to_bytes(2,'big')+b''.join(n.to_bytes(width,'big') for n in targets))
            for copies in [1,2,3]:add(f'refs-{version}-{targets}-{copies}',0,file=generic(base_items,extra=full(b'iref',entry*copies,version)))
    for value in [b'\0'*4,b'\0'*4+b'en-GB\0',b'\0'*4+b'no-nul',b'\1\0\0\0en\0',b'']:
        add(f'language-{value!r}',0,file=synthetic(extra=[box(b'elng',value)]))
    for kind,body in [(b'iinf',bytes(6)),(b'iloc',bytes(8)),(b'iref',bytes(4))]:
        for n in range(len(body)+1):add(f'table-body-{kind}-{n}',0,file=generic(base_items,omit=(kind,),extra=box(kind,body[:n])))
    # Duplicate table boxes select the first parsed object, while malformed
    # later boxes still fail before any table pointers are installed.
    for kind,bodies in [(b'iinf',[bytes(6),bytes(5)]),(b'iloc',[bytes(8),bytes(7)]),(b'iref',[bytes(4),bytes(3)])]:
        for body in bodies:add(f'duplicate-table-{kind}-{len(body)}',0,file=generic(base_items,extra=box(kind,body)))
    for fixture in FIXTURES:add(fixture,0,file=(Path('tests/upstream')/fixture).read_bytes())
    return cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference-build',required=True);p.add_argument('--candidate',default='target/release/libheifer.so');p.add_argument('--output',default='.build/items-report.json');p.add_argument('--sanitize',action='store_true');p.add_argument('--no-leak-check',action='store_true');a=p.parse_args()
    if a.no_leak_check and not a.sanitize:p.error('--no-leak-check requires --sanitize')
    output=Path(a.output);output.unlink(missing_ok=True);work=Path('.build/items-sanitized' if a.sanitize else '.build/items').resolve();inc=work/'include/libheif';inc.mkdir(parents=True,exist_ok=True);ref=Path(a.reference_build).resolve();(inc/'heif_version.h').write_bytes((ref/'libheif/heif_version.h').read_bytes());libs={'reference':ref/'libheif/libheif.so','candidate':Path(a.candidate).resolve()};hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()};client=Path('tests/items.c');client_hash=hashlib.sha256(client.read_bytes()).hexdigest();cases=corpus();payload=b''.join(data for _,data in cases);lines={};env=dict(os.environ)
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
    if hashes!={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in libs.items()} or client_hash!=hashlib.sha256(client.read_bytes()).hexdigest():raise SystemExit('Binaries/client changed')
    payloads_equal=filecmp.cmp(work/'reference-payloads.bin',work/'candidate-payloads.bin',shallow=False)
    if not payloads_equal and not differences:differences.append(dict(case='binary-payload-stream',error='Full byte comparison differs despite matching text fingerprints'))
    payload_hashes={}
    for name in libs:
        with (work/f'{name}-payloads.bin').open('rb') as f:payload_hashes[name]=hashlib.file_digest(f,'sha256').hexdigest()
    report=dict(scope=__doc__,cases=len(cases),mismatches=len(differences),full_payload_bytes_equal=payloads_equal,payload_sha256=payload_hashes,client_sanitizers=a.sanitize,leak_check=a.sanitize and not a.no_leak_check,client_sha256=client_hash,corpus_sha256=hashlib.sha256(payload).hexdigest(),differences=differences,**{k+'_sha256':v for k,v in hashes.items()});output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='differences'},indent=2));print(json.dumps(differences[:4],indent=2))
    if differences:raise SystemExit(1)
if __name__=='__main__':main()
