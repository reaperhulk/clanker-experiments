#!/usr/bin/env python3
"""Independent native-dav1d/Rust-rav1d AVIF pixels, color, alpha, transforms and errors."""
import sys
import hashlib
import json
from pathlib import Path
import test_decode

def main():
    test_decode.FIXTURES=['examples/example.avif']+[str(p.relative_to('tests/upstream')) for p in sorted(Path('tests/upstream/tests/data').glob('*.avif'))]
    if '--work' not in sys.argv:sys.argv+=['--work','.build/av1']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/av1-report.json']
    if '--modes' not in sys.argv:sys.argv+=['--modes',','.join(map(str,[*range(23),26,27]))]
    work = Path(sys.argv[sys.argv.index('--work') + 1]).resolve() / 'fixtures'
    work.mkdir(parents=True, exist_ok=True)
    for entry in json.loads(Path('tests/fixtures/av1-generated.json').read_text())['fixtures']:
        data = bytes.fromhex(entry['hex'])
        if hashlib.sha256(data).hexdigest() != entry['sha256']:
            raise SystemExit('Generated AV1 fixture hash mismatch')
        path = work / entry['name']
        path.write_bytes(data)
        test_decode.FIXTURES.append(str(path))
        # Change only container signalling; the compressed pixels/sequence header
        # remain those emitted by the independent native encoder.
        at = data.index(b'nclx') + 4
        for label, profile in [('limited', bytes.fromhex('00010001000100')),
                               ('unspecified', bytes.fromhex('00020002000280'))]:
            variant = work / (label + '-' + entry['name'])
            variant.write_bytes(data[:at] + profile + data[at+7:])
            test_decode.FIXTURES.append(str(variant))
    test_decode.__doc__ = __doc__
    test_decode.main()
if __name__=='__main__':main()
