#!/usr/bin/env python3
"""Property parse failures, item errors and optional warnings through image decoding."""
from pathlib import Path
import sys
import test_decode
from test_properties import corpus

def main():
    root=Path('.build/decode-properties-inputs').resolve();root.mkdir(parents=True,exist_ok=True)
    paths=[]
    for i,(_,data) in enumerate(corpus()[0]):
        path=root/f'property-{i}.heic';path.write_bytes(data);paths.append(str(path))
    test_decode.FIXTURES=paths
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/decode-properties-report.json'])
    if '--modes' not in sys.argv:sys.argv.extend(['--modes','1,2,3,11'])
    test_decode.main()

if __name__=='__main__':main()
