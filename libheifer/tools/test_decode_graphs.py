#!/usr/bin/env python3
"""Independent alpha overlays, MIAF, nesting and reference-amplification guards."""
from pathlib import Path
import struct
import sys
import test_decode
from test_context import box,full
from test_decode_geometry import children
from test_decode_overlay import overlay
from item_fixtures import item_file,ispe


def main():
    paths=[];root=Path('.build/decode-graphs-inputs').resolve();root.mkdir(parents=True,exist_ok=True)
    def add(name,items,primary=1,miaf=False):
        path=root/(name+'.heic');path.write_bytes(item_file(items,primary,miaf));paths.append(str(path))
    def mask(ident=1,size=8,alpha=False):
        props=[ispe(size,size),full(b'mskC',b'\10')]
        if alpha:props.append(full(b'auxC',b'urn:mpeg:hevc:2015:auxid:1\0'))
        return dict(id=ident,kind=b'mski',data=bytes((i*37+11)&255 for i in range(size*size)),props=props)
    for depth in [1,2,3,4,30]:
        items=[mask()];child=1
        for i in range(depth):
            ident=2+i;items.append(dict(id=ident,kind=b'iovl',props=[ispe(8,8)],data=overlay(8,8),refs={b'dimg':[child]}));child=ident
        add(f'overlay-depth-{depth}',items,child)
        add(f'overlay-miaf-depth-{depth}',items,child,True)
    for depth in [1,2,5,50,300]:
        items=[mask()];child=1
        for i in range(depth):
            ident=2+i;items.append(dict(id=ident,kind=b'iden',props=[ispe(8,8)],refs={b'dimg':[child]}));child=ident
        add(f'identity-depth-{depth}',items,child)
        add(f'identity-miaf-depth-{depth}',items,child,True)
    for depth in [1,3,8,11]:
        items=[mask()];child=1
        for i in range(depth):
            a,b,g=2+i*3,3+i*3,4+i*3
            for ident in [a,b]:items.append(dict(id=ident,kind=b'iden',props=[ispe(8,8)],refs={b'dimg':[child]}))
            items.append(dict(id=g,kind=b'grid',props=[ispe(8,8)],data=bytes([0,0,0,1])+struct.pack('>HH',8,8),refs={b'dimg':[a,b]}));child=g
        add(f'grid-amplification-{depth}',items,child)
        add(f'grid-miaf-amplification-{depth}',items,child,True)
    source=Path('tests/upstream');top=dict(children((source/'fuzzing/data/corpus/colors-no-alpha.heic').read_bytes()))
    meta=dict(children(top[b'meta'][4:]));iprp=dict(children(meta[b'iprp']));props=dict(children(iprp[b'ipco']))
    for wide in [False,True]:
        for xy in [(0,0),(1,3),(79,74),(-1,0),(0,-1),(-1,-1),(-33,-35),(-63,-63),(-64,0)]:
            alpha=mask(3,64,True);alpha['refs']={b'auxl':[2]}
            color=dict(id=2,kind=b'hvc1',data=top[b'mdat'],props=[ispe(64,64),box(b'hvcC',props[b'hvcC'])])
            canvas=dict(id=1,kind=b'iovl',data=overlay(offsets=[xy],wide=wide),props=[ispe(80,75)],refs={b'dimg':[2]})
            add(f'alpha-overlay-{int(wide)}-{xy}',[canvas,color,alpha])
    # A cycle involving auxiliary and derived edges must be rejected before workers start.
    alpha=mask(3,8,True);alpha['refs']={b'auxl':[2],b'dimg':[1]}
    add('mixed-alpha-cycle',[dict(id=1,kind=b'iden',props=[ispe(8,8)],refs={b'dimg':[2]}),dict(id=2,kind=b'iden',props=[ispe(8,8)],refs={b'dimg':[3]}),alpha])
    test_decode.FIXTURES=paths
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/decode-graphs-report.json'])
    if '--modes' not in sys.argv:sys.argv.extend(['--modes','25'])
    test_decode.main()


if __name__=='__main__':main()
