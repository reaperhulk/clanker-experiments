#!/usr/bin/env python3
"""Real HEVC payloads with independently generated ordered geometry properties."""
from pathlib import Path
import struct
import sys
import test_decode
from test_context import box, full


def children(data):
    while data:
        size,kind=struct.unpack('>I4s',data[:8])
        if size<8 or size>len(data):raise ValueError('invalid source fixture')
        yield kind,data[8:size]
        data=data[size:]


def fixture(config,payload,properties):
    props=[box(b'hvcC',config),full(b'ispe',struct.pack('>II',64,64)),*properties]
    hdlr=full(b'hdlr',bytes(4)+b'pict'+bytes(12)+b'\0')
    pitm=full(b'pitm',struct.pack('>H',1))
    infe=full(b'infe',struct.pack('>HH4s',1,0,b'hvc1')+b'image\0',2)
    iinf=full(b'iinf',struct.pack('>H',1)+infe)
    iloc=full(b'iloc',struct.pack('>BBHHHHHII',68,0,1,1,1,0,1,0,len(payload)),1)
    ipma=full(b'ipma',struct.pack('>IHB',1,1,len(props))+bytes(range(1,len(props)+1)))
    return box(b'ftyp',b'heic'+bytes(4)+b'mif1heic')+full(b'meta',hdlr+pitm+iinf+iloc+box(b'iprp',box(b'ipco',b''.join(props))+ipma)+box(b'idat',payload))+box(b'free',bytes(32))


def main():
    source=Path('tests/upstream')
    top=dict(children((source/'fuzzing/data/corpus/colors-no-alpha.heic').read_bytes()))
    meta=dict(children(top[b'meta'][4:]));iprp=dict(children(meta[b'iprp']));props=dict(children(iprp[b'ipco']))
    config,payload=props[b'hvcC'],top[b'mdat']
    corpus=[]
    for rotation in range(4):
        for mirror in [None,0,1]:
            transforms=[box(b'irot',bytes([rotation]))]
            if mirror is not None:transforms.append(box(b'imir',bytes([mirror])))
            for reverse in [False,True]:
                corpus.append((f'rotation-{rotation}-mirror-{mirror}-reverse-{reverse}',list(reversed(transforms)) if reverse else transforms))
    fractions=[(64,1,64,1,0,1,0,1),(63,1,61,1,0,1,0,1),(13,2,15,2,1,2,-1,2),(5,1,7,1,-25,1,20,1),(1,1,1,1,0,1,0,1),(65,1,65,1,0,1,0,1),(0,1,2,1,0,1,0,1),(60,1,60,1,-90,1,-90,1),(60,1,60,1,90,1,90,1),(65537,65537,32767,10000,-17,3,17,3)]
    for index,frac in enumerate(fractions):
        clap=box(b'clap',struct.pack('>8I',*[n&0xffffffff for n in frac]))
        for rotation in range(4):
            for first in [True,False]:
                transform=[box(b'irot',bytes([rotation])),box(b'imir',b'\1')]
                corpus.append((f'clap-{index}-{rotation}-{first}',[clap,*transform] if first else [*transform,clap]))
    # Fraction parsing applies to all properties, even unused ones; validate every truncation and boundary.
    valid=struct.pack('>8I',63,1,61,1,0,1,0,1)
    for n in range(32):corpus.append((f'truncated-clap-{n}',[box(b'clap',valid[:n])]))
    for field in range(8):
        for value in [0,0x7fffffff,0x80000000,0xffffffff]:
            values=[63,1,61,1,0,1,0,1];values[field]=value
            corpus.append((f'fraction-{field}-{value}',[box(b'clap',struct.pack('>8I',*values))]))
    root=Path('.build/decode-geometry-inputs').resolve();root.mkdir(parents=True,exist_ok=True)
    paths=[]
    for name,properties in corpus:
        path=root/(name+'.heic');path.write_bytes(fixture(config,payload,properties));paths.append(str(path))
    paths.extend(str((source/'tests/data'/name).resolve()) for name in ['clap_cropped.heic','conformance_window_padding.heic','rainbow-451x461.heic','with-alpha-512x512.heic'])
    test_decode.FIXTURES=paths
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/decode-geometry-report.json'])
    if '--modes' not in sys.argv:sys.argv.extend(['--modes','1,2,3,6'])
    test_decode.main()


if __name__=='__main__':main()
