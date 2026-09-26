#!/usr/bin/env python3
"""Original-header minimized HEVC encoding, NAL configuration and exact repeated writes."""
import struct
import sys
import test_writing
from test_hevc_builtin_encoding import fallback as hevc_fallback
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
    # A refused plugin falls back to the default HEVC encoder: x265 in the
    # oracle (.build/reference-x265), hpvca in the candidate.
    test_writing.known=lambda x,y:hevc_fallback(x,y) if b'e5,2003,Unsupported plugin version' in y else None
    test_writing.main()
