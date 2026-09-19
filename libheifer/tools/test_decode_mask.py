#!/usr/bin/env python3
"""Independent built-in mask decoding, profiles, bit depth and truncation cases."""
from pathlib import Path
import sys
import test_decode
from item_fixtures import item_file,ispe
from test_context import full


def main():
    cases=[]
    for depth in [8,16]:
        for width,height in [(1,1),(4,5),(17,9)]:
            payload=bytes((i*37+11)&255 for i in range(width*height*(depth//8)))
            cases.append((f'valid-{depth}-{width}-{height}',payload,[ispe(width,height),full(b'mskC',bytes([depth]))]))
        payload=bytes((i*37+11)&255 for i in range(4*5*(depth//8)))
        for size in range(len(payload)):
            cases.append((f'payload-{depth}-{size}',payload[:size],[ispe(4,5),full(b'mskC',bytes([depth]))]))
        cases.append((f'trailing-{depth}',payload+bytes(32),[ispe(4,5),full(b'mskC',bytes([depth]))]))
    for depth in [0,1,7,9,15,17,128,255]:cases.append((f'depth-{depth}',bytes(640),[ispe(4,5),full(b'mskC',bytes([depth]))]))
    for version in [1,2,255]:cases.append((f'version-{version}',bytes(20),[ispe(4,5),full(b'mskC',b'\10',version)]))
    for size in range(5):cases.append((f'config-{size}',bytes(20),[ispe(4,5),full(b'mskC',b'\10')[:8+size]]))
    cases.append(('missing-config',bytes(20),[ispe(4,5)]))
    cases.append(('missing-ispe',bytes(20),[full(b'mskC',b'\10')]))
    for w,h in [(0,5),(4,0),(32769,32769)]:cases.append((f'size-{w}-{h}',bytes(20),[ispe(w,h),full(b'mskC',b'\10')]))
    root=Path('.build/decode-mask-inputs').resolve();root.mkdir(parents=True,exist_ok=True)
    paths=[]
    for name,payload,props in cases:
        # Correct the outer box size when deliberately truncating its payload.
        props=[len(p).to_bytes(4,'big')+p[4:] for p in props]
        path=root/(name+'.heic');path.write_bytes(item_file([dict(id=1,kind=b'mski',data=payload,props=props)]));paths.append(str(path))
    test_decode.FIXTURES=paths
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/decode-mask-report.json'])
    if '--modes' not in sys.argv:sys.argv.extend(['--modes','0,1,2,3,4,5,6,8,10,11,12,13,14,15'])
    test_decode.main()


if __name__=='__main__':main()
