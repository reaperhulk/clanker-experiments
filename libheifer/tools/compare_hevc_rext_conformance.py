#!/usr/bin/env python3
"""Decode the JCT-VC HEVC range-extension conformance streams with the candidate's
RExt decoder (oxideav-h265, examples/hevc_sequence_yuv.rs) and with libde265's
dec265, and compare each whole output sequence with the conformance package's
reconstruction MD5.

The streams are downloaded from the ITU-T site into --work (73 MB) and are not
redistributed; the report records every stream's name, SHA-256 and results.
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import urllib.request
import zipfile
from pathlib import Path

SITE = 'https://www.itu.int/wftp3/av-arch/jctvc-site/bitstream_exchange/draft_conformance/RExt/'


def md5_of(command, env=None):
    """MD5 and size of a command's stdout, streamed."""
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    digest, size = hashlib.md5(), 0
    while chunk := process.stdout.read(1 << 20):
        digest.update(chunk)
        size += len(chunk)
    stderr = process.stderr.read().decode(errors='replace')
    return process.wait(), digest.hexdigest(), size, stderr.strip().splitlines()[-1:]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--work', default='.build/hevc-rext-conformance')
    p.add_argument('--dec265', default='.build/libde265-install/bin/dec265')
    p.add_argument('--candidate', default='target/release/examples/hevc_sequence_yuv')
    p.add_argument('--output', default='.build/hevc-rext-conformance-report.json')
    a = p.parse_args()
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    listing = urllib.request.urlopen(SITE).read().decode()
    names = sorted(set(re.findall(r'HREF="[^"]*/([^/"]+\.zip)"', listing)))
    dec265 = Path(a.dec265).resolve()
    env = dict(os.environ, LD_LIBRARY_PATH=str(dec265.parent.parent / 'lib'))
    results = []
    for name in names:
        archive = work / name
        if not archive.exists():
            archive.write_bytes(urllib.request.urlopen(SITE + name).read())
        with zipfile.ZipFile(archive) as z:
            members = z.namelist()
            stream = next(m for m in members if m.lower().endswith(('.bit', '.bin')))
            data = z.read(stream)
            sums = b''.join(z.read(m) for m in members if 'md5' in Path(m).name.lower())
        # The reconstruction's entry: the MD5 file line that does not name the bitstream.
        expected = next((line.split()[0].lower() for line in sums.decode(errors='replace').splitlines()
                         if line.strip() and not re.search(r'\.(bit|bin)\b', line, re.I)), None)
        path = work / Path(stream).name
        path.write_bytes(data)
        record = dict(stream=Path(stream).name, sha256=hashlib.sha256(data).hexdigest(), expected_md5=expected)
        for label, command, environment in [
                ('candidate', [str(Path(a.candidate).resolve()), str(path)], None),
                ('libde265', [str(dec265), '--quiet', '--output', '/dev/stdout', str(path)], env)]:
            code, digest, size, message = md5_of(command, environment)
            record[label] = dict(exit=code, md5=digest, bytes=size, match=code == 0 and digest == expected,
                                 message=message)
        path.unlink()
        results.append(record)
        print(record['stream'], 'candidate', record['candidate']['match'], 'libde265', record['libde265']['match'], flush=True)
    report = dict(scope=__doc__, site=SITE, streams=len(results),
                  candidate_matches=sum(r['candidate']['match'] for r in results),
                  libde265_matches=sum(r['libde265']['match'] for r in results),
                  candidate_sha256=hashlib.sha256(Path(a.candidate).read_bytes()).hexdigest(),
                  dec265_sha256=hashlib.sha256(dec265.read_bytes()).hexdigest(), results=results)
    Path(a.output).write_text(json.dumps(report, indent=1) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k not in ('results', 'scope')}, indent=1))


if __name__ == '__main__':
    main()
