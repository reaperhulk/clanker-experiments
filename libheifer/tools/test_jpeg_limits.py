#!/usr/bin/env python3
"""Original-header JPEG allocation and dimension limit boundaries at read and decode."""
import json
from pathlib import Path
import struct
import sys
import test_security
from item_fixtures import item_file,ispe

def corpus():
    cases=[]
    for entry in json.loads(Path('tests/fixtures/jpeg-generated.json').read_text())['fixtures']:
        if entry['name'] not in ['17x19-2x2-75-0.jpg','17x19-gray-75-0.jpg','3x9-2x2-75-0.jpg']:continue
        w,h=entry['width'],entry['height'];jpeg=bytes.fromhex(entry['hex'])
        data=item_file([dict(id=1,kind=b'jpeg',data=jpeg,props=[ispe(w,h)])])
        estimated=w*h*(3 if entry['sampling']=='gray' else 9)
        for field,values in {0:[0,1,w*h-1,w*h,w*h+1],5:[0,1,len(jpeg)-1,len(jpeg),estimated-1,estimated,estimated+1,4096,16384],10:[0,1,1023,1024,2048,4096,16384]}.items():
            for value in values:
                for phase in [0,1]:cases.append((f'{entry["name"]}/{field}/{phase}/{value}',struct.pack('=IIIQ',len(data),field,phase,value)+data))
    return cases

if __name__=='__main__':
    test_security.corpus=corpus
    test_security.__doc__=__doc__
    if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg-limits-report.json']
    test_security.main()
