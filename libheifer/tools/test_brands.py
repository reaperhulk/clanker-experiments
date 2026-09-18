#!/usr/bin/env python3
"""Compare an original-header C client in isolated reference/candidate processes."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import subprocess


def corpus():
    cases = [b"", b"\x89PNG\r\n\x1a\n", bytes.fromhex("ffd8ffe000104a4649460001"), bytes.fromhex("ffd8ffe11234457869660000")]
    brands = [b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"mif1", b"mif2", b"mif3", b"msf1", b"avif", b"avis", b"avci", b"avcs", b"vvic", b"vvis", b"evbi", b"evbs", b"j2ki", b"j2is", b"jpeg", b"jpgs", b"miaf", b"isom", b"mp41", b"mp42", b"zzzz", b"\0vif", b"a\0if", b"av\0f", b"avi\0", b"\xff\x80\xfe\x81"]
    for main in brands:
        for minor in [b"\0\0\0\0", b"avif", b"heic"]:
            for compatible in [b"", b"mif1", main * 2, b"zzzzavif"]:
                for extended in [False, True]:
                    body = main + minor + compatible
                    ftyp = struct.pack(">I4sQ", 1, b"ftyp", len(body) + 16) if extended else struct.pack(">I4s", len(body) + 8, b"ftyp")
                    data = ftyp + body
                    cases.extend(data[:i] for i in range(len(data) + 1))
    for size in list(range(33)) + [0xffffffff, 0x80000000]:
        for box in [b"ftyp", b"free", b"uuid", b"zzzz"]:
            cases.append(struct.pack(">I4s", size, box) + b"avif\0\0\0\0mif1")
    for size in [0, 1, 7, 8, 15, 16, 20, 24, 28, (1 << 60) - 1, 1 << 60, (1 << 64) - 1]:
        cases.append(struct.pack(">I4sQ", 1, b"ftyp", size) + b"avif\0\0\0\0mif1")
    for count in [999, 1000, 1001, 1024]:
        cases.append(struct.pack(">I", 16 + count * 4) + b"ftypavif\0\0\0\0" + b"mif1" * count)
    rng = random.Random(0x4e14f594)
    for _ in range(10000):
        data = bytearray(rng.randbytes(rng.randrange(0, 257)))
        if len(data) >= 16:
            data[:4] = struct.pack(">I", len(data))
            data[4:8] = b"ftyp"
            if rng.randrange(2):
                for _ in range(rng.randrange(1, 5)):
                    data[rng.randrange(len(data))] = rng.randrange(256)
        cases.append(bytes(data))
    return list(dict.fromkeys(cases))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--source", default="tests/upstream")
    p.add_argument("--reference-build", required=True)
    p.add_argument("--candidate", default="target/release/libheifer.so")
    p.add_argument("--output", default=".build/brands-report.json")
    args = p.parse_args()
    build = Path(".build/brands").resolve()
    build.mkdir(parents=True, exist_ok=True)
    source = Path(args.source).resolve()
    reference = Path(args.reference_build).resolve()
    include = build / "include/libheif"
    include.mkdir(parents=True, exist_ok=True)
    (include / "heif_version.h").write_bytes((reference / "libheif/heif_version.h").read_bytes())
    compile_common = ["cc", "-O2", "-std=c11", "-Werror", "-Wno-deprecated-declarations", f"-I{source / 'libheif/api'}", f"-I{include.parent}", "tests/brands.c"]
    libraries = {"reference": reference / "libheif/libheif.so", "candidate": Path(args.candidate).resolve()}
    cases = corpus()
    payload = b"".join(struct.pack("=I", len(data)) + data for data in cases)
    results = {}
    for name, library in libraries.items():
        binary = build / name
        subprocess.run(compile_common + [str(library), f"-Wl,-rpath,{library.parent}", "-o", str(binary)], check=True)
        run = subprocess.run([str(binary)], input=payload, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120, check=True)
        if run.stderr:
            (build / f"{name}.stderr").write_bytes(run.stderr)
        results[name] = run.stdout.splitlines()
        (build / f"{name}.txt").write_bytes(run.stdout)
        if len(results[name]) != len(cases) + 1:
            raise SystemExit(f"{name}: incomplete transcript")
    mismatches = []
    for index, (a, b) in enumerate(zip(results["reference"], results["candidate"], strict=True)):
        if a != b:
            mismatches.append({"case": index - 1, "input": cases[index - 1].hex() if index else "version", "reference": a.decode(), "candidate": b.decode()})
    report = {"scope": "version and brand APIs only; not whole-library compatibility", "cases": len(cases), "seed": "0x4e14f594", "corpus_sha256": hashlib.sha256(payload).hexdigest(), "reference": "1.23.4", "reference_sha256": hashlib.sha256(libraries["reference"].read_bytes()).hexdigest(), "candidate_sha256": hashlib.sha256(libraries["candidate"].read_bytes()).hexdigest(), "mismatches": len(mismatches), "examples": mismatches[:20]}
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "examples"}, indent=2))
    if mismatches:
        print(json.dumps(mismatches[:3], indent=2))
        raise SystemExit(1)


if __name__ == "__main__":
    main()
