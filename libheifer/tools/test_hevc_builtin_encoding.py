#!/usr/bin/env python3
"""Original-header built-in HEVC encoding against libheif with its x265 plugin: errors, parameters,
handles and non-codec boxes exactly; HEVC configuration formats; candidate files decoded by libde265."""
import argparse
import hashlib
import json
import os
import re
import struct
import subprocess
from pathlib import Path

# tests/encoding.c parameter sets 58-77 are the x265 ones; 1, 31 and 45 exercise unknown names.
PARAMETER_SETS = list(range(58, 78)) + [1, 31, 45]


def corpus():
    cases = []

    def add(v):
        cases.append(('-'.join(map(str, v)), struct.pack('=8I', *(x & 0xffffffff for x in v))))

    lossless = 16384
    for width, height in [(1, 1), (2, 3), (7, 5), (31, 40), (64, 48), (97, 70), (130, 66)]:
        for space, chroma in [(0, 1), (0, 2), (0, 3), (1, 3), (1, 10), (2, 0)]:
            add([1, width, height, space, chroma, 8, 1, 8])
    for quality in [0, 1, 25, 50, 75, 99, 100]:
        add([1 | (quality + 1) << 16, 64, 48, 0, 1, 8, 1, 8])
        add([1 | (quality + 1) << 16, 48, 40, 0, 3, 10, 1, 8])
    for mode in [lossless, 2048, 32768, lossless | 2048]:
        add([1 | mode, 64, 48, 0, 3, 8, 1, 8])
        add([1 | mode, 40, 36, 2, 0, 12, 1, 8])
    for depth in [7, 8, 9, 10, 12, 16]:
        for space, chroma in [(0, 1), (2, 0), (0, 3), (0, 2)]:
            add([1, 40, 36, space, chroma, depth, 1, 8])
    for index in PARAMETER_SETS:
        add([1 | index << 24, 70, 50, 0, 1, 8, 1, 8])
        add([1 | 51 << 16 | index << 24, 33, 29, 0, 3, 10, 1, 8])
    # Metadata (nclx, HDR, ICC), orientations, thumbnails, overlays, alpha and repeated encodes.
    for flags in [1024, 2048, 3072, 4096, 8192, 16384 | 3072, 32768 | 1024]:
        for orientation in [1, 6]:
            add([1, 48, 36, 0, 1, 8, orientation, 8 | flags])
    for mode in [256, 1024, 4096, 8192]:
        add([1 | mode, 48, 36, 0, 1, 8, 1, 8])
    for space, chroma in [(0, 1), (0, 3), (1, 3), (2, 0)]:
        add([1, 48, 36, space, chroma, 8, 1, 8 | 512])
        add([1, 48, 36, space, chroma, 10, 1, 8 | 512])
    for bbox in [1, 2, 6, 16]:
        add([1 | 512, 64, 40, 0, 1, 8, bbox, 8])
    return cases, b''.join(data for _, data in cases)


def boxes(data, start=0, end=None):
    end = len(data) if end is None else end
    while start + 8 <= end:
        size = int.from_bytes(data[start:start + 4], 'big')
        kind = data[start + 4:start + 8]
        header = 8
        if size == 1:
            size = int.from_bytes(data[start + 8:start + 16], 'big')
            header = 16
        elif size == 0:
            size = end - start
        if size < header or start + size > end:
            yield kind, data[start:end], None
            return
        yield kind, data[start + header:start + size], data[start:start + size]
        start += size


CONTAINERS = {b'meta': 4, b'iprp': 0, b'ipco': 0, b'iinf': 6, b'dinf': 0, b'iref': 4, b'grpl': 0}


def hvcc_summary(body):
    """The hvcC fields libheif derives from the stream format, not from encoder choices."""
    if len(body) < 23:
        return 'hvcC-short:' + body.hex()
    chroma = body[16] & 3
    luma = (body[17] & 7) + 8
    chroma_depth = (body[18] & 7) + 8
    arrays = []
    at = 23
    for _ in range(body[22]):
        if at + 3 > len(body):
            break
        kind = body[at] & 63
        count = int.from_bytes(body[at + 1:at + 3], 'big')
        at += 3
        for _ in range(count):
            size = int.from_bytes(body[at:at + 2], 'big')
            at += 2 + size
        arrays.append(f'{kind}x{count}')
    return f'hvcC(version={body[0]},chroma={chroma},depth={luma}/{chroma_depth},arrays={"+".join(arrays)})'


def normalize(data, depth=0):
    """Box tree with every box byte-exact except hvcC (format fields), iloc (item and
    extent counts) and mdat (length only when not HEVC data)."""
    out = []
    for kind, body, whole in boxes(data):
        name = kind.decode('latin1')
        if whole is None:
            out.append(f'truncated-{name}')
        elif kind == b'hvcC':
            out.append(hvcc_summary(body))
        elif kind == b'mdat':
            out.append('mdat')
        elif kind == b'iloc':
            out.append(f'iloc(version={body[0]})')
        elif kind == b'mini':
            # Compact files: the header flags, chroma and orientation exactly
            # (the first 16 bits); sizes, configuration and data are codec output.
            out.append(f'mini({body[:2].hex()},chroma={int.from_bytes(body[:2], "big") >> 3 & 3})')
        elif kind in CONTAINERS:
            skip = CONTAINERS[kind]
            if kind == b'iinf' and body[:1] == b'\x00':
                skip = 6
            elif kind == b'iinf':
                skip = 8
            elif kind == b'iref':
                # iref children are small references: keep them exact.
                out.append(f'{name}[{whole.hex()}]')
                continue
            out.append(f'{name}{{{body[:skip].hex()}|{normalize(body[skip:], depth + 1)}}}')
        else:
            out.append(f'{name}:{whole.hex()}')
    return ' '.join(out)


def transcript(line):
    """Normalize one transcript line; returns (text, candidate files)."""
    text = line.decode(errors='replace').replace('hpvca', 'x265')
    files = []
    parts = text.split(' ')
    for i, part in enumerate(parts):
        if part.startswith('file') and ':' in part:
            size, blob = part[4:].split(':', 1)
            data = bytes.fromhex(blob)
            files.append(data)
            parts[i] = f'file[{normalize(data)}]'
        elif part.startswith('pixels') and ':' in part:
            parts[i] = part.split(':', 1)[0] + ':<pixels>'
    return ' '.join(parts), files


REXT_ERROR = re.compile(r'e7,0,Decoder plugin generated an error: Unspecified: Codec\(Unsupported\(".*?"\)\)')


def classify(reference, candidate):
    """A named, visible known difference, or None for a mismatch."""
    if 'Unsupported x265 encoder parameter:' in candidate and 'paramx265:' in candidate:
        return 'x265-option'
    if 'x265 encoder could not be opened' in reference and 'e9,5001' not in candidate:
        return 'x265-open-failure'
    if REXT_ERROR.search(candidate):
        # The candidate's HEVC decoder lacks the range extensions: apart from
        # decodes that fail there and succeed in libde265, the lines agree.
        decoded = re.compile(r'e0,0,Success (?:pixels\S* ?)+')
        c = [part.strip() for part in REXT_ERROR.split(candidate)]
        r = [part.strip() for part in decoded.split(reference)]
        if len(c) == len(r) and c == r:
            return 'hevc-rext-decoding'
    # hpvca signals Main / Main 10 for 4:0:0 where x265 (and the standard)
    # use the RExt Monochrome profiles, so libheif chooses the heic brand.
    heix, heic = '68656978', '68656963'
    if 'chroma=0' in candidate and reference.replace(heix, heic) == candidate:
        return 'hpvca-monochrome-profile'
    strip = re.compile(r' ?(hvcC\([^)]*\)|ipma:[0-9a-f]*)')
    if strip.sub('', reference) == strip.sub('', candidate) and candidate.count('hvcC(') < reference.count('hvcC('):
        return 'shared-hvcC'
    return None


def fallback(reference, candidate):
    """Classify a differing transcript line of another encoding suite in which
    libheif falls back to its default HEVC encoder: x265 in the oracle, hpvca
    in the candidate. Files are compared as normalized box trees and decoded
    pixels are masked; returns a known-difference kind or None."""
    r, _ = transcript(reference)
    c, _ = transcript(candidate)
    # Only lines whose files carry HEVC (an hvcC, or a compact file branded heic/heix).
    hevc = 'hvcC(' in c or ('mini(' in c and ('68656963' in c or '68656978' in c))
    if not hevc:
        return None
    if r == c:
        return 'builtin-hevc-fallback'
    return classify(r, c)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference-build', default='.build/reference-x265')
    p.add_argument('--candidate', default='target/release/libheifer.so')
    p.add_argument('--output', default='.build/hevc-builtin-encoding-report.json')
    p.add_argument('--work', default='.build/hevc-builtin-encoding')
    p.add_argument('--sanitize', action='store_true')
    a = p.parse_args()
    work = Path(a.work).resolve()
    inc = work / 'include/libheif'
    inc.mkdir(parents=True, exist_ok=True)
    ref = Path(a.reference_build).resolve()
    (inc / 'heif_version.h').write_bytes((ref / 'libheif/heif_version.h').read_bytes())
    libs = {'reference': ref / 'libheif/libheif.so', 'candidate': Path(a.candidate).resolve()}
    hashes = {k: hashlib.sha256(v.read_bytes()).hexdigest() for k, v in libs.items()}
    env = dict(os.environ)
    if a.sanitize:
        env['UBSAN_OPTIONS'] = 'halt_on_error=1'
    flags = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer'] if a.sanitize else []
    cases, payload = corpus()
    lines = {}
    for name, lib in libs.items():
        for client in ['encoding', 'decode', 'encoder_listing']:
            subprocess.run(['cc', '-std=c11', '-O2', '-Werror', *flags, '-Itests/upstream/libheif/api', f'-I{inc.parent}',
                            f'tests/{client}.c', str(lib), f'-Wl,-rpath,{lib.parent}', '-o', str(work / f'{name}-{client}')],
                           check=True)
        run = subprocess.run([str(work / f'{name}-encoding')], input=payload, capture_output=True, timeout=1800, env=env)
        (work / f'{name}.txt').write_bytes(run.stdout)
        if run.returncode:
            raise SystemExit(f'{name} exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        lines[name] = run.stdout.splitlines()
    if any(len(v) != len(cases) for v in lines.values()):
        raise SystemExit(f'Incomplete transcript: {list(map(len, lines.values()))}')
    differences = []
    # The default HEVC encoder's parameter surface: types, defaults, valid values and values.
    listings = {}
    for name in libs:
        run = subprocess.run([str(work / f'{name}-encoder_listing'), '1'], capture_output=True, timeout=60, env=env)
        if run.returncode:
            raise SystemExit(f'{name} listing exited {run.returncode}: {run.stderr.decode(errors="replace")[:3000]}')
        listings[name] = run.stdout.decode(errors='replace').replace('hpvca', 'x265')
    if listings['reference'] != listings['candidate']:
        differences.append(dict(case='encoder-listing', reference=listings['reference'], candidate=listings['candidate']))
    known = []
    files = []
    for i, (x, y) in enumerate(zip(lines['reference'], lines['candidate'], strict=True)):
        rx, _ = transcript(x)
        ry, candidate_files = transcript(y)
        files += [(cases[i][0], data) for data in candidate_files]
        if rx != ry and (kind := classify(rx, ry)):
            known.append(dict(case=cases[i][0], line=i, kind=kind))
        elif rx != ry:
            at = next((j for j, (u, v) in enumerate(zip(rx, ry)) if u != v), min(len(rx), len(ry)))
            differences.append(dict(case=cases[i][0], line=i, offset=at, reference=rx[max(0, at - 80):at + 300],
                                    candidate=ry[max(0, at - 80):at + 300]))
    # Interoperability: libheif with libde265 decodes every candidate file exactly as the candidate does.
    decode_mismatches = []
    for index, (case, data) in enumerate(files):
        path = work / f'file-{index}.heif'
        path.write_bytes(data)
        outputs = {}
        for name in libs:
            result = []
            for mode in [0, 1]:
                target = work / f'file-{index}-{name}-{mode}.out'
                run = subprocess.run([str(work / f'{name}-decode'), str(path), str(target), str(mode)], capture_output=True,
                                     timeout=120, env=env)
                result.append((run.returncode, target.read_bytes() if target.exists() else b''))
            outputs[name] = result
        if outputs['reference'] != outputs['candidate']:
            unsupported = any(b'Codec(Unsupported(' in out for _, out in outputs['candidate'])
            reference_ok = all(code == 0 and b'e7,' not in out for code, out in outputs['reference'])
            if unsupported and reference_ok:
                known.append(dict(case=case, file=index, kind='hevc-rext-decoding'))
            else:
                decode_mismatches.append(dict(case=case, file=index))
    kinds = {}
    for k in known:
        kinds[k['kind']] = kinds.get(k['kind'], 0) + 1
    report = dict(scope=__doc__, cases=len(cases), listing=listings['candidate'], mismatches=len(differences), files=len(files),
                  known_differences=kinds, known=known,
                  decode_mismatches=decode_mismatches, client_sanitizers=a.sanitize,
                  corpus_sha256=hashlib.sha256(payload).hexdigest(), differences=differences,
                  client_sha256=hashlib.sha256(b''.join(Path(f'tests/{c}.c').read_bytes() for c in ('encoding', 'decode', 'encoder_listing'))).hexdigest(),
                  **{k + '_sha256': v for k, v in hashes.items()})
    Path(a.output).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k not in ('differences', 'decode_mismatches', 'known')}, indent=2))
    print(json.dumps(differences[:4], indent=2))
    print(json.dumps(decode_mismatches[:4], indent=2))
    if differences or decode_mismatches:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
