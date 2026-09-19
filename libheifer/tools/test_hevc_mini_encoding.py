#!/usr/bin/env python3
"""Original-header minimized HEVC encoding, NAL configuration and exact repeated writes."""
import struct
import sys
import test_writing
from test_hevc_encoding import corpus as encoder_corpus

def corpus():
    tests=[]
    for name,data in encoder_corpus()[0]:
        v=list(struct.unpack('=12I',data[:48]));v[10]|=64
        tests.append((name,struct.pack('=12I',*v)+data[48:]))
    return tests,b''.join(data for _,data in tests)

if __name__=='__main__':
    test_writing.corpus=corpus
    test_writing.CLIENT='tests/plugin_encoding.c'
    test_writing.__doc__=__doc__
    if '--work' not in sys.argv:sys.argv+=['--work','.build/hevc-mini-encoding']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/hevc-mini-encoding-report.json']
    test_writing.main()
