#!/usr/bin/env python3
"""Fail closed if the resolved implementation graph grows beyond reviewed Rust crates.

This is a dependency/build guard, not proof from filenames alone. Review new source
before extending REVIEWED. Native test oracles are deliberately outside Cargo.
"""
import json
from pathlib import Path
import subprocess

REVIEWED = {("libheifer", "0.1.0"), ("libheifer-capi", "0.1.0"), ("rusty_h265", "0.6.0"), ("rusty_h265-accel", "0.6.0"), ("zlib-rs", "0.6.8")}


def main():
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version=1", "--all-features"], text=True))
    resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
    features = {node["id"]: node["features"] for node in metadata["resolve"]["nodes"]}
    problems = []
    report = []
    for package in metadata["packages"]:
        if package["id"] not in resolved:
            continue
        name = (package["name"], package["version"])
        if name not in REVIEWED:
            problems.append(f"Unreviewed implementation dependency: {name}")
        if package.get("links"):
            problems.append(f"Native links declaration: {name}")
        if name[0] == "zlib-rs" and ("c-allocator" in features[package["id"]] or "rust-allocator" not in features[package["id"]]):
            problems.append("zlib-rs must use its Rust allocator without the C allocator feature")
        if any("custom-build" in t["kind"] for t in package["targets"]):
            problems.append(f"Unreviewed build script: {name}")
        report.append({"name": name[0], "version": name[1], "source": package["source"], "features": features[package["id"]]})
    Path(".build").mkdir(exist_ok=True)
    Path(".build/dependencies.json").write_text(json.dumps({"packages": report, "problems": problems}, indent=2) + "\n")
    print(json.dumps({"packages": report, "problems": problems}, indent=2))
    if problems:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
