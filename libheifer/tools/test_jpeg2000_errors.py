#!/usr/bin/env python3
"""Independent JPEG2000 complete byte prefixes and malformed marker fields."""
import hashlib,json,sys
from pathlib import Path
import test_decode
from item_fixtures import item_file,ispe
from test_context import box

def main():
 if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg2000-errors']
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-errors-report.json']
 if '--modes' not in sys.argv:sys.argv+=['--modes','0,11,27']
 directory=Path(sys.argv[sys.argv.index('--work')+1]).resolve()/'fixtures';directory.mkdir(parents=True,exist_ok=True);test_decode.FIXTURES=[]
 for entry in json.loads(Path('tests/fixtures/jpeg2000-generated.json').read_text())['fixtures']:
  if entry['name'] not in ['17x19-3-8-0-0','17x19-3-12-0-1','7x5-1-8-0-0']:continue
  data=bytes.fromhex(entry['hex']);assert hashlib.sha256(data).hexdigest()==entry['sha256']
  cases=[(f'prefix-{i}',data[:i]) for i in range(len(data)+1)]
  for at in [0,1,2,3,4,5,40,41,42,43,44,45]:
   for value in [0,1,2,7,15,31,127,128,255]:
    broken=bytearray(data);broken[at]=value;cases.append((f'field-{at}-{value}',bytes(broken)))
  sot=data.index(b'\xff\x90')
  for delta,width in [(2,2),(4,2),(6,4),(10,1),(11,1)]:
   for value in [0,1,2,7,13,14,255,65535,0xffffffff]:
    if value >= 1<<(8*width):continue
    broken=bytearray(data);broken[sot+delta:sot+delta+width]=value.to_bytes(width,'big');cases.append((f'tile-{delta}-{value}',bytes(broken)))
  for label,payload in cases:
   path=directory/(entry['name']+'-'+label+'.heif');path.write_bytes(item_file([dict(id=1,kind=b'j2k1',data=payload,props=[ispe(entry['width'],entry['height']),box(b'j2kH',b'')])]))
   test_decode.FIXTURES.append(str(path))
 test_decode.__doc__=__doc__;test_decode.main()
if __name__=='__main__':main()
