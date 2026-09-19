#!/usr/bin/env python3
"""Compressed image metadata, skipped unsupported encodings, errors and handle ownership."""
import sys,zlib
import test_context
from test_context import box,synthetic

def rewrite(data,encoding):
    def children(data,prefix=0):
        out=data[:prefix];at=prefix
        while at<len(data):
            size=int.from_bytes(data[at:at+4],'big');kind=data[at+4:at+8];body=data[at+8:at+size]
            if kind==b'meta':body=children(body,4)
            elif kind==b'iinf':body=children(body,6)
            elif kind==b'infe' and body[4:6]==b'\0\3':body+=encoding+b'\0'
            out+=box(kind,body);at+=size
        return out
    return children(data)

def corpus(_source):
    cases=[]
    for encoding,window in [(b'compress_zlib',15),(b'deflate',-15),(b'identity',None),(b'',None),(b'unknown',None),(b'br',None),(b'gzip',31)]:
        for data in [b'',b'x',b'ABC\0DEF',bytes(range(256)),bytes(8192),b'abcde'*6000]:
            if window:
                c=zlib.compressobj(wbits=window);payload=c.compress(data)+c.flush()
            else:payload=data
            for length in sorted(set([0,1,min(2,len(payload)),len(payload)//2,len(payload)-1,len(payload)])):
                if length<0:continue
                base=synthetic(kind=b'mime',metadata=payload[:length])
                cases.append((f'{encoding!r}-{len(data)}-{length}',rewrite(base,encoding)))
    return cases
if __name__=='__main__':
    test_context.corpus=corpus;test_context.SCOPE=__doc__
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/metadata-compression-report.json'])
    test_context.main()
