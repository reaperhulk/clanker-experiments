#!/usr/bin/env python3
"""Original-header OpenJPEG/Rust JPEG2000 decoded samples, metadata and conversion."""
import hashlib,json,sys
from pathlib import Path
import test_decode
from item_fixtures import item_file,ispe
from test_context import box

def main():
 if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg2000']
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-report.json']
 if '--modes' not in sys.argv:sys.argv+=['--modes',','.join(map(str,[*range(23),26,27]))]
 directory=Path(sys.argv[sys.argv.index('--work')+1]).resolve()/'fixtures';directory.mkdir(parents=True,exist_ok=True)
 test_decode.FIXTURES=[]
 for entry in json.loads(Path('tests/fixtures/jpeg2000-generated.json').read_text())['fixtures']:
  data=bytes.fromhex(entry['hex']);assert hashlib.sha256(data).hexdigest()==entry['sha256']
  path=directory/(entry['name']+'.heif');path.write_bytes(item_file([dict(id=1,kind=b'j2k1',data=data,props=[ispe(entry['width'],entry['height']),box(b'j2kH',b'')])]))
  test_decode.FIXTURES.append(str(path))
 test_decode.__doc__=__doc__;test_decode.main()
if __name__=='__main__':main()
