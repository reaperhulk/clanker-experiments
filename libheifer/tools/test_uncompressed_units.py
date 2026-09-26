#!/usr/bin/env python3
"""Full-byte uncompressed generic-compression units, malformed cmpC/icef and decompression errors."""
from pathlib import Path
import struct,sys,zlib
import brotli_fixtures
import test_uncompressed_pixels as runner
from test_uncompressed_pixels import cmpd,config
from test_context import box,full
from item_fixtures import item_file,ispe

def corpus():
    cases=[]
    pixels=bytes((i*37+11)&255 for i in range(16))
    def compress(data,kind):
        if kind==b'brot':return brotli_fixtures.compress(data)
        c=zlib.compressobj(wbits=15 if kind==b'zlib' else -15)
        return c.compress(data)+c.flush()
    def icef(units,offset_code=3,size_code=3,version=0,count=None):
        widths=[0,2,3,4,8];sizes=[1,2,3,4,8]
        return full(b'icef',bytes([(offset_code<<5)|(size_code<<2)])+struct.pack('>I',len(units) if count is None else count)+b''.join((off.to_bytes(widths[offset_code],'big') if offset_code else b'')+size.to_bytes(sizes[size_code],'big') for off,size in units),version)
    def add(name,data,props=(),tiled=False,required=False):
        file=item_file([dict(id=1,kind=b'unci',data=data,props=[ispe(4,4),cmpd([0]),config(cols=1 if tiled else 0,rows=1 if tiled else 0),*props])])
        cases.append((name,struct.pack('=II',len(file),2)+file,required))
    def cmpc(kind,unit=0,version=0):return full(b'cmpC',kind+bytes([unit]),version)
    for kind in [b'zlib',b'defl',b'brot']:
        data=compress(pixels,kind)
        for unit in range(256):add(f'unit-type-{kind}-{unit}',data,[cmpc(kind,unit)],required=unit<=4)
        for n in range(len(data)+1):add(f'compressed-prefix-{kind}-{n}',data[:n],[cmpc(kind)])
        for offset in range(len(data)):
            bad=bytearray(data);bad[offset]^=0x81;add(f'compressed-corrupt-{kind}-{offset}',bad,[cmpc(kind)])
        for tail in [b'',bytes(7),data]:add(f'compressed-trailing-{kind}-{len(tail)}',data+tail,[cmpc(kind)],tiled=True,required=True)
        chunks=[compress(pixels[i:i+4],kind) for i in range(0,16,4)]
        joined=b''.join(chunks);units=[];at=0
        for chunk in chunks:units.append((at,len(chunk)));at+=len(chunk)
        for unit_type in range(5):
            for offset_code in range(5):
                for size_code in range(5):add(f'index-{kind}-{unit_type}-{offset_code}-{size_code}',joined,[cmpc(kind,unit_type),icef(units,offset_code,size_code)],tiled=True,required=True)
        for unit_type in [0,2]:
            for table in [[],units[:1],units[:3],units+units,units[::-1],[(0,len(joined))],[(0,len(chunks[0]))]*4,[(999,1)],[(0,999)],[(0,0)],[(0,1)],[(0,len(data))]*8]:
                add(f'ranges-{kind}-{unit_type}-{table}',joined,[cmpc(kind,unit_type),icef(table)],tiled=True)
        for version in [1,2,255]:add(f'cmpc-version-{kind}-{version}',data,[cmpc(kind,version=version)]);add(f'icef-version-{kind}-{version}',data,[cmpc(kind),icef([(0,len(data))],version=version)])
    # Distinct component and tile bytes catch accidental reuse of unit zero and
    # full-item offsets on tile-local streams. Exercise every decoder layout.
    for kind in [b'zlib',b'defl',b'brot']:
        for layout in [0,1,2,3,4]:
            for types in [(0,), (4,5,6), (4,5,6,7)]:
                for tiled in [False,True]:
                    raw=bytes((i*37+11)&255 for i in range(16*len(types)))
                    cfg=config(components=[(i,8,0,0) for i in range(len(types))],interleave=layout,cols=int(tiled),rows=int(tiled))
                    def layout_case(label,payload,props):
                        file=item_file([dict(id=1,kind=b'unci',data=payload,props=[ispe(4,4),cmpd(types),cfg,*props])])
                        cases.append((f'layout-{kind}-{layout}-{types}-{tiled}-{label}',struct.pack('=II',len(file),2)+file,label.startswith('single-') and layout in (0,1,4)))
                    for unit_type in [0,1,2,3,4]:
                        layout_case(f'single-{unit_type}',compress(raw,kind),[cmpc(kind,unit_type)])
                        chunks=[compress(raw[i:i+4],kind) for i in range(0,len(raw),4)]
                        offsets=[];at=0
                        for chunk in chunks:offsets.append((at,len(chunk)));at+=len(chunk)
                        layout_case(f'indexed-{unit_type}',b''.join(chunks),[cmpc(kind,unit_type),icef(offsets)])
                        # Explicit unit offsets permit gaps and reordered storage.
                        chunks.reverse();at=3;reversed_offsets=[];payload=b'gap'
                        for chunk in chunks:
                            reversed_offsets.append((at,len(chunk)));payload+=chunk+b'gap';at+=len(chunk)+3
                        layout_case(f'reordered-{unit_type}',payload,[cmpc(kind,unit_type),icef(reversed_offsets[::-1])])
    for kind in [b'brot',b'zzzz',bytes(4),bytes([255])*4]:
        for unit in [0,2,4]:add(f'compression-{kind}-{unit}',pixels,[cmpc(kind,unit)])
    for prop in [cmpc(b'zlib'),icef([(0,16),(16,16)])]:
        for n in range(len(prop)-7):add(f'box-prefix-{prop[4:8]}-{n}',pixels,[box(prop[4:8],prop[8:8+n])])
    for code in range(256):add(f'icef-codes-{code}',pixels,[full(b'icef',bytes([code])+bytes(4))])
    for off in [0,1,2**32,2**64-2,2**64-1]:
        for size in [0,1,2,2**32,2**64-1]:add(f'icef-overflow-{off}-{size}',pixels,[icef([(off,size)],4,4)])
    for count in [0,1,2,16,2**32-1]:
        for n in range(10):add(f'icef-count-{count}-{n}',pixels,[full(b'icef',bytes([0])+struct.pack('>I',count)+bytes(n))])
    return cases

if __name__=='__main__':
    runner.corpus=corpus;runner.__doc__=__doc__
    if '--work' not in sys.argv:sys.argv.extend(['--work','.build/uncompressed-units-sanitized' if '--sanitize' in sys.argv else '.build/uncompressed-units'])
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/uncompressed-units-report.json'])
    runner.main()
