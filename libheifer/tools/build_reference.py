#!/usr/bin/env python3
"""Build pinned native test oracles. These are NOT candidate dependencies."""
import argparse
from pathlib import Path
import shutil
import subprocess

HEIF = "4e14f5942c1732ace9611b9522cc991501445463"
DE265 = "7ba65889d3d6d8a0d99b5360b028243ba843be3a"


def run(*args):
    subprocess.run(list(map(str, args)), check=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--build", default=".build/reference")
    p.add_argument("--source", default="tests/upstream")
    p.add_argument("--hevc", action="store_true")
    p.add_argument("-j", default="4")
    a = p.parse_args()
    source = Path(a.source).resolve()
    build = Path(a.build).resolve()
    sha = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
    if sha != HEIF:
        raise SystemExit("Wrong libheif reference revision")
    cmake = shutil.which("cmake")
    if cmake is None:
        raise SystemExit("cmake required for reference build")
    # Keep the oracle feature set deterministic. Brotli is a separate, still-open
    # compatibility target; do not let host package discovery change transcripts.
    flags = ["-DCMAKE_DISABLE_FIND_PACKAGE_Brotli=ON", "-DCMAKE_REQUIRE_FIND_PACKAGE_ZLIB=ON"]
    if a.hevc:
        decoder = build.parent / "libde265-source"
        install = build.parent / "libde265-install"
        dbuild = build.parent / "libde265-build"
        if not decoder.exists():
            run("git", "init", decoder)
            run("git", "-C", decoder, "remote", "add", "origin", "https://github.com/strukturag/libde265.git")
            run("git", "-C", decoder, "fetch", "--depth=1", "origin", DE265)
            run("git", "-C", decoder, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(decoder), "rev-parse", "HEAD"], text=True).strip()
        if revision != DE265:
            raise SystemExit("Wrong libde265 reference revision")
        run(cmake, "-S", decoder, "-B", dbuild, "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={install}", "-DENABLE_SDL=OFF", "-DENABLE_ENCODER=OFF")
        run(cmake, "--build", dbuild, "-j", a.j)
        run(cmake, "--install", dbuild)
        flags += [f"-DLIBDE265_INCLUDE_DIR={install / 'include'}", f"-DLIBDE265_LIBRARY={install / 'lib/libde265.so'}"]
    run(cmake, "-S", source, "-B", build, "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_TESTING=OFF", "-DBUILD_DOCUMENTATION=OFF", "-DWITH_EXAMPLES=OFF", "-DWITH_GDK_PIXBUF=OFF", "-DENABLE_PLUGIN_LOADING=OFF", f"-DWITH_LIBDE265={'ON' if a.hevc else 'OFF'}", "-DWITH_X265=OFF", "-DWITH_X264=OFF", "-DWITH_OpenH264_DECODER=OFF", "-DWITH_AOM_DECODER=OFF", "-DWITH_AOM_ENCODER=OFF", "-DWITH_LIBSHARPYUV=OFF", "-DWITH_UNCOMPRESSED_CODEC=ON", *flags)
    run(cmake, "--build", build, "-j", a.j)


if __name__ == "__main__":
    main()
