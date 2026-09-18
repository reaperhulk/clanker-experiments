#!/usr/bin/env python3
"""Independent original-header context/handle/lifetime differential tests."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
from test_hevc import FIXTURES

def box(kind, data=b''):
    return struct.pack('>I4s', len(data) + 8, kind) + data

def full(kind, data=b'', version=0):
    return box(kind, bytes([version, 0, 0, 0]) + data)

def synthetic(width=64, height=33, extra=(), relation=None, metadata=b'AB\x00CD', kind=b'Exif', handler=b'pict', omit=(), primary=1, hidden=False, ref_target=1, aux_type=b'urn:mpeg:hevc:2015:auxid:1', image_id=1):
    properties = [full(b'ispe', struct.pack('>II', width, height)), box(b'hvcC', bytes.fromhex('01016000000090000000000078f000fcfdf8f800000f00'))]
    properties = [p for p in properties if p[4:8] not in omit]
    properties += list(extra)
    if relation == b'auxl' and aux_type is not None:
        properties.append(full(b'auxC', aux_type + b'\x00'))
    ids = [image_id, 2] if relation else [image_id]
    entries = [full(b'infe', struct.pack('>HH4s', i, 0, b'hvc1') + b'image\x00', 2) for i in ids]
    if hidden:
        entries[0] = entries[0][:11] + b'\x01' + entries[0][12:]
    entries.append(full(b'infe', struct.pack('>HH4s', 3, 0, kind) + b'metadata\x00' + (b'application/rdf+xml\x00' if kind == b'mime' else b'urn:example:test\x00' if kind == b'uri ' else b''), 2))
    iinf = full(b'iinf', struct.pack('>H', len(entries)) + b''.join(entries))
    locations = struct.pack('>BBH', 68, 0, len(entries))
    for i in ids + [3]:
        locations += struct.pack('>HHHHII', i, 1, 0, 1, 0, len(metadata) if i == 3 else 0)
    iloc = full(b'iloc', locations, 1)
    associations = struct.pack('>I', len(ids))
    for i in ids:
        props = list(range(1, len(properties) + 1))
        associations += struct.pack('>HB', i, len(props)) + bytes(props)
    refs = box(b'cdsc', struct.pack('>HHH', 3, 1, image_id))
    if relation:
        refs += box(relation, struct.pack('>HHH', 2, 1, ref_target))
    hdlr = full(b'hdlr', bytes(4) + handler + bytes(12) + b'\x00') if handler else b''
    meta = full(b'meta', hdlr + full(b'pitm', struct.pack('>H', primary)) + iinf + iloc + box(b'iprp', box(b'ipco', b''.join(properties)) + full(b'ipma', associations)) + full(b'iref', refs) + box(b'idat', metadata))
    return box(b'ftyp', b'heic\x00\x00\x00\x00heic') + meta + box(b'free', bytes(32))

def corpus(source):
    cases = []
    for f in FIXTURES:
        data = (source / f).read_bytes()
        cases.append((f, data))
    base = synthetic()
    cases.extend(((f'handler-{handler}', synthetic(handler=handler)) for handler in [None, b'null', b'zzzz']))
    for n in range(len(base) + 1):
        cases.append((f'truncated-{n}', base[:n]))
    for image_id in [0, 65535]:
        for hidden in [False, True]:
            cases.append((f'item-id-{image_id}-{hidden}', synthetic(image_id=image_id, primary=image_id, hidden=hidden)))
    for primary in [0, 2, 3, 65535]:
        cases.append((f'primary-{primary}', synthetic(primary=primary)))
    cases.append(('primary-hidden', synthetic(hidden=True)))
    for width, height in [(0, 0), (0, 1), (1, 0), (2147483648, 1), (4294967295, 2)]:
        cases.append((f'dimensions-{width}-{height}', synthetic(width, height)))
    for omit in [(b'ispe',), (b'hvcC',), (b'ispe', b'hvcC')]:
        for rotation in range(4):
            cases.append((f'omit-{omit}-{rotation}', synthetic(omit=omit, extra=[box(b'irot', bytes([rotation]))])))
    configs = {b'ispe': bytes(4) + struct.pack('>II', 64, 33), b'hvcC': bytes.fromhex('01016000000090000000000078f000fcfdf8f800000f00'), b'pasp': struct.pack('>II', 4, 3), b'irot': b'\x01', b'colr': b'nclx' + struct.pack('>HHHB', 1, 13, 6, 128)}
    for kind, value in configs.items():
        for n in range(len(value) + 1):
            cases.append((f'property-truncated-{kind}-{n}', synthetic(omit=(kind,), extra=[box(kind, value[:n])])))
    for relation in [b'thmb', b'auxl']:
        for target in [0, 1, 2, 3, 65535]:
            for aux_type in [None, b'urn:mpeg:hevc:2015:auxid:1', b'urn:test:aux']:
                cases.append((f'reference-{relation}-{target}-{aux_type}', synthetic(relation=relation, ref_target=target, aux_type=aux_type)))
    for matrix in [0, 1, 2, 6, 65535]:
        cases.append((f'nclc-{matrix}', synthetic(extra=[box(b'colr', b'nclc' + struct.pack('>HHH', 1, 13, matrix))])))
    for n in range(80):
        cases.append((f'random-short-{n}', bytes([n]) * n))
    for width, height in [(1, 1), (3, 5), (63, 65), (1280, 854), (65535, 1)]:
        for relation in [None, b'thmb', b'auxl']:
            for kind in [b'Exif', b'mime', b'uri ', b'zzzz']:
                for size in [0, 1, 17, 256]:
                    cases.append((f'properties-{width}-{height}-{relation}-{kind}-{size}', synthetic(width, height, relation=relation, kind=kind, metadata=bytes(range(256))[:size])))
    for rotation in range(4):
        for cp in [1, 2, 9, 22, 65535]:
            for aspect in [(1, 1), (0, 0), (4, 3), (4294967295, 1)]:
                extra = [box(b'irot', bytes([rotation])), box(b'pasp', struct.pack('>II', *aspect)), box(b'colr', b'nclx' + struct.pack('>HHHB', cp, 13, 6, 128)), box(b'colr', b'prof' + b'ICC\x00bytes')]
                cases.append((f'color-{rotation}-{cp}-{aspect}', synthetic(extra=extra)))
    return cases

SCOPE = "Context and handle queries, color/metadata/thumbnails, copied and borrowed memory, context aliases and lifetimes; no decoding or reader callbacks"

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build', required=True)
    p.add_argument('--source', default='tests/upstream')
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--output', default='.build/context-report.json')
    p.add_argument('--sanitize', action='store_true', help='Instrument C clients with ASan/UBSan; libraries are not rebuilt with sanitizers')
    p.add_argument('--no-leak-check', action='store_true', help='Disable LeakSanitizer where ptrace prevents it from running')
    args = p.parse_args()
    if args.no_leak_check and not args.sanitize:
        p.error('--no-leak-check requires --sanitize')
    Path(args.output).unlink(missing_ok=True)
    work = Path('.build/context-sanitized' if args.sanitize else '.build/context').resolve()
    include = work / 'include/libheif'
    include.mkdir(parents=True, exist_ok=True)
    source = Path(args.source).resolve()
    reference = Path(args.reference_build).resolve()
    (include / 'heif_version.h').write_bytes((reference / 'libheif/heif_version.h').read_bytes())
    cases = corpus(source)
    payload = b''.join((struct.pack('=I', len(data)) + data for _, data in cases))
    results = {}
    libraries = {'reference': reference / 'libheif/libheif.so', 'candidate': Path(args.candidate).resolve()}
    hashes = {name: hashlib.sha256(lib.read_bytes()).hexdigest() for name, lib in libraries.items()}
    client_hash = hashlib.sha256(Path('tests/context.c').read_bytes()).hexdigest()
    sanitizer_flags = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer'] if args.sanitize else []
    env = dict(os.environ)
    if args.sanitize:
        env["UBSAN_OPTIONS"] = env.get("UBSAN_OPTIONS", "") + ":halt_on_error=1"
    if args.no_leak_check:
        env["ASAN_OPTIONS"] = env.get("ASAN_OPTIONS", "") + ":detect_leaks=0"
    for name, lib in libraries.items():
        binary = work / name
        subprocess.run(['cc', '-std=c11', '-O2', '-Werror', *sanitizer_flags, f"-I{source / 'libheif/api'}", f'-I{include.parent}', 'tests/context.c', str(lib), f'-Wl,-rpath,{lib.parent}', '-o', str(binary)], check=True)
        run = subprocess.run([str(binary)], input=payload, capture_output=True, timeout=180, env=env)
        (work / f'{name}.stderr').write_bytes(run.stderr)
        if run.returncode:
            raise SystemExit(f"{name} client exited {run.returncode}: {run.stderr.decode(errors='replace')[:4000]}")
        (work / f'{name}.txt').write_bytes(run.stdout)
        results[name] = run.stdout.splitlines()
        if len(results[name]) != 2 * len(cases):
            raise SystemExit(f'Incomplete {name} transcript')
    if any(hashlib.sha256(lib.read_bytes()).hexdigest() != hashes[name] for name, lib in libraries.items()) or hashlib.sha256(Path('tests/context.c').read_bytes()).hexdigest() != client_hash:
        raise SystemExit('Library or client changed during the run; refusing mixed evidence')
    mismatches = [{'case': i, 'name': cases[i // 2][0], 'reference': a.decode(), 'candidate': b.decode()} for i, (a, b) in enumerate(zip(results['reference'], results['candidate'], strict=True)) if a != b]
    report = {'scope': SCOPE, 'client_sanitizers': args.sanitize, 'leak_check': args.sanitize and not args.no_leak_check, 'cases': 2 * len(cases), 'mismatches': len(mismatches), 'corpus_sha256': hashlib.sha256(payload).hexdigest(), 'client_sha256': hashlib.sha256(Path('tests/context.c').read_bytes()).hexdigest(), 'reference_sha256': hashlib.sha256(libraries['reference'].read_bytes()).hexdigest(), 'candidate_sha256': hashlib.sha256(libraries['candidate'].read_bytes()).hexdigest(), 'examples': mismatches[:20]}
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k != 'examples'}, indent=2))
    if mismatches:
        for m in mismatches[:3]:
            a = m['reference']
            b = m['candidate']
            at = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            print(m['name'], m['case'], 'offset', at, 'reference:', a[max(0, at - 40):at + 160], 'candidate:', b[max(0, at - 40):at + 160])
        raise SystemExit(1)
if __name__ == '__main__':
    main()
