#!/usr/bin/env python3
"""Independent C-client color and HDR differential tests, using upstream headers."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--source", default="tests/upstream")
    p.add_argument("--reference-build", required=True)
    p.add_argument("--candidate", default="target/release/libheifer.so")
    p.add_argument("--output", default=".build/color-report.json")
    args = p.parse_args()
    build = Path(".build/color").resolve()
    include = build / "include/libheif"
    include.mkdir(parents=True, exist_ok=True)
    source = Path(args.source).resolve()
    reference = Path(args.reference_build).resolve()
    (include / "heif_version.h").write_bytes((reference / "libheif/heif_version.h").read_bytes())
    libraries = {"reference": reference / "libheif/libheif.so", "candidate": Path(args.candidate).resolve()}
    results = {}
    for name, library in libraries.items():
        binary = build / name
        subprocess.run(["cc", "-O2", "-std=c11", "-Werror", f"-I{source / 'libheif/api'}", f"-I{include.parent}", "tests/color.c", str(library), f"-Wl,-rpath,{library.parent}", "-o", str(binary)], check=True)
        run = subprocess.run([str(binary)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120, check=True)
        (build / f"{name}.txt").write_bytes(run.stdout)
        (build / f"{name}.stderr").write_bytes(run.stderr)
        results[name] = run.stdout.splitlines()
    mismatches = [{"case": i, "reference": a.decode(), "candidate": b.decode()} for i, (a, b) in enumerate(zip(results["reference"], results["candidate"], strict=True)) if a != b]
    if len(results["reference"]) != 198932:
        raise SystemExit("Incomplete color transcript")
    report = {"scope": "color options, NCLX/ICC and image HDR metadata; no color transforms or handle APIs", "cases": len(results["reference"]), "mismatches": len(mismatches), "reference_sha256": hashlib.sha256(libraries['reference'].read_bytes()).hexdigest(), "candidate_sha256": hashlib.sha256(libraries['candidate'].read_bytes()).hexdigest(), "client_sha256": hashlib.sha256(Path("tests/color.c").read_bytes()).hexdigest(), "transcript_sha256": {k: hashlib.sha256(b"\n".join(v)).hexdigest() for k,v in results.items()}, "examples": mismatches[:10]}
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k:v for k,v in report.items() if k != "examples"}, indent=2))
    if mismatches:
        print(json.dumps(mismatches[:2], indent=2))
        raise SystemExit(1)


if __name__ == '__main__':
    main()
