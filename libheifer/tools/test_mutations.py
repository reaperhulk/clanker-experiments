#!/usr/bin/env python3
"""Check that independent tests reject deliberate semantic and ABI defects.

Mutations are built in an isolated source copy. A compiler failure or a crashed
client is not counted as a detected behavioral difference.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

MUTATIONS = [
    ("filetype_enum", "src/brands.rs", "Supported = 1,", "Supported = 7,", "brands"),
    ("error_code", "src/error.rs", 'Self::new(5, 2001, c"NULL argument passed")', 'Self::new(2, 2001, c"NULL argument passed")', "brands"),
    ("plane_pixels", "src/image.rs", "storage.resize(allocation, 0);", "storage.resize(allocation, 0);\n        storage[..16].fill(1);", "images"),
    ("primary_coordinate", "src/color.rs", "color_primary_red_x: rx,", "color_primary_red_x: rx + 0.0001,", "color"),
    ("primary_id", "crates/capi/src/context.rs", "out.write(doc.primary)", "out.write(doc.primary.wrapping_add(1))", "context"),
    ("alpha_reload_state", "crates/capi/src/context.rs", ".is_some_and(|i| i.has_alpha)", ".is_some_and(|_| handle.image().has_alpha)", "context"),
    ("grid_worker_callbacks", "src/decoding.rs", "if options.max_decoding_threads > 0 {", "if false {", "decode_derived"),
    ("warning_text", "crates/capi/src/image.rs", "libheifer::error_text::message(error.code, error.subcode)", 'String::from("wrong warning text")', "warnings"),
    ("mask_samples", "src/mask.rs", ".copy_from_slice(&data[y * row_bytes..(y + 1) * row_bytes]);", ".copy_from_slice(&data[y * row_bytes..(y + 1) * row_bytes]);\n        plane.data_mut()[target] ^= 1;", "decode_mask"),
    ("overlay_alpha", "src/overlay.rs", "((src * a + dst * (255 - a)) / 255)", "((src * a + dst * (255 - a)) / 256)", "decode_graphs"),
    ("derived_operation_budget", "src/decoding.rs", ".saturating_mul(2)", ".saturating_mul(3)", "decode_graphs"),
    ("live_memory_budget", "src/security.rs", ".checked_add(amount)", ".checked_add(0)", "security"),
    ("auxiliary_filter", "crates/capi/src/auxiliary.rs", "filter & 2 == 0", "filter & 1 == 0", "auxiliary"),
    ("depth_value", "src/auxiliary.rs", "exponent - 31", "exponent - 30", "auxiliary"),
    ("property_ids", "crates/capi/src/properties.rs", "out.add(n as usize).write(index as u32 + 1)", "out.add(n as usize).write(index as u32 + 2)", "properties"),
    ("property_raw_class", "src/properties.rs", "None => p.raw,", "None => true,", "properties"),
    ("description_terminator", "src/properties.rs", ".unwrap_or(data.len().saturating_sub(1))", ".unwrap_or(data.len())", "properties"),
    ("property_crop_origin", "crates/capi/src/properties.rs", "l as c_int,", "(l + 1) as c_int,", "properties"),
    ("coded_size_limit", "src/decoding.rs", ".max(65536)", ".max(65535)", "hevc_limits"),
    ("error_field_order", "crates/capi/src/lib.rs", "pub code: c_int,\n    pub subcode: c_int,", "pub subcode: c_int,\n    pub code: c_int,", "abi"),
]


def execute(command, cwd, env, log):
    run = subprocess.run(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180)
    log.write_bytes(run.stdout)
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-build", required=True)
    parser.add_argument("--candidate", default="target/release/libheifer.so")
    parser.add_argument("--output", default=".build/mutations-report.json")
    parser.add_argument("--only", choices=[m[0] for m in MUTATIONS], action="append", help="Run selected defects; default runs the complete mutation set")
    args = parser.parse_args()
    mutations = [m for m in MUTATIONS if args.only is None or m[0] in args.only]
    root = Path.cwd()
    evidence = root / ".build/mutations"
    evidence.mkdir(parents=True, exist_ok=True)
    reference = str(Path(args.reference_build).resolve())
    candidate = str(Path(args.candidate).resolve())
    # Baselines must pass on this tree before a rejected mutant is meaningful.
    for suite in dict.fromkeys(m[4] for m in mutations if m[4] != "abi"):
        run = execute([sys.executable, f"tools/test_{suite}.py", "--reference-build", reference, "--candidate", candidate, *(["--work", str(evidence / "decode")] if suite.startswith("decode_") or suite == "hevc_limits" else []), "--output", str(evidence / f"baseline-{suite}.json")], root, os.environ, evidence / f"baseline-{suite}.log")
        if run.returncode:
            raise SystemExit(f"Baseline {suite} failed; see {evidence}")
    run = execute(["cargo", "test", "--locked", "-p", "libheifer-capi", "--test", "abi"], root, os.environ, evidence / "baseline-abi.log")
    if run.returncode:
        raise SystemExit("Baseline ABI test failed")
    results = []
    with tempfile.TemporaryDirectory(prefix="mutant-", dir=root / ".build") as temporary:
        clone = Path(temporary)
        for name in ("Cargo.toml", "Cargo.lock"):
            shutil.copy2(root / name, clone / name)
        for name in ("src", "crates", "examples", "vendor"):
            shutil.copytree(root / name, clone / name)
        (clone / "tests").mkdir()
        (clone / "tests/upstream").symlink_to(root / "tests/upstream", target_is_directory=True)
        env = dict(os.environ, CARGO_TARGET_DIR=str(clone / "target"))
        for name, file, before, after, suite in mutations:
            path = clone / file
            original = path.read_text()
            if original.count(before) != 1:
                raise SystemExit(f"Mutation anchor drift: {name}")
            path.write_text(original.replace(before, after))
            try:
                build = execute(["cargo", "build", "--locked", "--release", "-p", "libheifer-capi"], clone, env, evidence / f"{name}-build.log")
                if build.returncode:
                    raise SystemExit(f"Mutation did not compile: {name}")
                library = clone / "target/release/libheifer.so"
                record = {"mutation": name, "suite": suite, "source": file, "before": before, "after": after, "candidate_sha256": hashlib.sha256(library.read_bytes()).hexdigest()}
                if suite == "abi":
                    run = execute(["cargo", "test", "--locked", "-p", "libheifer-capi", "--test", "abi"], clone, env, evidence / f"{name}.log")
                    record["detected"] = run.returncode == 101 and b"public_structs_match_original_header_layouts ... FAILED" in run.stdout and b"assertion `left == right` failed" in run.stdout
                else:
                    report_path = evidence / f"{name}.json"
                    report_path.unlink(missing_ok=True)
                    run = execute([sys.executable, f"tools/test_{suite}.py", "--reference-build", reference, "--candidate", str(library), *(["--work", str(evidence / "decode")] if suite.startswith("decode_") or suite == "hevc_limits" else []), "--output", str(report_path)], root, os.environ, evidence / f"{name}.log")
                    report = json.loads(report_path.read_text()) if report_path.exists() else {}
                    record["mismatches"] = report.get("mismatches", 0)
                    record["detected"] = run.returncode == 1 and record["mismatches"] > 0
                results.append(record)
                print(json.dumps(record), flush=True)
            finally:
                path.write_text(original)
    report = {"scope": f"{len(mutations)} deliberate defects; not comprehensive mutation coverage", "mutations": results, "all_detected": all(r["detected"] for r in results)}
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    if not report["all_detected"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
