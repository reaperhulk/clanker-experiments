#!/usr/bin/env python3
"""Original-header VVC encoder configuration, callbacks, alpha and exact serialized files."""
import itertools
import struct
import sys
import test_writing
from test_plugin_encoding import corpus as av1_corpus
from test_hevc_encoding import packets
from test_hevc_limits import ue
from test_vvc_builtin_encoding import fallback as vvc_fallback

def sps(layers=1,chroma=1,depth=8,profile=1,tier=0,level=30,frame=1,multi=0,flags=0,subprofiles=(),size=(7,5),crop=None,ptl=True,gci=False,subpic=False,resample=False):
    bits='00000000'+f'{layers-1:03b}{chroma:02b}00'+str(int(ptl))
    if ptl:
        bits+=f'{profile:07b}{tier:01b}{level:08b}{frame:01b}{multi:01b}'+str(int(gci))
        bits+='0'*(-len(bits)%8)
        bits+=''.join(str((flags>>i)&1) for i in reversed(range(layers-1)))
        bits+='0'*(-len(bits)%8)
        bits+=''.join(f'{(level+i+1)&255:08b}' for i in reversed(range(layers-1)) if flags>>i&1)
        bits+=f'{len(subprofiles):08b}'+''.join(f'{s:032b}' for s in subprofiles)
    bits+='0'+str(int(resample))+('1' if resample else '')
    bits+=ue(size[0])+ue(size[1])
    bits+=('1'+''.join(map(ue,crop))) if crop else '0'
    bits+=str(int(subpic))+ue(depth-8)
    bits+='0'*(-len(bits)%8)
    data=b'\x00\x79'+int(bits,2).to_bytes(len(bits)//8,'big')
    # Prevent start-code emulation in all generated variable-width fields.
    out=bytearray();zeros=0
    for byte in data:
        if zeros>=2 and byte<=3:out.append(3);zeros=0
        out.append(byte);zeros=zeros+1 if byte==0 else 0
    return bytes(out)

def corpus():
    cases=[]
    def add(name,nals=None,fields=None,compact=0):
        if nals is None:nals=[b'\x00\x71\x02',sps(),b'\x00\x81\x03',b'\x00\x09\x04']
        data=packets(nals);v=list(fields or [3,0,1,8,1,99,99,0,0,1,0,0]);v[10]|=(5<<20)|262144|compact;v[11]=len(data)
        cases.append((name,struct.pack('=12I',*v)+data))
    for name,data in av1_corpus()[0]:
        if name.startswith('packet-'):continue
        for compact in [0,64]:add(f'{compact}-{name}',fields=struct.unpack('=12I',data[:48]),compact=compact)
    for layers,chroma,depth,flags in itertools.product(range(1,8),range(4),[8,10,12,15],[0,1,0x55,0x7f]):
        add(f'config-{layers}-{chroma}-{depth}-{flags}',[sps(layers,chroma,depth,flags=flags),b'\x00\x09'])
    for profile,tier,level,frame,multi in itertools.product([0,1,2,127],[0,1],[0,1,30,255],[0,1],[0,1]):
        add(f'ptl-{profile}-{tier}-{level}-{frame}-{multi}',[sps(profile=profile,tier=tier,level=level,frame=frame,multi=multi),b'\x00\x09'])
    for profiles in [(),(0,),(1,),(0xffffffff,),(0,1,0x12345678),tuple(range(255))]:
        add(f'subprofiles-{len(profiles)}-{profiles[:3]}',[sps(subprofiles=profiles),b'\x00\x09'])
    for chroma,crop in itertools.product(range(4),[(0,0,0,0),(1,1,1,1),(100,0,0,0),(1048575,)*4]):
        add(f'crop-{chroma}-{crop}',[sps(chroma=chroma,crop=crop),b'\x00\x09'])
    for size in [(0,0),(1,1),(65535,65535),(65536,1),(1,65536)]:
        add(f'sps-size-{size}',[sps(size=size),b'\x00\x09'])
    for kwargs in [dict(ptl=False),dict(gci=True),dict(subpic=True),dict(resample=True),dict(depth=256)]:
        add(f'partial-fields-{kwargs}',[sps(**kwargs),b'\x00\x09'])
    nal=sps()
    for n in range(2,len(nal)):
        # All initial partial records here have at most one sublayer. Native
        # missing multi-layer vector accesses are undefined and excluded.
        for prior in [[],[sps(chroma=3,depth=12)]]:add(f'truncation-{n}-{bool(prior)}',prior+[nal[:n],b'\x00\x09'])
    for kind in range(32):
        if kind==15:continue
        add(f'packet-kind-{kind}',[sps(),bytes([0,kind<<3|1,42])])
    for nals in [[],[b'\x00\x71'],[b'\x00\x81'],[b'\x00\x09'],[b''],[b'\x01']]:add(f'missing-sps-{len(cases)}',nals)
    for kind,count in itertools.product([14,15,16],[1,2,255,256,65535,65536]):
        n=sps() if kind==15 else bytes([0,kind<<3|1,42])
        add(f'duplicate-{kind}-{count}',[sps()]+[n]*count+[b'\x00\x09'])
    for kind,size,flags in itertools.product([14,15,16],[65535,65536],[0,128,256,4096,4096|8192,131072]):
        n=sps() if kind==15 else bytes([0,kind<<3|1])
        add(f'nal-size-{kind}-{size}-{flags}',[sps(),n+bytes(size-len(n)),b'\x00\x09'],fields=[3,0,1,8,1,99,99,0,0,1,flags,0])
    return cases,b''.join(data for _,data in cases)

if __name__=='__main__':
    test_writing.corpus=corpus
    test_writing.CLIENT='tests/plugin_encoding.c'
    # Unsupported plugin versions fall back to the default VVC encoder.
    test_writing.known=lambda x,y:vvc_fallback(x,y) if b'e5,2003,Unsupported plugin version' in y else None
    test_writing.__doc__=__doc__
    if '--work' not in sys.argv:sys.argv+=['--work','.build/vvc-encoding']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/vvc-encoding-report.json']
    test_writing.main()
