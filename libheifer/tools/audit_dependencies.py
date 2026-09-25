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
    ("hayro-jpeg2000", "0.4.0"),
    ("jpeg-decoder", "0.3.2"),
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
    ('rusty_h264-decoder', '0.16.0'), ('rusty_h264-common', '0.16.0'),
    ('wide', '0.7.33'), ('safe_arch', '0.7.4'), ('bytemuck', '1.25.2'),
    ('libm', '0.2.16'), ('once_cell', '1.21.4'), ('portable-atomic', '1.15.0'),
    ('fearless_simd', '1.0.0'), ('spin', '0.12.3'),
}

# rav1e (AV1 encoder; vendored Rust-only with av-scenechange) and its Rust graph.
# wasm-bindgen* are wasm32-only target dependencies that never build natively.
REVIEWED |= {
    ('aligned', '0.4.3'), ('aligned-vec', '0.6.4'), ('anyhow', '1.0.104'), ('arg_enum_proc_macro', '0.3.4'), ('arrayvec', '0.7.8'), ('as-slice', '0.2.1'), ('autocfg', '1.5.1'), ('av-scenechange', '0.14.1'), ('av1-grain', '0.2.5'), ('bitstream-io', '4.10.0'), ('built', '0.8.1'), ('bumpalo', '3.20.3'), ('crossbeam-deque', '0.8.8'), ('crossbeam-epoch', '0.9.21'), ('crossbeam-utils', '0.8.23'), ('either', '1.18.0'), ('equator', '0.4.2'), ('equator-macro', '0.4.2'), ('itertools', '0.14.0'), ('log', '0.4.34'), ('maybe-rayon', '0.1.1'), ('memchr', '2.8.3'), ('new_debug_unreachable', '1.0.6'), ('no_std_io2', '0.9.4'), ('nom', '8.0.0'), ('noop_proc_macro', '0.3.0'), ('num-bigint', '0.4.8'), ('num-derive', '0.4.2'), ('num-integer', '0.1.47'), ('num-rational', '0.4.2'), ('num-traits', '0.2.19'), ('pastey', '0.1.1'), ('profiling', '1.0.18'), ('profiling-procmacros', '1.0.18'), ('rav1e', '0.8.1'), ('rayon', '1.12.0'), ('rayon-core', '1.13.0'), ('scan_fmt', '0.2.6'), ('simd_helpers', '0.1.0'), ('stable_deref_trait', '1.2.1'), ('syn', '3.0.6'), ('thiserror', '2.0.21'), ('thiserror-impl', '2.0.21'), ('v_frame', '0.3.9'), ('wasm-bindgen', '0.2.128'), ('wasm-bindgen-macro', '0.2.128'), ('wasm-bindgen-macro-support', '0.2.128'), ('wasm-bindgen-shared', '0.2.128'), ('y4m', '0.8.0'),
}
# hpvca (HEVC encoder, crates.io as published; no dependencies). Its default
# avx/neon features select core::arch intrinsic kernels behind runtime detection.
REVIEWED |= {('hpvca', '0.1.17')}
# `links` keys that only guard against duplicate crate versions (no native library).
MARKER_LINKS = {('rayon-core', '1.13.0'), ('wasm-bindgen-shared', '0.2.128')}

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
        if package.get("links") and name not in MARKER_LINKS:
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
        if name[0] in ('rav1e', 'av-scenechange'):
            # Vendored Rust-only trees: no assembly sources or native build path.
            expected = {'capi', 'scan_fmt', 'threading'} if name[0] == 'rav1e' else set()
            if set(features[package['id']]) != expected:
                problems.append(f'{name[0]} must use only the reviewed Rust configuration')
            if any(p.suffix.lower() in ('.c', '.cc', '.cpp', '.s', '.asm', '.h') for p in root.rglob('*')):
                problems.append(f'Native implementation source in {name[0]} vendor tree')
        if name[0] == 'hpvca':
            if set(features[package['id']]) != {'avx', 'default', 'neon'}:
                problems.append('hpvca must use only its reviewed default features')
            if any(p.suffix.lower() in ('.c', '.cc', '.cpp', '.s', '.asm') for p in root.rglob('*')) or (root / 'build.rs').exists():
                problems.append('Native source or build script in hpvca')
        if name[0] == 'hayro-jpeg2000':
            if set(features[package['id']]) != {'std'}:
                problems.append('hayro-jpeg2000 must use only the reviewed scalar Rust implementation')
            if any(p.suffix.lower() in ('.c', '.cc', '.cpp', '.s', '.asm') for p in root.rglob('*')):
                problems.append('Native implementation source in hayro-jpeg2000 vendor tree')
        if name[0] in ('rusty_h264-decoder', 'rusty_h264-common'):
            # no_std + libm: no accel kernels, global allocator, environment knobs or
            # threads; `simd-detect` only enables fearless_simd's runtime detection.
            if set(features[package['id']]) != {'libm', 'simd-detect'}:
                problems.append(f'{name[0]} must use only the reviewed no_std Rust SIMD configuration')
            if any(p.suffix.lower() in ('.c', '.cc', '.cpp', '.s', '.asm') for p in root.rglob('*')):
                problems.append(f'Native implementation source in {name[0]} vendor tree')
        if name[0] == 'fearless_simd':
            # Pure Rust core::arch wrappers; `std` is used for CPU feature detection.
            if set(features[package['id']]) != {'libm', 'std'}:
                problems.append('fearless_simd must use only the reviewed libm/std features')
            if any(p.suffix.lower() in ('.c', '.cc', '.cpp', '.s', '.asm') for p in root.rglob('*')) or (root / 'build.rs').exists():
                problems.append('Native source or build script in fearless_simd')
        if name[0] == 'spin':
            # Pure Rust no_std locks for the vendored decoder's shared state.
            if set(features[package['id']]) != {'mutex', 'once', 'rwlock', 'spin_mutex'}:
                problems.append('spin must use only the reviewed lock features')
            if any(p.suffix.lower() in ('.c', '.cc', '.cpp', '.s', '.asm') for p in root.rglob('*')) or (root / 'build.rs').exists():
                problems.append('Native source or build script in spin')
        if name[0] == 'jpeg-decoder':
            if set(features[package['id']]) != {'platform_independent'}:
                problems.append('jpeg-decoder must use only the reviewed scalar Rust implementation')
            if any(p.suffix.lower() in ('.c', '.cc', '.cpp', '.s', '.asm') for p in root.rglob('*')):
                problems.append('Native implementation source in jpeg-decoder vendor tree')
        report.append({"name": name[0], "version": name[1], "source": package["source"], "features": features[package["id"]]})
    Path(".build").mkdir(exist_ok=True)
    Path(".build/dependencies.json").write_text(json.dumps({"packages": report, "problems": problems}, indent=2) + "\n")
    print(json.dumps({"packages": report, "problems": problems}, indent=2))
    if problems:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
