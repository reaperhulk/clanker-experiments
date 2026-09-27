#!/usr/bin/env python3
"""Original-header JPEG2000 allocation and dimension boundaries at read and decode."""
import json,struct,sys
from pathlib import Path
import test_security
from item_fixtures import item_file,ispe
from test_context import box

def corpus():
 cases=[]
 for e in json.loads(Path('tests/fixtures/jpeg2000-generated.json').read_text())['fixtures']:
  if e['name'] not in ['17x19-3-8-0-0','7x5-1-8-0-0','sampling-2x2-12-False']:continue
  w,h=e['width'],e['height'];raw=bytes.fromhex(e['hex']);data=item_file([dict(id=1,kind=b'j2k1',data=raw,props=[ispe(w,h),box(b'j2kH',b'')])]);estimated=sum(((w+dx-1)//dx)*((h+dy-1)//dy)*12 for dx,dy in e['sampling'])
  for field,values in {0:[0,1,w*h-1,w*h,w*h+1],5:[0,1,len(raw)-1,len(raw),estimated-1,estimated,estimated+1,4096,16384],10:[0,1,1023,1024,2048,4096,16384]}.items():
   for value in values:
    for phase in [0,1]:cases.append((f'{e["name"]}/{field}/{phase}/{value}',struct.pack('=IIIQ',len(data),field,phase,value)+data))
 return cases
if __name__=='__main__':
 test_security.corpus=corpus;test_security.__doc__=__doc__
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-limits-report.json']
 test_security.main()
