#!/usr/bin/env python3
"""Pinned inputs and native builds shared by the GitHub Actions matrix."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import subprocess
import sysconfig
import tarfile
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES = json.loads((ROOT / "compatibility/sources.json").read_text())
MATRIX = json.loads((ROOT / "compatibility/matrix.json").read_text())
ROWS = {row["id"]: row for row in MATRIX["rows"]}
ROWS["msrv-openssl4"] = dict(ROWS["openssl4"], id="msrv-openssl4", rust="1.83.0")
REPOSITORIES = {
    "openssl": "https://github.com/openssl/openssl.git",
    "boringssl": "https://github.com/google/boringssl.git",
    "awslc": "https://github.com/aws/aws-lc.git",
    "cryptography": "https://github.com/pyca/cryptography.git",
    "pyopenssl": "https://github.com/pyca/pyopenssl.git",
    "wycheproof": "https://github.com/C2SP/wycheproof.git",
    "x509-limbo": "https://github.com/C2SP/x509-limbo.git",
}


def run(command: list[str], cwd: Path) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def checkout(repository: str, revision: str, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    if not (destination / ".git").exists():
        run(["git", "init", "--quiet"], destination)
        run(["git", "remote", "add", "origin", repository], destination)
        run(["git", "fetch", "--depth=1", "origin", revision], destination)
        run(["git", "checkout", "--detach", "FETCH_HEAD"], destination)
    actual = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=destination, text=True
    ).strip()
    if actual != revision:
        raise SystemExit(f"{destination}: expected {revision}, found {actual}")


def native_spec(row: dict) -> dict:
    backend = row["backend"]
    if row["id"] == "system":
        return {"backend": backend, "system": True}
    version = row["version_or_revision"]
    if backend in {"boringssl", "awslc"}:
        pin = SOURCES["backend_builds"][backend]
    elif backend == "libressl":
        pin = (
            SOURCES["backend_builds"][backend]
            if version == "4.3.2"
            else SOURCES["additional_backend_builds"][f"libressl-{version}"]
        )
    elif version == "4.0.2":
        pin = SOURCES["backend_builds"][backend]
    elif version == "3.6.4":
        pin = SOURCES["additional_backend_builds"]["3.6.4-fips"]
    elif version == SOURCES["additional_backend_builds"]["main"]["commit"]:
        pin = SOURCES["additional_backend_builds"]["main"]
    else:
        pin = SOURCES["additional_backend_builds"][version]
    return {
        "backend": backend,
        "system": False,
        "pin": pin,
        "flags": row["extra_native_flags"],
        "common_flags": SOURCES["openssl_common_configure_flags"],
    }


def native_plan(row: dict, cache: Path) -> dict:
    spec = native_spec(row)
    # Include the complete build recipe, platform, and pins. Rust versions and
    # runtime legacy/FIPS selections can safely share the same native artifact.
    fingerprint = hashlib.sha256(
        json.dumps(spec, sort_keys=True).encode() + Path(__file__).read_bytes()
    ).hexdigest()[:24]
    prefix = Path("/usr") if spec["system"] else cache.resolve() / fingerprint
    return {
        "cache_key": f"openssl-bridge-native-ubuntu24-x64-{fingerprint}",
        "prefix": str(prefix),
        "system": str(spec["system"]).lower(),
        "spec": spec,
    }


def configure_environment(row: dict, plan: dict, destination: Path) -> None:
    values = {
        "OPENSSL_DIR": plan["prefix"],
        "OPENSSL_STATIC": "0" if plan["spec"]["system"] else "1",
        "OPENSSL_CONF": (
            str(Path(plan["prefix"]) / "ssl/openssl.cnf") if row["fips"] else os.devnull
        ),
    }
    if plan["spec"]["system"]:
        values["OPENSSL_LIB_DIR"] = f"/usr/lib/{sysconfig.get_config_var('MULTIARCH')}"
    if "CRYPTOGRAPHY_OPENSSL_NO_LEGACY" in row:
        values["CRYPTOGRAPHY_OPENSSL_NO_LEGACY"] = row["CRYPTOGRAPHY_OPENSSL_NO_LEGACY"]
    with destination.open("a") as output:
        for key, value in values.items():
            output.write(f"{key}={value}\n")


def build(row: dict, plan: dict, work: Path, jobs: int) -> None:
    spec = plan["spec"]
    if spec["system"]:
        return
    prefix = Path(plan["prefix"])
    receipt = prefix / "bridge-build.json"
    if receipt.exists():
        if json.loads(receipt.read_text()) != plan:
            raise SystemExit("native cache receipt does not match this build")
        if (
            not (prefix / "lib/libssl.a").is_file()
            or not (prefix / "lib/libcrypto.a").is_file()
        ):
            raise SystemExit("native cache is incomplete")
        print(f"Using verified native build receipt: {receipt}", flush=True)
        return
    backend, pin = spec["backend"], spec["pin"]
    work = work.resolve() / plan["cache_key"]
    work.mkdir(parents=True, exist_ok=True)
    source = work / "source"
    if backend == "libressl":
        archive = work / "libressl.tar.gz"
        if not archive.exists():
            run(
                [
                    "curl",
                    "--fail",
                    "--location",
                    "--retry",
                    "4",
                    "--retry-all-errors",
                    "--output",
                    str(archive),
                    f"https://ftp.openbsd.org/pub/OpenBSD/LibreSSL/libressl-{pin['version']}.tar.gz",
                ],
                work,
            )
        if hashlib.sha256(archive.read_bytes()).hexdigest() != pin["archive_sha256"]:
            raise SystemExit("LibreSSL archive hash mismatch")
        source = work / f"libressl-{pin['version']}"
        if not source.exists():
            with tarfile.open(archive) as tar:
                tar.extractall(work, filter="data")
    else:
        checkout(REPOSITORIES[backend], pin["commit"], source)
    if backend == "openssl":
        run(
            [
                "./Configure",
                *spec["common_flags"],
                *spec["flags"],
                f"--prefix={prefix}",
                "--libdir=lib",
            ],
            source,
        )
        run(["make", f"-j{jobs}", "build_sw"], source)
        run(["make", "install_sw", "install_ssldirs"], source)
        if "enable-fips" in spec["flags"]:
            run(["make", f"-j{jobs}", "install_fips"], source)
            (prefix / "ssl/openssl.cnf").write_text(
                "config_diagnostics = 1\nopenssl_conf = openssl_init\n"
                f".include {prefix}/ssl/fipsmodule.cnf\n"
                "[openssl_init]\nproviders = provider_sect\nalg_section = algorithm_sect\n"
                "[provider_sect]\nfips = fips_sect\nbase = base_sect\n"
                "[base_sect]\nactivate = 1\n[algorithm_sect]\ndefault_properties = fips=yes\n"
            )
    else:
        extra = (
            ["-DLIBRESSL_APPS=OFF", "-DLIBRESSL_TESTS=OFF"]
            if backend == "libressl"
            else ["-DBUILD_TESTING=OFF"]
        )
        if backend == "awslc":
            extra.append("-DBUILD_TOOL=OFF")
        run(
            [
                "cmake",
                "-S",
                str(source),
                "-B",
                str(work / "build"),
                "-GNinja",
                "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
                "-DBUILD_SHARED_LIBS=OFF",
                "-DCMAKE_BUILD_TYPE=RelWithAsserts",
                f"-DCMAKE_INSTALL_PREFIX={prefix}",
                "-DCMAKE_INSTALL_LIBDIR=lib",
                *extra,
            ],
            work,
        )
        run(["cmake", "--build", str(work / "build"), "--parallel", str(jobs)], work)
        run(["cmake", "--install", str(work / "build")], work)
    receipt.write_text(json.dumps(plan, indent=2) + "\n")


def prepare(work: Path, row: str) -> None:
    work = work.resolve()
    for name in ("cryptography", "wycheproof", "x509-limbo"):
        checkout(REPOSITORIES[name], SOURCES[name], work / name)
    cryptography = work / "cryptography"
    patch = ROOT / "compatibility/cryptography.patch"
    run(["git", "apply", "--check", str(patch)], cryptography)
    # Include new files and all deleted adapter files in the source-identity
    # checks. Do not accidentally validate a patch that drops untracked files.
    run(["git", "apply", "--index", str(patch)], cryptography)
    if row in MATRIX["wrapper_rows"]:
        pyopenssl = work / "pyopenssl"
        checkout(REPOSITORIES["pyopenssl"], SOURCES["pyopenssl"], pyopenssl)
        tls_patch = ROOT / "compatibility/pyopenssl.patch"
        run(["git", "apply", "--check", str(tls_patch)], pyopenssl)
        run(["git", "apply", "--index", str(tls_patch)], pyopenssl)
    acceptance = ROOT / "validation/acceptance"
    report = json.loads((acceptance / "report.json").read_text())
    accepted = next(item for item in report["rows"] if item["label"] == row)
    baseline = accepted["baseline_junit"]
    data = gzip.decompress((acceptance / baseline["file"]).read_bytes())
    if hashlib.sha256(data).hexdigest() != baseline["uncompressed_sha256"]:
        raise SystemExit("archived upstream baseline hash mismatch")
    cases = ET.fromstring(data).findall(".//testcase")
    outcomes = {
        f"{case.get('classname')}::{case.get('name')}": "skipped"
        if case.find("skipped") is not None
        else "passed"
        for case in cases
    }
    if not outcomes:
        raise SystemExit("archived baseline contains no test cases")
    (work / "baseline.json").write_text(json.dumps({"test_outcomes": outcomes}) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["matrix", "plan", "build", "prepare"])
    parser.add_argument("--row", choices=ROWS)
    parser.add_argument("--cache", type=Path, default=Path("native-backends"))
    parser.add_argument("--work", type=Path, default=Path("native-sources"))
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--github-env", type=Path)
    args = parser.parse_args()
    if args.command == "matrix":
        matrix = {
            "library": {
                "include": [
                    ROWS[key]
                    for key in [
                        *MATRIX["wrapper_rows"],
                        "msrv-openssl4",
                        "msrv-boringssl",
                        "msrv-awslc",
                    ]
                ]
            },
            "integration": {"include": MATRIX["rows"]},
        }
        if args.github_output:
            with args.github_output.open("a") as output:
                for key, value in matrix.items():
                    output.write(f"{key}={json.dumps(value, separators=(',', ':'))}\n")
        else:
            print(json.dumps(matrix, indent=2))
        return
    if args.row is None:
        parser.error("--row is required")
    if args.command == "prepare":
        prepare(args.work, args.row)
        return
    row = ROWS[args.row]
    plan = native_plan(row, args.cache)
    if args.command == "build":
        build(row, plan, args.work, args.jobs)
    if args.github_output:
        with args.github_output.open("a") as output:
            for key in ("cache_key", "prefix", "system"):
                output.write(f"{key}={plan[key]}\n")
    if args.github_env:
        configure_environment(row, plan, args.github_env)
    print(json.dumps(plan, indent=2))


if __name__ == "__main__":
    main()
