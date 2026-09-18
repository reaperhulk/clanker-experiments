#!/usr/bin/env python3
"""Run a reproducible library + cryptography validation row; never open a PR."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[1]


def source_hash() -> str:
    digest = hashlib.sha256()
    for path in sorted((ROOT / "crates").rglob("*")):
        if path.is_file():
            digest.update(str(path.relative_to(ROOT)).encode())
            digest.update(b"\0")
            digest.update(path.read_bytes())
            digest.update(b"\0")
    for name in ("Cargo.toml", "Cargo.lock"):
        digest.update((ROOT / name).read_bytes())
    return digest.hexdigest()


def git(directory: Path, *args: str) -> bytes:
    return subprocess.check_output(["git", "-C", str(directory), *args])


def run(command: list[str], directory: Path, env: dict[str, str], log: Path) -> dict:
    print(f"Running {' '.join(command)} in {directory}", flush=True)
    started = time.monotonic()
    with log.open("w") as output:
        result = subprocess.run(
            command,
            cwd=directory,
            env=env,
            stdout=output,
            stderr=subprocess.STDOUT,
            check=False,
        )
    print(f"Exit {result.returncode}; log: {log}", flush=True)
    return {
        "command": command,
        "exit_code": result.returncode,
        "elapsed_seconds": round(time.monotonic() - started, 2),
        "log": log.name,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cryptography", type=Path, required=True)
    parser.add_argument("--wycheproof", type=Path, required=True)
    parser.add_argument("--limbo", type=Path, required=True)
    parser.add_argument(
        "--backend",
        choices=["openssl", "libressl", "boringssl", "awslc"],
        required=True,
    )
    parser.add_argument("--openssl-dir", type=Path, required=True)
    parser.add_argument("--openssl-lib-dir", type=Path)
    parser.add_argument("--static", action="store_true")
    parser.add_argument("--fips", action="store_true")
    parser.add_argument("--no-legacy", choices=["0", "1"])
    parser.add_argument(
        "--baseline", action="store_true",
        help="Require an unmodified cryptography checkout and omit wrapper checks",
    )
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--baseline-report",
        type=Path,
        help="Compare Python test identities and skips with an unmodified baseline",
    )
    args = parser.parse_args()
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    sources = json.loads((ROOT / "compatibility/sources.json").read_text())
    for path, key in [
        (args.cryptography, "cryptography"),
        (args.wycheproof, "wycheproof"),
        (args.limbo, "x509-limbo"),
    ]:
        revision = git(path, "rev-parse", "HEAD").decode().strip()
        if revision != sources[key]:
            raise SystemExit(
                f"{key} is {revision}, expected pinned revision {sources[key]}"
            )

    env = os.environ.copy()
    env.update(
        OPENSSL_DIR=str(args.openssl_dir.resolve()),
        OPENSSL_STATIC="1" if args.static else "0",
        CARGO_INCREMENTAL="0",
    )
    if args.openssl_lib_dir:
        env["OPENSSL_LIB_DIR"] = str(args.openssl_lib_dir.resolve())
    else:
        env.pop("OPENSSL_LIB_DIR", None)
    env.pop("OPENSSL_INCLUDE_DIR", None)
    if args.no_legacy is not None:
        env["CRYPTOGRAPHY_OPENSSL_NO_LEGACY"] = args.no_legacy
    env.setdefault("PYTEST_XDIST_AUTO_NUM_WORKERS", "4")
    env["CARGO_TARGET_DIR"] = str(output / "library-target")
    code_hash = source_hash()
    integration_diff = git(args.cryptography, "diff", "HEAD", "--binary")
    if args.baseline and git(args.cryptography, "status", "--porcelain"):
        raise SystemExit("Baseline checkout must have no tracked or untracked changes")
    report = {
        "backend": args.backend,
        "baseline": args.baseline,
        "fips_requested": args.fips,
        "no_legacy": env.get("CRYPTOGRAPHY_OPENSSL_NO_LEGACY"),
        "cryptography_openssl_conf": env.get("OPENSSL_CONF", "native default"),
        "openssl_dir": str(args.openssl_dir.resolve()),
        "source_sha256": code_hash,
        "cryptography_commit": sources["cryptography"],
        "cryptography_patch_sha256": hashlib.sha256(integration_diff).hexdigest(),
        "complete_replacement": False,
        "checks": [],
    }
    if not args.baseline:
        library_env = env.copy()
        if args.fips:
            # The standalone vector suite deliberately exercises non-FIPS
            # algorithms too. Run it in ordinary mode on the same native build;
            # the complete cryptography Python AND Rust suite below retains the
            # startup FIPS configuration and checks its rejection behavior.
            library_env["OPENSSL_CONF"] = os.devnull
        report["library_openssl_conf"] = library_env.get(
            "OPENSSL_CONF", "native default"
        )
        for name, command in [
            ("library", ["cargo", "test", "--workspace", "--locked"]),
            ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
        ]:
            report["checks"].append(
                run(command, ROOT, library_env, output / f"{name}.log")
            )
    env["CARGO_TARGET_DIR"] = str(output / "cryptography-target")
    junit = output / "python-tests.xml"
    report["checks"].append(
        run(
            [
                "nox",
                "-e",
                "local",
                "--",
                f"--wycheproof-root={args.wycheproof.resolve()}",
                f"--x509-limbo-root={args.limbo.resolve()}",
                f"--junitxml={junit}",
                *(["--enable-fips=1"] if args.fips else []),
            ],
            args.cryptography.resolve(),
            env,
            output / "cryptography.log",
        )
    )
    report["unchanged_during_run"] = (
        code_hash == source_hash()
        and integration_diff == git(args.cryptography, "diff", "HEAD", "--binary")
    )
    lock = tomllib.loads((args.cryptography / "Cargo.lock").read_text())
    report["remaining_original_dependencies"] = sorted(
        {p["name"] for p in lock["package"] if p["name"] in {"openssl", "openssl-sys"}}
    )
    if junit.exists():
        cases = ET.parse(junit).findall(".//testcase")
        report["test_outcomes"] = {
            f"{case.get('classname')}::{case.get('name')}": "skipped"
            if case.find("skipped") is not None
            else "failed"
            if case.find("failure") is not None or case.find("error") is not None
            else "passed"
            for case in cases
        }
    if args.baseline_report:
        baseline = json.loads(args.baseline_report.read_text())
        before, after = baseline["test_outcomes"], report.get("test_outcomes", {})
        report["missing_tests"] = sorted(set(before) - set(after))
        report["new_skips"] = sorted(
            k for k in set(before) & set(after)
            if after[k] == "skipped" and before[k] != "skipped"
        )
        report["added_tests"] = sorted(set(after) - set(before))
    report["validation_passed"] = (
        all(c["exit_code"] == 0 for c in report["checks"])
        and report["unchanged_during_run"]
        and not report.get("missing_tests")
        and not report.get("new_skips")
        and (args.baseline or not report["remaining_original_dependencies"])
    )
    # Passing this development row never asserts API completeness or full matrix
    # coverage. The separate primary acceptance requirements still apply.
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["validation_passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
