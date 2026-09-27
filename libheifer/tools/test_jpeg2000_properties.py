#!/usr/bin/env python3
"""Original-header JPEG2000 channel, palette and nested property validation."""
import itertools,struct,sys
import test_properties
from test_jpeg2000_tiles import entries
from item_fixtures import item_file,ispe
from test_context import box,full

def corpus():
 records=[]
 def add(name,kind,data):records.append((name,box(kind,data)))
 for kind,data in [(b'cdef',struct.pack('>7H',2,0,0,1,1,0,2)),(b'cmap',bytes.fromhex('0000000000010102')),(b'j2kL',bytes.fromhex('000200010200030004050006')),(b'pclr',bytes.fromhex('000203080f010002030004'))]:
  for n in range(len(data)+1):add(f'{kind.decode()}-prefix-{n}',kind,data[:n])
  add(f'{kind.decode()}-trailing',kind,data+b'xyz')
 for kind,count,length in itertools.product([b'cdef',b'j2kL'],[0,1,2,3,64,65,65535],[0,5,6,12]):add(f'{kind.decode()}-{count}-{length}',kind,struct.pack('>H',count)+bytes(length))
 for count,depths,length in itertools.product([0,1,2,65535],[[],[0],[7],[8],[9],[15],[16],[17],[127],[128],[255],[7,15]],[0,1,4]):add(f'palette-{count}-{depths}-{length}',b'pclr',struct.pack('>HB',count,len(depths))+bytes(depths)+bytes(length))
 child=box(b'cdef',struct.pack('>4H',1,0,0,1))
 for n in range(1,9):records.append((f'child-header-{n}',child[:n]))
 records.append(('unknown',box(b'zzzz',b'123')))
 cases=[];coded=entries('tests/fixtures/jpeg2000-generated.json')['1x1-1-8-0-0']
 for (name,record),nesting,codec in itertools.product(records,[0,1,2],[False,True]):
  prop=record
  for _ in range(nesting):prop=box(b'j2kH',prop)
  props=[ispe(1,1)]+([box(b'j2kH',b'')] if codec and nesting==0 else [])+([] if codec else [full(b'mskC',bytes([8]))])+[prop]
  cases.append((f'{name}/{nesting}/{codec}',item_file([dict(id=1,kind=b'j2k1' if codec else b'mski',props=props,data=coded if codec else b'\0')])))
 return cases,b''.join(struct.pack('=I',len(data))+data for _,data in cases)
if __name__=='__main__':
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-properties-report.json']
 test_properties.corpus=corpus;test_properties.__doc__=__doc__;test_properties.main()
