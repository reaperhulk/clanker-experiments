#!/usr/bin/env python3
"""Original-header handle/lifetime checks for the generated derived and mask corpus."""
from pathlib import Path
import hashlib
import json
import sys
import test_context


def corpus(_source):
    cases=[]
    for family in ['derived','overlay','mask','graphs']:
        report=json.loads(Path(f'.build/decode-{family}-report.json').read_text())
        fixtures={r['fixture']:r['fixture_sha256'] for r in report['results']}
        if not fixtures:raise SystemExit(f'Empty {family} fixture manifest')
        for name,digest in fixtures.items():
            path=Path(name);data=path.read_bytes()
            if hashlib.sha256(data).hexdigest()!=digest:raise SystemExit(f'Fixture changed: {name}')
            cases.append((family+'/'+path.name,data))
    return cases


if __name__=='__main__':
    test_context.corpus=corpus
    test_context.SCOPE="Generated grid, identity, overlay and mask handle queries and lifetimes; fixture hashes verified against decode manifests"
    if '--output' not in sys.argv:sys.argv.extend(['--output','.build/derived-handles-report.json'])
    test_context.main()
