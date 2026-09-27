#!/usr/bin/env python3
"""Original-header JPEG2000 retained components, precision and partial codestream descriptions."""
import json,struct,sys
from pathlib import Path
import test_component_handles
from item_fixtures import item_file,ispe
from test_context import box

def corpus(descriptions_only=False):
 cases=[]
 for e in json.loads(Path('tests/fixtures/jpeg2000-generated.json').read_text())['fixtures']:
  data=bytes.fromhex(e['hex']);variants=[('full',data,True)]
  if e['name'] in ['17x19-3-8-0-0','7x5-1-8-0-0']:
   variants.extend((f'prefix-{i}',data[:i],True) for i in range(80))
   variants.append(('missing-property',data,False))
   for at in [8,12,16,20,24,28,32,36]:
    for value in [0,1,16,17,19,65535,0xffffffff]:
     broken=bytearray(data);broken[at:at+4]=value.to_bytes(4,'big');variants.append((f'siz-{at}-{value}',bytes(broken),True))

  for label,raw,prop in variants:
   payload=item_file([dict(id=1,kind=b'j2k1',data=raw,props=[ispe(e['width'],e['height'])]+([box(b'j2kH',b'')] if prop else []))])
   for mode in [0,2]:cases.append((f'{e["name"]}-{label}-{mode}',struct.pack('=II',len(payload),0 if descriptions_only else mode)+payload))
 return cases,b''.join(data for _,data in cases)
if __name__=='__main__':
 test_component_handles.corpus=corpus;test_component_handles.__doc__=__doc__
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-handles-report.json']
 test_component_handles.main()
