#!/usr/bin/env python3
"""Interleaved parse + native-sample decode timings, with file I/O outside timing."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--reference-build", required=True)
    p.add_argument("--samples", type=int, default=11)
    p.add_argument("--iterations", type=int, default=3)
    p.add_argument("--output", default=".build/hevc-bench.json")
    args = p.parse_args()
    evidence = json.loads(Path(".build/hevc-report.json").read_text())
    fixture = "examples/example.heic"
    row = next(r for r in evidence["fixtures"] if r["fixture"] == fixture)
    if not row["native_match"]:
        raise SystemExit("Refusing timings without exact output equality")
    source = Path("tests/upstream").resolve()
    work = Path(".build/hevc").resolve()
    reference = Path(args.reference_build).resolve()
    library = reference / "libheif/libheif.so"
    candidate = Path("target/release/examples/bench_hevc").resolve()
    binary = work / "bench-reference"
    subprocess.run(["cc", "-O3", "-std=c11", "-Werror", f"-I{source / 'libheif/api'}", f"-I{work / 'include'}", "tests/bench_hevc.c", str(library), f"-Wl,-rpath,{library.parent}", "-o", str(binary)], check=True)
    samples = []
    for i in range(args.samples):
        pair = {}
        for name in (["reference", "candidate"] if i % 2 == 0 else ["candidate", "reference"]):
            path = binary if name == "reference" else candidate
            run = subprocess.run([str(path), str(source / fixture), str(args.iterations)], check=True, capture_output=True, timeout=120)
            pair[name] = json.loads(run.stdout)
        if pair["candidate"]["checksum"] != pair["reference"]["checksum"]:
            raise SystemExit("Benchmark checksum mismatch")
        samples.append(pair)
    summary = {}
    for name in ("reference", "candidate"):
        values = [s[name]["ns"] / s[name]["iterations"] / 1e6 for s in samples]
        summary[name] = {"median_ms": statistics.median(values), "min_ms": min(values), "max_ms": max(values), "stdev_ms": statistics.stdev(values)}
    summary["elapsed_reduction_percent"] = (1 - summary["candidate"]["median_ms"] / summary["reference"]["median_ms"]) * 100
    report = {"scope": "memory parse + all top-level HEVC native YUV decode + teardown; I/O and output serialization excluded; one corpus file with two images; no transforms/profile conversion/alpha", "limitation": "Candidate lacks most libheif behavior. This does not establish whole-library performance or memory efficiency.", "platform": platform.platform(), "cpu": next((l.split(':',1)[1].strip() for l in Path('/proc/cpuinfo').read_text().splitlines() if l.startswith('model name')), 'unknown'), "rustc": subprocess.check_output(['rustc','--version'],text=True).strip(), "reference": "libheif 1.23.4, libde265 1.0.16, Release", "candidate": "libheifer Release, rusty_h265 0.6.0, Rust SIMD default", "fixture_sha256": hashlib.sha256((source/fixture).read_bytes()).hexdigest(), "reference_sha256": hashlib.sha256(library.read_bytes()).hexdigest(), "candidate_sha256": hashlib.sha256(candidate.read_bytes()).hexdigest(), "summary": summary, "samples": samples}
    Path(args.output).write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(summary,indent=2))


if __name__ == '__main__':
    main()
