#!/usr/bin/env python3
"""Original-header exact compact AV1 encoding, callbacks and repeated file writes."""
import struct
import sys
import test_writing
from test_plugin_encoding import corpus as encoder_corpus

def corpus():
    tests = []
    for name,data in encoder_corpus()[0]:
        fields=list(struct.unpack('=12I',data[:48]));fields[10]|=64
        tests.append((name,struct.pack('=12I',*fields)+data[48:]))
    return tests,b''.join(data for _,data in tests)

if __name__=='__main__':
    test_writing.corpus=corpus
    test_writing.CLIENT='tests/plugin_encoding.c'
    test_writing.__doc__=__doc__
    if '--work' not in sys.argv:sys.argv+=['--work','.build/mini-encoding']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/mini-encoding-report.json']
    test_writing.main()
