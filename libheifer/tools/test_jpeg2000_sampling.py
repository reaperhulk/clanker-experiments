#!/usr/bin/env python3
"""Independent common-component sample grid and oversized packet boundaries."""
import sys
import test_jpeg2000_tiles as tiles

def corpus():
 base=tiles.entries('tests/fixtures/jpeg2000-generated.json');cases=[]
 for mct in [0,1]:
  for dx,dy in [(1,1),(2,1),(1,2),(2,2),(4,4)]:
   data=bytearray(base[f'coding-LRCP-True-{mct}-0'])
   for c in range(3):data[43+3*c:45+3*c]=bytes([dx,dy])
   cases.append((f'sampling-{mct}-{dx}-{dy}',32,32,bytes(data)))
 return cases
if __name__=='__main__':
 if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg2000-sampling']
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-sampling-report.json']
 tiles.corpus=corpus;tiles.main()
