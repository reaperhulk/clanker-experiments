#!/usr/bin/env python3
"""Fail closed on absent symbols or behavioral evidence. Not an equivalence proof."""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess


def exports(path):
    return {line.split()[-1] for line in subprocess.check_output(["nm", "-D", "--defined-only", str(path)], text=True).splitlines() if line.split()[-1].startswith("heif_")}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--candidate", default="target/release/libheifer.so")
    p.add_argument("--reference")
    p.add_argument("--development", action="store_true", help="permit missing APIs; still reject untracked exports and stale test references")
    p.add_argument("--output", default=".build/coverage.json")
    args = p.parse_args()
    inventory = json.loads(Path("compat/api.json").read_text())
    expected = set(inventory["functions"]) | {n for n, v in inventory["variables"].items() if v["exported"]}
    actual = exports(args.candidate)
    coverage = json.loads(Path("compat/coverage.json").read_text())
    problems = []
    if args.reference:
        ref = exports(args.reference)
        if ref != expected:
            problems.append({"reference_inventory_mismatch": {"missing": sorted(expected - ref), "unexpected": sorted(ref - expected)}})
    for name, record in coverage.items():
        if name not in expected or name not in actual:
            problems.append({"stale_coverage_entry": name})
        if record["status"] not in ("partial", "validated"):
            problems.append({"invalid_status": name})
        for test in record["tests"]:
            if not Path(test).is_file():
                problems.append({"missing_test": test})
    missing = sorted(expected - actual)
    unvalidated = sorted(n for n in expected & actual if coverage.get(n, {}).get("status") != "validated")
    unknown = sorted(actual - expected)
    if unknown:
        problems.append({"unexpected_exports": unknown})
    report = {"reference": inventory["reference"], "expected_functions": len(inventory["functions"]), "expected_variables": sum(v["exported"] for v in inventory["variables"].values()), "exported": len(actual), "missing": missing, "unvalidated": unvalidated, "problems": problems, "missing_by_header": dict(Counter((inventory["functions"] | inventory["variables"])[n]["header"] for n in missing)), "complete": not (missing or unvalidated or problems)}
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k not in ("missing", "unvalidated")}, indent=2))
    if problems or (not args.development and not report["complete"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
