#!/usr/bin/env python3
"""Independent HEVC configuration errors and coded-size checks before codec entry."""
import struct,sys
from pathlib import Path
import test_decode
from item_fixtures import item_file,ispe
from test_context import box
from test_decode_geometry import children


def ue(n):
    bits=bin(n+1)[2:];return '0'*(len(bits)-1)+bits

def sps(width=1048575,height=1048575,chroma=1,luma=0,color=0,crop=None,layers=0,profiles=0,levels=0):
    bits=f'{0:04b}{layers:03b}1'+'0'*96
    if layers:
        bits+=''.join(f'{int(bool(profiles>>i&1))}{int(bool(levels>>i&1))}' for i in range(layers))+'0'*((8-layers)*2)
        for i in range(layers):
            if profiles>>i&1:bits+='0'*56
            if levels>>i&1:bits+='0'*8
    bits+=ue(0)+ue(chroma)+('0' if chroma==3 else '')+ue(width)+ue(height)+('1'+''.join(map(ue,crop)) if crop else '0')+ue(luma)+ue(color)
    bits+='0'*(-len(bits)%8)
    return b'\x42\x01'+int(bits,2).to_bytes(len(bits)//8,'big')

def main():
    raw=(Path('tests/upstream/fuzzing/data/corpus/colors-no-alpha.heic').read_bytes());top=dict(children(raw));meta=dict(children(top[b'meta'][4:]));props=dict(children(dict(children(meta[b'iprp']))[b'ipco']));base=props[b'hvcC'][:22]
    cases=[]
    def add_arrays(name,arrays,payload=None,dimensions=(64,64)):
        config=base+bytes([len(arrays)])
        for kind,nals in arrays:
            config+=bytes([kind|128])+struct.pack('>H',len(nals))+b''.join(struct.pack('>H',len(nal))+nal for nal in nals)
        cases.append((name,item_file([dict(id=1,kind=b'hvc1',data=top[b'mdat'] if payload is None else payload,props=[ispe(*dimensions),box(b'hvcC',config)])])))
    def add(name,nal):add_arrays(name,[(33,[nal])])
    prefix=sps()
    for n in range(len(prefix)+1):add(f'prefix-{n}',prefix[:n])
    for w,h in [(0,1),(1,0),(256,257),(257,256),(1048575,1),(1,1048575),(2097150,2097150)]:add(f'size-{w}-{h}',sps(w,h))
    for chroma in [0,1,2,3,4,255,2097150]:add(f'chroma-{chroma}',sps(chroma=chroma))
    for depth in [0,8,9,255,2097150]:
        add(f'luma-{depth}',sps(luma=depth));add(f'color-{depth}',sps(color=depth))
    for crop in [(1048575,1048575,0,0),(0,0,1048575,1048575),(500000,0,500000,0)]:add(f'crop-{crop}',sps(crop=crop))
    for layers in [1,2,7]:
        for profiles in [0,1,127]:
            for levels in [0,1,127]:add(f'layers-{layers}-{profiles}-{levels}',sps(layers=layers,profiles=profiles,levels=levels))
    for nals in [[],[b''],[b'',b''],[b'',prefix],[b'',b'',prefix],[prefix,b'']]:
        add_arrays('empty-'+str(list(map(len,nals))),[(33,nals)])
    add_arrays('empty-first-sps-array',[(33,[b'']),(33,[prefix])])
    add_arrays('no-arrays',[])
    # Real configuration and stream permutations test recovery, not just rejection.
    config=props[b'hvcC'];arrays=[];at=23
    for _ in range(config[22]):
        kind=config[at]&63;count=int.from_bytes(config[at+1:at+3],'big');at+=3;nals=[]
        for _ in range(count):
            size=int.from_bytes(config[at:at+2],'big');at+=2;nals.append(config[at:at+size]);at+=size
        arrays.append((kind,nals))
    dimensions=struct.unpack('>II',props[b'ispe'][4:12])
    add_arrays('real-baseline',arrays,dimensions=dimensions)
    add_arrays('real-empty-prefix',[(kind,[b'']+nals) for kind,nals in arrays],dimensions=dimensions)
    add_arrays('real-empty-array',[(33,[b''])]+arrays,dimensions=dimensions)
    for kind in [32,33,34]:
        add_arrays('real-without-'+str(kind),[(k,nals) for k,nals in arrays if k!=kind],dimensions=dimensions)
    for n in range(len(arrays)):
        ordered=arrays[n:]+arrays[:n]
        add_arrays('real-order-'+str(n),ordered,dimensions=dimensions)
    length_size=(config[21]&3)+1
    serialized=b''.join(len(nal).to_bytes(length_size,'big')+nal for _,nals in arrays for nal in nals)
    add_arrays('real-recover-after-slice',[],payload=top[b'mdat']+serialized+top[b'mdat'],dimensions=dimensions)
    work=sys.argv[sys.argv.index('--work')+1] if '--work' in sys.argv else '.build/hevc-limits'
    root=Path(work+'-inputs').resolve();root.mkdir(parents=True,exist_ok=True);paths=[]
    for name,data in cases:
        path=root/(name+'.heic');path.write_bytes(data);paths.append(str(path))
    test_decode.__doc__=__doc__
    test_decode.FIXTURES=paths
    if '--modes' not in sys.argv:sys.argv.extend(['--modes','0,6,10,11'])
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/hevc-limits-report.json'])
    if '--work' not in sys.argv:sys.argv.extend(['--work','.build/hevc-limits'])
    test_decode.main()
if __name__=='__main__':main()
