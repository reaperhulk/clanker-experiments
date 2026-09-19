#!/usr/bin/env python3
"""Original-header AVC encoder SPS/PPS/ext packets, callbacks, alpha and exact files."""
import itertools
import struct
import sys
import test_writing
from test_plugin_encoding import corpus as av1_corpus
from test_hevc_encoding import packets
from test_hevc_limits import ue

EXTENDED={100,110,122,244,44,83,86,118,128,138,139,134,135}
def se(n):return ue(2*n-1 if n>0 else -2*n)
def sps(profile=66,chroma=1,depth=8,frame=1,crop=None,poc=0,scaling=None,mbs=(1,1),compat=0,level=30):
    bits=ue(0)
    if profile in EXTENDED:
        bits+=ue(chroma)+('0' if chroma==3 else '')+ue(depth-8)*2+'0'+str(int(scaling is not None))
        if scaling is not None:
            for i in range(12 if chroma==3 else 8):
                present=(i%3)==0;bits+=str(int(present))
                if present:bits+=se(scaling)*(1 if scaling==-8 else (16 if i<6 else 64))
    bits+=ue(0)+ue(poc)
    if poc==0:bits+=ue(0)
    elif poc==1:bits+='0'+se(-1)+se(1)+ue(3)+ue(0)+ue(1)+ue(2)
    bits+=ue(0)+'0'+ue(mbs[0]-1)+ue(mbs[1]-1)+str(frame)+('0' if not frame else '')+'1'
    bits+=('1'+''.join(map(ue,crop))) if crop else '0'
    bits+='0'*(-len(bits)%8)
    return bytes([0x67,profile,compat,level])+int(bits,2).to_bytes(len(bits)//8,'big')

def corpus():
    tests=[]
    def add(name,nals=None,fields=None,compact=0):
        nals=nals if nals is not None else [sps(),b'\x68\x01',b'\x65\x42']
        data=packets(nals);v=list(fields or [3,0,1,8,1,99,99,0,0,1,0,0]);v[10]|=(2<<20)|262144|compact;v[11]=len(data)
        tests.append((name,struct.pack('=12I',*v)+data))
    for name,data in av1_corpus()[0]:
        if name.startswith(('packet-','size-','encoded-size-boundary-')):continue
        v=struct.unpack('=12I',data[:48])
        for compact in [0,64]:add(f'{compact}-{name}',fields=v,compact=compact)
    for profile,chroma,depth,frame in itertools.product([66,77,88,100,110,122,244,44,83,86,118,128,138,139,134,135],range(4),[8,10,12],[0,1]):
        add(f'profile-{profile}-{chroma}-{depth}-{frame}',[sps(profile,chroma,depth,frame),b'\x68\x11',b'\x6d\x12',b'\x65\x13'])
    for chroma,frame,poc,scaling in itertools.product(range(4),[0,1],[0,1,2],[None,0,1,-8]):
        add(f'crop-poc-scaling-{chroma}-{frame}-{poc}-{scaling}',[sps(100,chroma,8,frame,(1,1,1,1),poc,scaling),b'\x65\x01'])
    for count in [1,2,30,31,32]:add(f'duplicate-sps-{count}',[sps()]*count+[b'\x65\x01'])
    for count in [0,1,2,254,255,256]:
        add(f'duplicate-pps-{count}',[sps()]+[b'\x68\x01']*count+[b'\x65\x01'])
        add(f'duplicate-ext-{count}',[sps(100)]+[b'\x6d\x01']*count+[b'\x65\x01'])
    for size in [65535,65536]:
        for kind in [7,8,13]:
            nal=sps(100) if kind==7 else bytes([kind|0x60,1])
            for flags in [0,128,256,4096,4096|8192,131072]:
                fields=[3,0,1,8,1,99,99,0,0,1,flags,0]
                add(f'nal-size-{size}-{kind}-{flags}',([] if kind==7 else [sps(100)])+[nal+bytes(size-len(nal)),b'\x65\x01'],fields=fields)
    for profile in [66,100]:
        nal=sps(profile=profile)
        for prefix in range(1,len(nal)):
            for prior in [[],[sps(100,3,12)]]:
                add(f'truncated-sps-{profile}-{prefix}-{bool(prior)}',prior+[nal[:prefix],b'\x65\x01'])
    for chroma in [0,1,2,3]:
        for crop in [(0,0,0,0),(8,8,0,0),(0,0,8,8),(1048575,1048575,1048575,1048575)]:
            add(f'crop-boundary-{chroma}-{crop}',[sps(100,chroma,crop=crop),b'\x65\x01'])
    for kind in range(32):
        if kind in (7,8,13):continue
        add(f'nal-type-{kind}',[sps(),bytes([kind|0x60,1,2,3])])
    for flags,level in itertools.product([0,1,128,255],[0,1,30,255]):add(f'compat-level-{flags}-{level}',[sps(compat=flags,level=level),b'\x65\x01'])
    for nals in [[],[b'\x68\x01'],[b'\x6d\x01'],[b'\x65\x01']]:add(f'missing-sps-{len(tests)}',nals)
    return tests,b''.join(data for _,data in tests)

if __name__=='__main__':
    test_writing.corpus=corpus
    test_writing.CLIENT='tests/plugin_encoding.c'
    test_writing.__doc__=__doc__
    if '--work' not in sys.argv:sys.argv+=['--work','.build/avc-encoding']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/avc-encoding-report.json']
    test_writing.main()
