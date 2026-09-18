#!/usr/bin/env python3
"""Validate the pinned pyOpenSSL migration using a completed nox environment."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import tomllib
from validate import ROOT, git, run, source_hash


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cryptography", type=Path, required=True)
    parser.add_argument("--pyopenssl", type=Path, required=True)
    parser.add_argument(
        "--row",
        choices=["system", "openssl4", "libressl", "boringssl", "awslc"],
        required=True,
    )
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    crypto, upstream, output = (
        args.cryptography.resolve(),
        args.pyopenssl.resolve(),
        args.out.resolve(),
    )
    output.mkdir(parents=True, exist_ok=True)
    pins = json.loads((ROOT / "compatibility/sources.json").read_text())
    for directory, name in [(crypto, "cryptography"), (upstream, "pyopenssl")]:
        if git(directory, "rev-parse", "HEAD").decode().strip() != pins[name]:
            raise SystemExit(
                f"{name}: checkout does not match the recorded upstream pin"
            )
    original = {
        name: git(directory, "diff", "HEAD", "--binary")
        for directory, name in [(crypto, "cryptography"), (upstream, "pyopenssl")]
    }
    before = source_hash()
    report = {
        "row": args.row,
        "source_sha256": before,
        "patch_sha256": {
            name: hashlib.sha256(data).hexdigest() for name, data in original.items()
        },
        "commits": {name: pins[name] for name in original},
        "checks": [],
    }
    python = crypto / ".nox/local/bin/python"
    if not python.is_file():
        raise SystemExit(
            "Run the full cryptography nox check before pyOpenSSL validation"
        )
    env = os.environ.copy()
    # The runner may need a custom CA to install build dependencies. Do not let
    # it replace the native library's trust paths in the upstream TLS tests.
    env.update(PYTHONPATH=str(upstream / "src"), PYTEST_DISABLE_PLUGIN_AUTOLOAD="1")

    def check(name: str, command: list[str], environment: dict[str, str] = env) -> bool:
        item = run(command, upstream, environment, output / f"pyopenssl-{name}.log")
        report["checks"].append(item)
        return item["exit_code"] == 0

    installed = check(
        "dependencies",
        ["uv", "pip", "install", "--python", str(python), "pytest-rerunfailures==16.7"],
    )
    # Physically remove the old native binding implementation, not just imports
    # in these tests. The migration must run without either CFFI package.
    removed = check(
        "remove-cffi",
        ["uv", "pip", "uninstall", "--python", str(python), "cffi", "pycparser"],
    )
    for name in ("SSL_CERT_FILE", "SSL_CERT_DIR"):
        env.pop(name, None)
    runtime = check(
        "runtime",
        [
            str(python),
            "-c",
            """
import hashlib, importlib.util, importlib.metadata, json
from pathlib import Path
import cryptography.hazmat.bindings._rust as rust
import OpenSSL.debug
assert importlib.util.find_spec('cffi') is None
assert importlib.util.find_spec('_cffi_backend') is None
assert not hasattr(rust, '_openssl')
assert not any(r.lower().startswith('cffi') for r in importlib.metadata.requires('cryptography'))
print(json.dumps({'extension_sha256': hashlib.sha256(Path(rust.__file__).read_bytes()).hexdigest()}))
print(OpenSSL.debug._env_info)
""",
        ],
    )
    junit = output / "pyopenssl-tests.xml"
    if installed and removed and runtime:
        for name, command in [
            ("ruff", [str(python.parent / "ruff"), "check", "src", "tests"]),
            (
                "format",
                [str(python.parent / "ruff"), "format", "--check", "src", "tests"],
            ),
            ("mypy", [str(python.parent / "mypy"), "src", "tests"]),
            (
                "tests",
                [
                    str(python),
                    "-m",
                    "pytest",
                    "-p",
                    "pytest_rerunfailures",
                    "-v",
                    "-o",
                    "faulthandler_timeout=60",
                    f"--junitxml={junit}",
                ],
            ),
        ]:
            check(name, command)
    report["unchanged_during_run"] = before == source_hash() and all(
        original[name] == git(directory, "diff", "HEAD", "--binary")
        for directory, name in [(crypto, "cryptography"), (upstream, "pyopenssl")]
    )
    forbidden = {"openssl", "openssl-sys", "cryptography-openssl", "cryptography-cffi"}
    lock = tomllib.loads((crypto / "Cargo.lock").read_text())
    report["remaining_original_dependencies"] = sorted(
        p["name"] for p in lock["package"] if p["name"] in forbidden
    )
    outcomes = {}
    if junit.is_file():
        for case in ET.parse(junit).findall(".//testcase"):
            key = f"{case.get('classname')}::{case.get('name')}"
            value = (
                "failed"
                if case.find("failure") is not None or case.find("error") is not None
                else "skipped"
                if case.find("skipped") is not None
                else "passed"
            )
            if outcomes.get(key) != "failed":
                outcomes[key] = value
    baseline = json.loads((ROOT / "validation/tls/upstream-pyopenssl.json").read_text())
    expected = baseline["rows"][args.row]["test_outcomes"]
    report.update(
        test_outcomes=outcomes,
        missing_tests=sorted(set(expected) - set(outcomes)),
        new_skips=sorted(
            k
            for k in expected.keys() & outcomes.keys()
            if outcomes[k] == "skipped" and expected[k] != "skipped"
        ),
        added_tests=sorted(set(outcomes) - set(expected)),
    )
    report["validation_passed"] = (
        all(c["exit_code"] == 0 for c in report["checks"])
        and report["unchanged_during_run"]
        and not report["remaining_original_dependencies"]
        and not report["missing_tests"]
        and not report["new_skips"]
        and bool(outcomes)
        and "failed" not in outcomes.values()
    )
    (output / "pyopenssl-report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["validation_passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
