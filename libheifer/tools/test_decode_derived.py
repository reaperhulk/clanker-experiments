#!/usr/bin/env python3
"""Independent generated grid/identity containers around a real HEVC tile."""
from pathlib import Path
import struct
import sys
import test_decode
from test_context import box, full
from test_decode_geometry import children


def fixture(config,payload,rows=2,columns=2,width=127,height=125,kind=b'grid',references=None,iref=True,version=0,truncate=None,nested=False,cycle=False,miaf=False,wide=False,tile_sizes=None,tile_rotations=None,bad_identity=None,missing_config=None):
    grid=bytes([version,int(wide),rows-1,columns-1])+struct.pack('>II' if wide else '>HH',width,height)
    if truncate is not None:grid=grid[:truncate]
    tile_ids=list(range(10,10+(rows*columns if kind==b'grid' and references is None else 1)))
    items=[(1,kind,grid,(width,height),False)]+[(i,b'iden' if i==bad_identity else b'hvc1',payload,(tile_sizes or {}).get(i,(64,64)),True) for i in tile_ids]
    if nested:items.append((2,b'iden',b'',(width,height),True))
    props=[box(b'hvcC',config)]
    locations=bytearray();entries=[];associations=bytearray();data=bytearray()
    for item_id,item_kind,item_data,size,hidden in items:
        infe=full(b'infe',struct.pack('>HH4s',item_id,0,item_kind)+b'image\0',2)
        if hidden:infe=infe[:11]+b'\1'+infe[12:]
        entries.append(infe)
        locations+=struct.pack('>HHHHII',item_id,1,0,1,len(data),len(item_data));data+=item_data
        props.append(full(b'ispe',struct.pack('>II',*size)))
        indices=([1] if item_kind==b'hvc1' and item_id!=missing_config else [])+[len(props)]
        if item_id in (tile_rotations or {}):
            props.append(box(b'irot',bytes([tile_rotations[item_id]])));indices.append(len(props))
        associations+=struct.pack('>HB',item_id,len(indices))+bytes(indices)
    refs=references if references is not None else tile_ids
    if nested:refs=[2]
    if cycle:refs=[1]
    refs_data=box(b'dimg',struct.pack('>HH',1,len(refs))+b''.join(struct.pack('>H',r) for r in refs))
    if nested:refs_data+=box(b'dimg',struct.pack('>HHH',2,1,1 if cycle else 10))
    hdlr=full(b'hdlr',bytes(4)+b'pict'+bytes(12)+b'\0')
    meta=hdlr+full(b'pitm',struct.pack('>H',1))+full(b'iinf',struct.pack('>H',len(items))+b''.join(entries))
    meta+=full(b'iloc',struct.pack('>BBH',68,0,len(items))+locations,1)
    meta+=box(b'iprp',box(b'ipco',b''.join(props))+full(b'ipma',struct.pack('>I',len(items))+associations))
    if iref:meta+=full(b'iref',refs_data)
    return box(b'ftyp',b'heic'+bytes(4)+b'mif1heic'+(b'miaf' if miaf else b''))+full(b'meta',meta+box(b'idat',data))+box(b'free',bytes(32))


def main():
    source=Path('tests/upstream');top=dict(children((source/'fuzzing/data/corpus/colors-no-alpha.heic').read_bytes()))
    meta=dict(children(top[b'meta'][4:]));iprp=dict(children(meta[b'iprp']));props=dict(children(iprp[b'ipco']))
    config,payload=props[b'hvcC'],top[b'mdat']
    cases=[]
    for rows,cols in [(1,1),(1,3),(3,1),(2,2)]:
        for dw,dh in [(0,0),(1,1),(33,35)]:
            cases.append((f'grid-{rows}-{cols}-{dw}-{dh}',dict(rows=rows,columns=cols,width=64*cols-dw,height=64*rows-dh)))
    for kind in [b'grid',b'iden']:
        for refs in [[],[0],[1],[10],[10,10],[10,11,10,10]]:
            cases.append((f'{kind.decode()}-refs-{refs}',dict(kind=kind,references=refs,width=64,height=64)))
        cases.append((f'{kind.decode()}-no-iref',dict(kind=kind,iref=False)))
    for version in [1,255]:cases.append((f'grid-version-{version}',dict(version=version)))
    for n in range(8):cases.append((f'grid-truncated-{n}',dict(truncate=n)))
    for size in [(0,64),(64,0),(200,64),(64,200)]:cases.append((f'grid-size-{size}',dict(width=size[0],height=size[1])))
    for cycle in [False,True]:cases.append((f'identity-chain-{cycle}',dict(kind=b'iden',nested=True,cycle=cycle,width=64,height=64)))
    for refs in [[10,11,12,13],[10,11],[1,10,11,12]]:
        cases.append((f'grid-missing-{refs}',dict(references=refs)))
    for miaf in [False,True]:
        cases.append((f'identity-chain-miaf-{miaf}',dict(kind=b'iden',nested=True,width=64,height=64,miaf=miaf)))
    for wide in [False,True]:
        cases.append((f'grid-wide-{wide}',dict(wide=wide)))
    for n in range(8,12):cases.append((f'grid-wide-truncated-{n}',dict(wide=True,truncate=n)))
    for tile,sizes in [(10,(0,64)),(11,(60,64)),(12,(64,60)),(13,(65,64))]:
        cases.append((f'grid-tile-size-{tile}-{sizes}',dict(tile_sizes={tile:sizes})))
    cases.append(('grid-tile-rotations',dict(tile_rotations={10:0,11:1,12:2,13:3})))
    for tile in [10,11,13]:
        cases.append((f'grid-invalid-identity-{tile}',dict(bad_identity=tile)))
        cases.append((f'grid-missing-config-{tile}',dict(missing_config=tile)))
    root=Path('.build/decode-derived-inputs').resolve();root.mkdir(parents=True,exist_ok=True)
    paths=[]
    for name,kwargs in cases:
        path=root/(name+'.heic');path.write_bytes(fixture(config,payload,**kwargs));paths.append(str(path))
    test_decode.FIXTURES=paths
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/decode-derived-report.json'])
    if '--modes' not in sys.argv:sys.argv.extend(['--modes','0,1,2,3,6,9,10,11,23,24'])
    test_decode.main()


if __name__=='__main__':main()
