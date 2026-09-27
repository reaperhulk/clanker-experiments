#!/usr/bin/env python3
"""First-box parsing through brand queries, including nested malformed boxes."""
import sys,struct,random
import test_brands
from test_context import box,full

TYPES=[b'meta',b'hdlr',b'pitm',b'iinf',b'infe',b'iprp',b'ipco',b'ipma',b'iloc',b'iref',b'ispe',b'auxC',b'irot',b'imir',b'clap',b'pasp',b'udes',b'cmin',b'cmex',b'hvcC',b'mskC',b'taic',b'itai',b'free',b'zzzz',b'ftyp']
def corpus():
    cases=[]
    for kind in TYPES:
        for version in [0,1,2,3,255]:
            for length in range(41):
                body=(bytes([version])+bytes(40))[:length]
                raw=box(kind,body)
                cases += [raw,full(b'meta',raw),box(b'ipco',raw),box(b'iprp',box(b'ipco',raw))]
    for size in [0,1,7,8,9,11,12,13,16,32,255,2**32-1]:
        for length in range(40):
            cases.append(struct.pack('>I4s',size,b'meta')+bytes(length))
    for size in [0,1,7,8,15,16,17,19,20,21,24,64,(1<<60)-1,1<<60,(1<<64)-1]:
        for length in [0,1,3,4,8,17,32]:cases.append(struct.pack('>I4sQ',1,b'meta',size)+bytes(length))
    for kind in [b'meta',b'iprp',b'ipco']:
        for n in [0,1,7,8,9,15,16,99,100,101,102,103]:
            content=box(b'free',b'')*n;cases.append(full(kind,content) if kind==b'meta' else box(kind,content))
        for length in range(1,9):
            for declared in [0,1,7,8,9,16,32]:
                payload=(struct.pack('>I4s',declared,b'free')+bytes(24))[:length]
                parent=full(kind,payload) if kind==b'meta' else box(kind,payload)
                cases.extend([parent,parent+box(b'free',bytes(64))])
    for n in [1,2,10,19,20,21,22,25]:
        content=box(b'free',b'')
        for _ in range(n):content=full(b'meta',content)
        cases.append(content)
    # Nonzero fields, truncated scalars, entry counts, arbitrary associations,
    # flags, and error precedence. This corpus is independent of Rust parsers.
    structures=[]
    for version in [0,1,2,3,255]:
        for count in [0,1,2,32,33,100,101,999,1000,1001,65535]:
            n=count.to_bytes(2 if version==0 else 4,'big')
            structures += [full(b'iinf',n+box(b'infe',bytes(16))*2,version),full(b'ipma',count.to_bytes(4,'big')+b'\0\1\2\x81\x02'*3,version)]
            structures += [full(b'iloc',b'\x44\x88'+count.to_bytes(2 if version<2 else 4,'big')+bytes(128),version)]
            width=2 if version==0 else 4
            for declared in [0,1,8,12,100]:
                entry=struct.pack('>I4s',declared,b'dimg')+(1).to_bytes(width,'big')+count.to_bytes(2,'big')+(2).to_bytes(width,'big')*3
                structures += [full(b'iref',entry,version),full(b'iref',entry*2,version)]
    for nibbles in range(65536):
        # Exercise each field width, without constructing the full Cartesian
        # product of equivalent ignored widths.
        if sum((nibbles>>shift)&15 not in [0,4,8] for shift in [0,4,8,12])>1:continue
        structures.append(full(b'iloc',struct.pack('>HHHHHH',nibbles,1,1,0,0,1)+bytes(64),1))
    for kind,uuid in [(b'cmin','22cc04c7d6d94e079d904eb6ecbaf3a3'),(b'cmex','4363e9145b7d4aab97aebea69803b434')]:
        for flags in [0,1,8,16,31,32,63,0x1f1f01,0xffffff]:
            for n in range(41):structures.append(box(b'uuid',bytes.fromhex(uuid)+flags.to_bytes(4,'big')+bytes(n)))
    for n in [0,1,7,8,15,16,17,24,1001*4+16]:
        structures.append(struct.pack('>I4s',n,b'ftyp')+b'avif'+bytes(16))
    for raw in structures:
        cases.append(full(b'meta',raw+bytes(4)))
        for size in sorted(set([len(raw),8,9,11,12,13,15,16,17,19,len(raw)-1])):
            cut=raw[:size]
            cases += [cut,full(b'meta',cut),box(b'ipco',cut),box(b'iprp',box(b'ipco',cut))]
    for targets in [[2],[2,2],[2,3]]:
        entry=box(b'dimg',struct.pack('>HH',1,len(targets))+b''.join(struct.pack('>H',n) for n in targets))
        for copies in [1,2,3]:
            for tail in [b'',bytes(4),box(b'free',bytes(1))]:
                cases.append(full(b'meta',full(b'iref',entry*copies)+tail))
    rng=random.Random(0x626f7873)
    for _ in range(5000):
        kind=rng.choice(TYPES);body=rng.randbytes(rng.randrange(129))
        if len(body)>=4 and rng.randrange(2):body=bytes([rng.randrange(4)])+body[1:]
        raw=box(kind,body)
        cases += [raw,full(b'meta',raw),box(b'ipco',raw)]
    return list(dict.fromkeys(cases))
if __name__=='__main__':
    test_brands.corpus=corpus
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/brand-boxes-report.json'])
    test_brands.main(scope=__doc__ + ' Finite coverage of 26 box types and camera UUID aliases, not the complete box factory.', work='brand-boxes', seed='0x626f7873')
