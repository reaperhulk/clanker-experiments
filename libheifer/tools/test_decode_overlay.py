#!/usr/bin/env python3
"""Original-header overlay comparisons: clipping, versions, sizes and references."""
from pathlib import Path
import struct
import sys
import test_decode
from test_decode_derived import fixture
from test_decode_geometry import children


def overlay(width=80,height=75,offsets=((0,0),),wide=False,version=0,background=(0x1234,0xabcd,0xff,0x8000)):
    return bytes([version,int(wide)])+struct.pack('>4H',*background)+struct.pack('>II' if wide else '>HH',width,height)+b''.join(struct.pack('>ii' if wide else '>hh',*xy) for xy in offsets)


def main():
    source=Path('tests/upstream');top=dict(children((source/'fuzzing/data/corpus/colors-no-alpha.heic').read_bytes()))
    meta=dict(children(top[b'meta'][4:]));iprp=dict(children(meta[b'iprp']));props=dict(children(iprp[b'ipco']))
    config,payload=props[b'hvcC'],top[b'mdat']
    cases=[]
    def add(name,settings=None,**options):
        settings=settings or {}
        width,height=settings.get('width',80),settings.get('height',75)
        data=overlay(**settings)
        count=len(settings.get('offsets',[(0,0)]))
        cases.append((name,dict(kind=b'iovl',tile_count=count,width=width,height=height,root_data=data,**options)))
    for wide in [False,True]:
        for xy in [(0,0),(1,3),(79,74),(80,75),(81,76),(-1,0),(0,-1),(-1,-1),(-33,-35),(-63,-63),(-64,0),(0,-64),(-65,-65),(-32768,-32768),(32767,32767)]:
            add(f'offset-{int(wide)}-{xy}',dict(wide=wide,offsets=[xy]))
        for count in [1,2,5,6]:
            add(f'inputs-{int(wide)}-{count}',dict(wide=wide,offsets=[(i*7,i*9) for i in range(count)]),tile_rotations={10+i:i%4 for i in range(count)})
    for xy in [(-2**31,0),(0,-2**31),(2**31-1,2**31-1)]:add(f'wide-boundary-{xy}',dict(wide=True,offsets=[xy]))
    for size in [(0,75),(80,0),(1,1),(31,29),(128,128),(32769,32769),(2**31,1)]:
        add(f'size-{size}',dict(wide=True,width=size[0],height=size[1]))
    for version in [1,255]:add(f'version-{version}',dict(version=version))
    for wide in [False,True]:
        data=overlay(wide=wide)
        for length in range(len(data)):
            cases.append((f'truncated-{int(wide)}-{length}',dict(kind=b'iovl',tile_count=1,width=80,height=75,root_data=data[:length])))
    add('no-iref',iref=False)
    add('no-inputs',omit_dimg=True)
    add('self-reference',references=[1])
    add('missing-input',references=[12])
    add('missing-config',missing_config=10)
    add('invalid-identity',bad_identity=10)
    for size in [(79,75),(80,74),(81,76)]:
        cases.append((f'ispe-mismatch-{size}',dict(kind=b'iovl',tile_count=1,width=size[0],height=size[1],root_data=overlay())))
    root=Path('.build/decode-overlay-inputs').resolve();root.mkdir(parents=True,exist_ok=True)
    paths=[]
    for name,options in cases:
        path=root/(name+'.heic');path.write_bytes(fixture(config,payload,**options));paths.append(str(path))
    test_decode.FIXTURES=paths
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/decode-overlay-report.json'])
    if '--modes' not in sys.argv:sys.argv.extend(['--modes','0,1,2,3,4,6,10,11'])
    test_decode.main()


if __name__=='__main__':main()
