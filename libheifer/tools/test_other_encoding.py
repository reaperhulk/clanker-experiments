#!/usr/bin/env python3
"""Original-header JPEG, JPEG2000 and HTJ2K encoder callbacks and exact container output."""
import struct
import sys
import test_writing
from test_plugin_encoding import corpus as av1_corpus

def corpus():
    tests=[]
    for fmt in [3,7,10]:
        for name,data in av1_corpus()[0]:
            if name.startswith('packet-'):continue
            for compact in [0,64]:
                v=list(struct.unpack('=12I',data[:48]));v[10]|=(fmt<<20)|compact
                tests.append((f'{fmt}-{compact}-{name}',struct.pack('=12I',*v)+data[48:]))
        for failure in [3,4]:
            v=[3,0,1,8,1,99,99,0,failure,3,fmt<<20,2]
            tests.append((f'{fmt}-formatted-error-{failure}',struct.pack('=12I',*v)+b'\x12\0'))
    return tests,b''.join(data for _,data in tests)

if __name__=='__main__':
    test_writing.corpus=corpus
    test_writing.CLIENT='tests/plugin_encoding.c'
    test_writing.__doc__=__doc__
    if '--work' not in sys.argv:sys.argv+=['--work','.build/other-encoding']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/other-encoding-report.json']
    test_writing.main()
