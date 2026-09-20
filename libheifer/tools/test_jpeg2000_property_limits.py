#!/usr/bin/env python3
"""Independent JPEG2000 channel and nested child-count allocation limits."""
import itertools,struct,sys
import test_security
from item_fixtures import item_file,ispe
from test_context import box,full

def corpus():
 cases=[]
 for count,kind,nesting in itertools.product([0,1,2,63,64,65,257],['channels','children'],[1,2]):
  prop=box(b'cdef',struct.pack('>H',count)+bytes(6*count)) if kind=='channels' else box(b'zzzz',b'')*count
  for _ in range(nesting):prop=box(b'j2kH',prop)
  data=item_file([dict(id=1,kind=b'mski',data=b'\0',props=[ispe(1,1),full(b'mskC',bytes([8])),prop])])
  field=6 if kind=='channels' else 9
  for value,phase in itertools.product(sorted(set([0,1,2,5,16,63,64,65,256,257,max(0,count-1),count,count+1])),[0,1]):
   cases.append((f'{kind}-{count}-{nesting}-{value}-{phase}',struct.pack('=IIIQ',len(data),field,phase,value)+data))
 return cases
if __name__=='__main__':
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-property-limits-report.json']
 test_security.corpus=corpus;test_security.__doc__=__doc__;test_security.main()
