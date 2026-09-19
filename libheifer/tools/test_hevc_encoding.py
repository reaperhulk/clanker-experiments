#!/usr/bin/env python3
"""Original-header HEVC encoder NAL configuration, callbacks, alpha and exact files."""
import itertools
from pathlib import Path
from test_decode_geometry import children
import struct
import sys
import test_writing
from test_hevc_limits import sps
from test_plugin_encoding import corpus as av1_corpus

def packets(nals):
    return b''.join(struct.pack('=I',len(n))+n for n in nals)

def corpus():
    cases=[]
    def add(name,nals=None,version=3,cs=0,ch=1,depth=8,padding=0,flags=0,orientation=1,failure=0,chunks=1,target=99,target_ch=99):
        nals=nals if nals is not None else [b'\x40\x01\x02',sps(7+padding,5+padding),b'\x44\x01\x03',b'\x26\x01\x04']
        packet=packets(nals);v=[version,cs,ch,depth,orientation,target,target_ch,padding,failure,chunks,flags|262144,len(packet)]
        cases.append((name,struct.pack('=12I',*(x&0xffffffff for x in v))+packet))
    for name,data in av1_corpus()[0]:
        v=struct.unpack('=12I',data[:48])
        if name.startswith(('packet-','size-','encoded-size-boundary-')):continue
        padding=v[7] if v[7]<2147483648 else v[7]-4294967296
        add(name,version=v[0],cs=v[1],ch=v[2],depth=v[3],padding=padding,flags=v[10],orientation=v[4],failure=v[8],chunks=v[9],target=v[5],target_ch=v[6])
    for chroma,luma,color in itertools.product(range(4),[0,2,4,8],[0,2,4,8]):
        add(f'configuration-{chroma}-{luma}-{color}',[sps(7,5,chroma=chroma,luma=luma,color=color),b'\x26\x01'])
    for layers,profiles,levels in itertools.product(range(8),[0,1,127],[0,1,127]):
        add(f'layers-{layers}-{profiles}-{levels}',[sps(7,5,layers=layers,profiles=profiles,levels=levels),b'\x26\x01'])
    for chroma,crop in itertools.product(range(4),[(0,0,0,0),(1,1,0,0),(0,0,1,1),(1,1,1,1)]):
        sx=2 if chroma in (1,2) else 1;sy=2 if chroma==1 else 1
        add(f'crop-{chroma}-{crop}',[sps(7+sx*(crop[0]+crop[1]),5+sy*(crop[2]+crop[3]),chroma=chroma,crop=crop),b'\x26\x01'])
    for profile,flags in itertools.product(range(32),[0,0x40000000,0x10000000,0x20000000,0x08000000]):
        n=bytearray(sps(7,5));n[3]=profile;n[4:8]=flags.to_bytes(4,'big')
        add(f'profile-{profile}-{flags}',[bytes(n),b'\x26\x01'])
    for padding in [121,122,32761,32762,65536,65537,100000]:add(f'padding-{padding}',padding=padding)
    for typ,tail in itertools.product([32,33,34], [b'',b'\0',b'\0\0',b'\x01']):
        n=sps(7,5) if typ==33 else bytes([typ<<1,1,42])
        for reverse in [False,True]:
            seq=[n,n+tail];seq=seq[::-1] if reverse else seq
            add(f'duplicate-{typ}-{tail.hex()}-{reverse}',[sps(7,5),*seq,b'\x26\x01'])
    for kind in [0,1,19,32,34,35,39,40,63,64,127]:
        add(f'nal-{kind}',[sps(7,5),bytes([kind<<1,1,2,3])])
    for nals in [[],[b'\x26\x01'],[b'\x40\x01'],[b'\x44\x01']]:add('missing-sps-'+str(len(cases)),nals)
    # A preceding valid SPS initializes every native configuration member.
    # An initial malformed SPS can expose uninitialized native fields and is not parity evidence.
    prior=sps(9,7,chroma=3,luma=4,color=4)
    for n in range(1,len(sps(7,5))):
        add(f'partial-sps-{n}',[prior,sps(7,5)[:n],b'\x26\x01'])
    for nal in [sps(7,5,chroma=4),sps(7,5,luma=9),sps(7,5,color=9),sps(7,5,crop=(100,0,0,0))]:
        add(f'invalid-followup-sps-{len(cases)}',[prior,nal,b'\x26\x01'])
    for size in [65535,65536]:
        for kind in [32,33,34]:
            nal=sps(7,5) if kind==33 else bytes([kind<<1,1])
            add(f'configuration-size-{size}-{kind}',([] if kind==33 else [sps(7,5)])+[nal+bytes(size-len(nal)),b'\x26\x01'])
    for size in [7,1024,1025,32763,32764,32765]:
        for flags in [0,4|2048,256,4096,4096|8192]:
            add(f'payload-size-{size}-{flags}',[sps(7,5),b'\x26\x01'+bytes(size-2)],flags=flags)
    for fixture in ['colors-no-alpha.heic','colors-with-alpha.heic']:
        top=dict(children(Path('tests/upstream/fuzzing/data/corpus',fixture).read_bytes()))
        meta=dict(children(top[b'meta'][4:]));iprp=dict(children(meta[b'iprp']))
        for index,(kind,config) in enumerate(children(iprp[b'ipco'])):
            if kind!=b'hvcC':continue
            at=23;nals=[]
            for _ in range(config[22]):
                count=int.from_bytes(config[at+1:at+3],'big');at+=3
                for _ in range(count):
                    n=int.from_bytes(config[at:at+2],'big');at+=2
                    nals.append(config[at:at+n]);at+=n
            for flags in [0,4096,4096|8192,1024|3]:
                add(f'native-configuration-{fixture}-{index}-{flags}',nals+[b'\x26\x01'],version=2,flags=flags)
    return cases,b''.join(data for _,data in cases)

if __name__=='__main__':
    test_writing.corpus=corpus
    test_writing.CLIENT='tests/plugin_encoding.c'
    test_writing.__doc__=__doc__
    if '--work' not in sys.argv:sys.argv+=['--work','.build/hevc-encoding']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/hevc-encoding-report.json']
    test_writing.main()
