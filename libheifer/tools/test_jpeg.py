#!/usr/bin/env python3
"""Original-header native JPEG/Rust JPEG decoded pixels, metadata, conversions and callbacks."""
import hashlib
import json
from pathlib import Path
import sys
import test_decode
from item_fixtures import item_file,ispe
from test_context import box

def main():
    if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg-report.json']
    if '--modes' not in sys.argv:sys.argv+=['--modes',','.join(map(str,[*range(23),26,27]))]
    directory=Path(sys.argv[sys.argv.index('--work')+1]).resolve()/'fixtures'
    directory.mkdir(parents=True,exist_ok=True)
    test_decode.FIXTURES=[]
    for entry in json.loads(Path('tests/fixtures/jpeg-generated.json').read_text())['fixtures']:
        jpeg=bytes.fromhex(entry['hex'])
        if hashlib.sha256(jpeg).hexdigest()!=entry['sha256']:raise SystemExit('JPEG fixture hash mismatch')
        props=[ispe(entry['width'],entry['height'])]
        cases=[('plain',jpeg,props)]
        if entry['name']=='17x19-2x2-75-0.jpg':
            for marker,label in [(196,'default-huffman'),(219,'missing-quantization')]:
                parts=[jpeg[:2]];at=2
                while at<len(jpeg):
                    kind=jpeg[at+1]
                    if kind==218:parts.append(jpeg[at:]);break
                    size=int.from_bytes(jpeg[at+2:at+4],'big')+2
                    if kind!=marker:parts.append(jpeg[at:at+size])
                    at+=size
                cases.append((label,b''.join(parts),props))
            for split in [0,2,20,100,len(jpeg)]:cases.append((f'config-{split}',jpeg[split:],props+[box(b'jpgC',jpeg[:split])]))
            for angle in range(4):cases.append((f'rotate-{angle}',jpeg,props+[box(b'irot',bytes([angle]))]))
            for label,nclx in [('full','00010001000680'),('limited','00010001000600'),('unspecified','00020002000280')]:cases.append((label,jpeg,props+[box(b'colr',b'nclx'+bytes.fromhex(nclx))]))
        for label,payload,properties in cases:
            path=directory/(label+'-'+entry['name']+'.heif')
            path.write_bytes(item_file([dict(id=1,kind=b'jpeg',data=payload,props=properties)]))
            test_decode.FIXTURES.append(str(path))
    test_decode.__doc__=__doc__
    test_decode.main()

if __name__=='__main__':main()
