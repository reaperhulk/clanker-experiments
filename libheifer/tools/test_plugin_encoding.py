#!/usr/bin/env python3
"""Original-header registered AV1 encoder callbacks, pixels, errors and exact files."""
import itertools
import json
from pathlib import Path
import struct
import sys
import test_writing

def corpus():
    cases = []
    def add(name, version=3, cs=0, ch=1, depth=8, orientation=1, target=99,
            target_ch=99, padding=0, failure=0, chunks=1, flags=0, packet=b'\x12\0'):
        v=[version,cs,ch,depth,orientation,target,target_ch,padding,failure,chunks,flags,len(packet)]
        cases.append((name,struct.pack('=12I',*(x&0xffffffff for x in v))+packet))
    for version, (cs,ch), depth in itertools.product([1,2,3,4,5], [(2,0),(0,1),(0,2),(0,3),(1,3),(1,10)], [8,10,12]):
        if ch==10 and depth!=8: continue
        add(f'layout-{version}-{cs}-{ch}-{depth}',version=version,cs=cs,ch=ch,depth=depth)
    for flags in range(64): add(f'color-{flags}',flags=flags)
    for orientation,padding in itertools.product(range(1,9),[-1,0,1,2]):
        add(f'orientation-{orientation}-{padding}',orientation=orientation,padding=padding)
    for padding in [121,122,32761,32762,65536,65537,2147483647,2147483648,2147483649,4294967295]:
        add(f'encoded-size-boundary-{padding}',padding=padding)
    for version,failure,chunks in itertools.product([1,2,3],[0,1,2],[0,1,3]):
        add(f'callbacks-{version}-{failure}-{chunks}',version=version,failure=failure,chunks=chunks)
    for source,target in itertools.product([(2,0),(0,1),(0,2),(0,3),(1,3),(1,10)],repeat=2):
        add(f'conversion-{source}-{target}',cs=source[0],ch=source[1],target=target[0],target_ch=target[1])
    for flags in [128,256,384,512|256,512|384,1024,1024|4|3|8,4096,4096|8192,4096|16384,4096|32768,4096|65536]:
        for (cs,ch),target in itertools.product([(2,0),(0,1),(1,3),(1,11)],[99,0]):
            add(f'aux-metadata-{flags}-{cs}-{ch}-{target}',flags=flags,cs=cs,ch=ch,target=target,target_ch=1)
    for version,flags in itertools.product([1,2,3,4],[4096,4096|8192,131072,131072|4096,131072|3|1024]):
        add(f'alpha-parameters-thumbnail-{version}-{flags}',version=version,flags=flags,cs=1,ch=11)
    for chroma,depth in itertools.product([11,13,15],[8,10,12]):
        if chroma in (13,15) and depth==8: continue  # Plane construction rejects this layout.
        add(f'packed-alpha-{chroma}-{depth}',cs=1,ch=chroma,depth=depth,flags=4096)
    for size in [0,1,7,1024,1025,32767,32768,32769]:
        for flags in [0,4|2048,256]:
            add(f'size-{size}-{flags}',packet=b'\x12\0'+bytes(max(0,size-2)) if size>=2 else bytes(size),flags=flags)
    for entry in json.loads(Path('tests/fixtures/av1-generated.json').read_text())['fixtures']:
        data=bytes.fromhex(entry['hex']);at=0
        while data[at+4:at+8]!=b'mdat':at+=int.from_bytes(data[at:at+4],'big')
        packet=data[at+8:at+int.from_bytes(data[at:at+4],'big')]
        for prefix in [b'',b'\x12\0',b'\x7a\x01\0']:
            add(f'packet-{entry["name"]}-{prefix.hex()}',packet=prefix+packet)
    return cases,b''.join(data for _,data in cases)

if __name__=='__main__':
    test_writing.corpus=corpus
    test_writing.CLIENT='tests/plugin_encoding.c'
    test_writing.__doc__=__doc__
    if '--work' not in sys.argv:sys.argv+=['--work','.build/plugin-encoding']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/plugin-encoding-report.json']
    test_writing.main()
