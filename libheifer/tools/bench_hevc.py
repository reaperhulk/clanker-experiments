#!/usr/bin/env python3
"""Interleaved parse + native-sample decode timings, with fresh output validation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
from test_hevc import FIXTURES


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inputs():
    files = [Path("Cargo.toml"), Path("Cargo.lock")]
    for directory in ("src", "crates", "examples"):
        files.extend(Path(directory).rglob("*.rs"))
        files.extend(Path(directory).rglob("Cargo.toml"))
    files += [Path(p) for p in ("tools/bench_hevc.py", "tools/test_hevc.py", "tests/hevc_reference.c", "tests/bench_hevc.c")]
    return {str(p): sha(p) for p in sorted(set(files))}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--reference-build", required=True)
    p.add_argument("--fixture", choices=FIXTURES, default=FIXTURES[0])
    p.add_argument("--samples", type=int, default=11)
    p.add_argument("--iterations", type=int, default=3)
    p.add_argument("--output", default=".build/hevc-bench.json")
    args = p.parse_args()
    if args.samples < 3 or args.iterations < 1:
        p.error("at least three sample pairs and one measured iteration are required")
    source = Path("tests/upstream").resolve()
    work = Path(".build/hevc").resolve()
    work.mkdir(parents=True, exist_ok=True)
    reference = Path(args.reference_build).resolve()
    library = reference / "libheif/libheif.so"
    target = Path(".build/benchmark-target").resolve()
    # An isolated Cargo target avoids accidentally timing the `hevc-profile`
    # feature. Both clients use exactly the same resolved features/source tree.
    env = dict(os.environ, CARGO_TARGET_DIR=str(target))
    source_hashes = inputs()
    build_command = ["cargo", "build", "--locked", "--release", "--features", "hevc", "--example", "bench_hevc", "--example", "hevc_probe"]
    subprocess.run(build_command, env=env, check=True)
    candidate = target / "release/examples/bench_hevc"
    probe = target / "release/examples/hevc_probe"
    evidence_path = work / "benchmark-output-report.json"
    evidence_path.unlink(missing_ok=True)
    subprocess.run([sys.executable, "tools/test_hevc.py", "--reference-build", str(reference), "--candidate", str(probe), "--output", str(evidence_path)], check=True, stdout=subprocess.DEVNULL)
    evidence = json.loads(evidence_path.read_text())
    fixture = args.fixture
    row = next(r for r in evidence["fixtures"] if r["fixture"] == fixture)
    hashes = {"fixture_sha256": sha(source / fixture), "reference_sha256": sha(library), "candidate_sha256": sha(candidate), "candidate_probe_sha256": sha(probe)}
    if not row["native_match"] or row["sha256"] != hashes["fixture_sha256"] or evidence["reference_sha256"] != hashes["reference_sha256"] or evidence["candidate_sha256"] != hashes["candidate_probe_sha256"]:
        raise SystemExit("Refusing timings without fresh exact native output equality")
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
    if source_hashes != inputs() or hashes != {"fixture_sha256": sha(source / fixture), "reference_sha256": sha(library), "candidate_sha256": sha(candidate), "candidate_probe_sha256": sha(probe)}:
        raise SystemExit("Source, input or binaries changed during measurement; results discarded")
    summary = {}
    for name in ("reference", "candidate"):
        values = [s[name]["ns"] / s[name]["iterations"] / 1e6 for s in samples]
        summary[name] = {"median_ms": statistics.median(values), "min_ms": min(values), "max_ms": max(values), "stdev_ms": statistics.stdev(values)}
    summary["elapsed_reduction_percent"] = (1 - summary["candidate"]["median_ms"] / summary["reference"]["median_ms"]) * 100
    report = {"scope": "memory parse + all top-level HEVC native YUV decode + teardown; I/O and output serialization excluded; one corpus file; no transforms/profile conversion/alpha", "limitation": "Candidate lacks most libheif behavior. This does not establish whole-library performance or memory efficiency.", "platform": platform.platform(), "cpu": next((l.split(':',1)[1].strip() for l in Path('/proc/cpuinfo').read_text().splitlines() if l.startswith('model name')), 'unknown'), "rustc": subprocess.check_output(['rustc','--version'],text=True).strip(), "reference": "libheif 1.23.4, libde265 1.0.16, Release", "candidate": "libheifer Release, rusty_h265 0.6.0, Rust SIMD default", "build_command": build_command, "fixture": fixture, **hashes, "reference_client_sha256": sha(binary), "output_evidence_sha256": sha(evidence_path), "source_hashes": source_hashes, "summary": summary, "samples": samples}
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(summary,indent=2))


if __name__ == '__main__':
    main()
