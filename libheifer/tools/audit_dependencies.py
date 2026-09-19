#!/usr/bin/env python3
"""Fail closed if the resolved implementation graph grows beyond reviewed Rust crates.

This is a dependency/build guard, not proof from filenames alone. Review new source
before extending REVIEWED. Native test oracles are deliberately outside Cargo.
"""
import json
import hashlib
from pathlib import Path
import subprocess

REVIEWED = {("libheifer", "0.1.0"), ("libheifer-capi", "0.1.0"), ("rusty_h265", "0.6.0"), ("rusty_h265-accel", "0.6.0"), ("zlib-rs", "0.6.8")}
REVIEWED |= {
    ('rav1d', '1.1.0'), ('assert_matches', '1.5.0'), ('atomig', '0.4.3'),
    ('atomig-macro', '0.4.0'), ('bitflags', '2.13.2'), ('byteorder', '1.5.0'),
    ('cfg-if', '1.0.5'), ('heck', '0.5.0'), ('libc', '0.2.189'),
    ('lock_api', '0.4.14'), ('parking_lot', '0.12.5'), ('parking_lot_core', '0.9.12'),
    ('paste', '1.0.15'), ('proc-macro2', '1.0.107'), ('quote', '1.0.47'),
    ('raw-cpuid', '11.6.0'), ('redox_syscall', '0.5.18'), ('rustversion', '1.0.23'),
    ('scopeguard', '1.2.0'), ('smallvec', '1.16.1'), ('strum', '0.26.3'),
    ('strum_macros', '0.26.4'), ('syn', '2.0.119'), ('to_method', '1.1.0'),
    ('unicode-ident', '1.0.26'), ('windows-link', '0.2.1'),
    ('zerocopy', '0.7.35'), ('zerocopy-derive', '0.7.35'),
}


def main():
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version=1", "--all-features"], text=True))
    resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
    features = {node["id"]: node["features"] for node in metadata["resolve"]["nodes"]}
    problems = []
    report = []
    build_scripts = json.loads(Path('docs/dependency-build-scripts.json').read_text())
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
        root = Path(package['manifest_path']).parent
        approved = build_scripts.get('@'.join(name), {})
        for target in package['targets']:
            if 'custom-build' in target['kind'] and str(Path(target['src_path']).relative_to(root)) not in approved:
                problems.append(f"Unreviewed build script: {name}")
        for file, expected in approved.items():
            if not (root/file).is_file() or hashlib.sha256((root/file).read_bytes()).hexdigest() != expected:
                problems.append(f"Reviewed build source changed: {name} {file}")
        if name[0] == 'rav1d':
            if set(features[package['id']]) != {'bitdepth_8', 'bitdepth_16'}:
                problems.append('rav1d must enable only Rust 8/16-bit implementations')
            if any(p.suffix.lower() in ('.c', '.cc', '.cpp', '.s', '.asm') for p in root.rglob('*')):
                problems.append('Native implementation source in rav1d vendor tree')
        report.append({"name": name[0], "version": name[1], "source": package["source"], "features": features[package["id"]]})
    Path(".build").mkdir(exist_ok=True)
    Path(".build/dependencies.json").write_text(json.dumps({"packages": report, "problems": problems}, indent=2) + "\n")
    print(json.dumps({"packages": report, "problems": problems}, indent=2))
    if problems:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
