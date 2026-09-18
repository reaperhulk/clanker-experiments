#!/usr/bin/env python3
"""Compare native and default-converted HEVC color samples independently."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

FIXTURES = ["examples/example.heic", "fuzzing/data/corpus/colors-no-alpha.heic", "fuzzing/data/corpus/colors-with-alpha.heic", "fuzzing/data/corpus/colors-no-alpha-thumbnail.heic", "fuzzing/data/corpus/colors-with-alpha-thumbnail.heic"]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--reference-build", required=True)
    p.add_argument("--source", default="tests/upstream")
    p.add_argument("--candidate", default="target/release/examples/hevc_probe")
    p.add_argument("--output", default=".build/hevc-report.json")
    p.add_argument("--require-default-output", action="store_true")
    args = p.parse_args()
    source, reference = Path(args.source).resolve(), Path(args.reference_build).resolve()
    work = Path(".build/hevc").resolve()
    include = work / "include/libheif"
    include.mkdir(parents=True, exist_ok=True)
    (include / "heif_version.h").write_bytes((reference / "libheif/heif_version.h").read_bytes())
    library = reference / "libheif/libheif.so"
    binary = work / "reference"
    subprocess.run(["cc", "-O2", "-std=c11", "-Werror", f"-I{source / 'libheif/api'}", f"-I{include.parent}", "tests/hevc_reference.c", str(library), f"-Wl,-rpath,{library.parent}", "-o", str(binary)], check=True)
    results = []
    for index, fixture in enumerate(FIXTURES):
        record = {"fixture": fixture, "sha256": sha((source / fixture).read_bytes())}
        outputs = {}
        for mode in ["reference_native", "reference_default", "candidate_native", "candidate_default"]:
            output = work / f"{index}-{mode}.bin"
            cmd = [str(Path(args.candidate).resolve()) if mode.startswith("candidate") else str(binary), str(source / fixture), str(output)]
            if mode == "reference_native": cmd.append("native")
            if mode == "candidate_default": cmd.append("default")
            output.unlink(missing_ok=True)
            run = subprocess.run(cmd, capture_output=True, timeout=60)
            record[mode + "_status"] = run.returncode
            if run.returncode:
                record[mode + "_error"] = run.stderr.decode(errors="replace")[:1000]
            else:
                outputs[mode] = output.read_bytes()
                record[mode + "_sha256"] = sha(outputs[mode])
        record["native_match"] = "candidate_native" in outputs and "reference_native" in outputs and outputs["candidate_native"] == outputs["reference_native"]
        record["default_output_match"] = "candidate_default" in outputs and "reference_default" in outputs and outputs["candidate_default"] == outputs["reference_default"]
        results.append(record)
    report = {"scope": "direct HEVC items; transformations disabled; Y/Cb/Cr planes, dimensions, bit depths and strides; alpha is NOT compared", "complete_decode_compatibility": False, "native_matches": sum(r["native_match"] for r in results), "default_output_matches": sum(r["default_output_match"] for r in results), "reference_sha256": sha(library.read_bytes()), "candidate_sha256": sha(Path(args.candidate).read_bytes()), "fixtures": results}
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    if any(not r["native_match"] for r in results) or (args.require_default_output and any(not r["default_output_match"] for r in results)):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
