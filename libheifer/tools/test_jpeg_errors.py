#!/usr/bin/env python3
"""Independent JPEG truncation, marker and frame-description error comparisons."""
import json
from pathlib import Path
import sys
import test_decode
from item_fixtures import item_file,ispe

def main(all_prefixes=True):
    if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg-errors']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg-errors-report.json']
    if '--modes' not in sys.argv:sys.argv+=['--modes','0,11,27']
    directory=Path(sys.argv[sys.argv.index('--work')+1]).resolve()/'fixtures';directory.mkdir(parents=True,exist_ok=True)
    test_decode.FIXTURES=[]
    for entry in json.loads(Path('tests/fixtures/jpeg-generated.json').read_text())['fixtures']:
        if entry['name'] not in ['17x19-2x2-75-0.jpg','17x19-2x2-75-1.jpg','17x19-gray-75-0.jpg']:continue
        data=bytes.fromhex(entry['hex']);sof=next(i for i in range(len(data)-1) if data[i]==255 and data[i+1] in [192,193,194]);sos=data.index(b'\xff\xda')
        variants=[(f'truncated-{n}',data[:n]) for n in (range(len(data)+1) if all_prefixes else sorted(set([0,1,2,4,20,sof,sof+10,sof+20,sos,sos+10,len(data)//2,len(data)-20,len(data)-2,len(data)-1])))]
        for name,at,values in [('soi',0,[0,254]),('marker',1,[0,216,217]),('precision',sof+4,[0,7,8,12,16]),('height',sof+5,[0,255]),('components',sof+9,[0,1,2,4]),('sampling',sof+11,[0,17,33,34,49,255])]:
            for value in values:
                d=bytearray(data);d[at]=value;variants.append((f'{name}-{value}',bytes(d)))
        for name,jpeg in variants:
            path=directory/(name+'-'+entry['name']+'.heif');path.write_bytes(item_file([dict(id=1,kind=b'jpeg',data=jpeg,props=[ispe(entry['width'],entry['height'])])]))
            test_decode.FIXTURES.append(str(path))
    test_decode.__doc__=__doc__;test_decode.main()

if __name__=='__main__':main()
