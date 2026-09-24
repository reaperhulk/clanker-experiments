#!/usr/bin/env python3
"""Build pinned native test oracles. These are NOT candidate dependencies."""
import argparse
from pathlib import Path
import shutil
import os
import subprocess

HEIF = "4e14f5942c1732ace9611b9522cc991501445463"
DE265 = "7ba65889d3d6d8a0d99b5360b028243ba843be3a"
OPENJPEG = "6c4a29b00211eb0430fa0e5e890f1ce5c80f409f"
JPEG = "7723f50f3f66b9da74376e6d8badb6162464212c"
DAV1D = "42b2b24fb8819f1ed3643aa9cf2a62f03868e3aa"
OPENH264 = "652bdb7719f30b52b08e506645a7322ff1b2cc6f"  # v2.6.0
OPENJPH = "8c2826fdaaac3b0334ff5bc2ed2a8ec153c99a35"  # 0.32.0
RAV1E = "1fe82de02510767539e89b2ee6fa846920ae2686"  # v0.8.1


def run(*args):
    subprocess.run(list(map(str, args)), check=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--build", default=".build/reference")
    p.add_argument("--source", default="tests/upstream")
    p.add_argument("--hevc", action="store_true")
    p.add_argument("--av1", action="store_true")
    p.add_argument("--jpeg", action="store_true")
    p.add_argument("--jpeg2000", action="store_true")
    p.add_argument("--avc", action="store_true", help="enable the native OpenH264 decoder oracle (scalar, no assembly)")
    p.add_argument("--avc-asm", action="store_true", help="with --avc: OpenH264 with its assembly kernels (performance baseline only)")
    p.add_argument("--rav1e", action="store_true", help="enable libheif's rav1e AV1 encoder (rav1e C API via cargo-c, no assembly)")
    p.add_argument("--htj2k", action="store_true", help="enable libheif's OpenJPH HTJ2K encoder (scalar)")
    p.add_argument("--plugins", action="store_true", help="enable native dynamic-plugin oracle with an empty default search path")
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
    flags = ["-DCMAKE_DISABLE_FIND_PACKAGE_Brotli=ON", "-DCMAKE_REQUIRE_FIND_PACKAGE_ZLIB=ON",
             f"-DWITH_JPEG_DECODER={'ON' if a.jpeg else 'OFF'}", "-DWITH_JPEG_ENCODER=OFF",
             "-DWITH_JPEG_DECODER_PLUGIN=OFF", "-DWITH_JPEG_ENCODER_PLUGIN=OFF",
             f"-DWITH_OpenJPEG_DECODER={'ON' if a.jpeg2000 else 'OFF'}",
             "-DWITH_OpenJPEG_DECODER_PLUGIN=OFF", f"-DWITH_OpenJPEG_ENCODER={'ON' if a.jpeg2000 else 'OFF'}", "-DWITH_OpenJPEG_ENCODER_PLUGIN=OFF"]
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
    if a.av1:
        decoder = build.parent / "dav1d-source"
        install = build.parent / "dav1d-install"
        dbuild = build.parent / "dav1d-build"
        if not decoder.exists():
            run("git", "init", decoder)
            run("git", "-C", decoder, "remote", "add", "origin", "https://code.videolan.org/videolan/dav1d.git")
            run("git", "-C", decoder, "fetch", "--depth=1", "origin", DAV1D)
            run("git", "-C", decoder, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(decoder), "rev-parse", "HEAD"], text=True).strip()
        if revision != DAV1D:
            raise SystemExit("Wrong dav1d reference revision")
        run("meson", "setup", *(["--reconfigure"] if (dbuild / "build.ninja").exists() else []), dbuild, decoder,
            "--buildtype=release", f"--prefix={install}", "--libdir=lib", "-Denable_tools=false",
            "-Denable_tests=false", "-Denable_asm=false", "-Ddefault_library=shared")
        run("ninja", "-C", dbuild, "-j", a.j)
        run("ninja", "-C", dbuild, "install")
        flags += [f"-DDAV1D_INCLUDE_DIR={install / 'include'}", f"-DDAV1D_LIBRARY={install / 'lib/libdav1d.so'}"]
    if a.jpeg2000:
        decoder = build.parent / "openjpeg-source"
        install = build.parent / "openjpeg-install"
        dbuild = build.parent / "openjpeg-build"
        if not decoder.exists():
            run("git", "init", decoder)
            run("git", "-C", decoder, "remote", "add", "origin", "https://github.com/uclouvain/openjpeg.git")
            run("git", "-C", decoder, "fetch", "--depth=1", "origin", OPENJPEG)
            run("git", "-C", decoder, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(decoder), "rev-parse", "HEAD"], text=True).strip()
        if revision != OPENJPEG:
            raise SystemExit("Wrong OpenJPEG reference revision")
        run(cmake, "-S", decoder, "-B", dbuild, "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={install}",
            "-DCMAKE_INSTALL_LIBDIR=lib", "-DBUILD_CODEC=ON", "-DBUILD_SHARED_LIBS=ON", "-DBUILD_TESTING=OFF")
        run(cmake, "--build", dbuild, "-j", a.j)
        run(cmake, "--install", dbuild)
        flags += [f"-DOpenJPEG_DIR={install / 'lib/cmake/openjpeg-2.5'}"]
    if a.htj2k:
        encoder = build.parent / "openjph-source"
        install = build.parent / "openjph-install"
        ebuild = build.parent / "openjph-lib-build"
        if not encoder.exists():
            run("git", "init", encoder)
            run("git", "-C", encoder, "remote", "add", "origin", "https://github.com/aous72/OpenJPH.git")
            run("git", "-C", encoder, "fetch", "--depth=1", "origin", OPENJPH)
            run("git", "-C", encoder, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(encoder), "rev-parse", "HEAD"], text=True).strip()
        if revision != OPENJPH:
            raise SystemExit("Wrong OpenJPH reference revision")
        run(cmake, "-S", encoder, "-B", ebuild, "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={install}",
            "-DCMAKE_INSTALL_LIBDIR=lib", "-DBUILD_SHARED_LIBS=ON", "-DOJPH_DISABLE_SIMD=ON",
            "-DOJPH_BUILD_EXECUTABLES=OFF", "-DOJPH_ENABLE_TIFF_SUPPORT=OFF")
        run(cmake, "--build", ebuild, "-j", a.j)
        run(cmake, "--install", ebuild)
        flags += ["-DWITH_OPENJPH_ENCODER=ON", "-DWITH_OPENJPH_ENCODER_PLUGIN=OFF", f"-DOPENJPH_DIR={install / 'lib/cmake/openjph'}"]
    if a.rav1e:
        encoder = build.parent / "rav1e-source"
        install = build.parent / "rav1e-install"
        if not encoder.exists():
            run("git", "init", encoder)
            run("git", "-C", encoder, "remote", "add", "origin", "https://github.com/xiph/rav1e.git")
            run("git", "-C", encoder, "fetch", "--depth=1", "origin", RAV1E)
            run("git", "-C", encoder, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(encoder), "rev-parse", "HEAD"], text=True).strip()
        if revision != RAV1E:
            raise SystemExit("Wrong rav1e reference revision")
        # Build against the dependency versions libheifer locks, so both sides
        # compile the same crate sources.
        def locked(path):
            packages, name = {}, None
            for line in Path(path).read_text().splitlines():
                if line.startswith("name = "):
                    name = line.split('"')[1]
                elif line.startswith("version = ") and name:
                    packages.setdefault(name, set()).add(line.split('"')[1])
                    name = None
            return packages
        ours = locked(Path(__file__).resolve().parent.parent / "Cargo.lock")
        for name in sorted(locked(encoder / "Cargo.lock")):
            versions = locked(encoder / "Cargo.lock").get(name, set())
            if name in ours and len(versions) == 1 and len(ours[name]) == 1 and versions != ours[name]:
                # Exact pins elsewhere in rav1e's graph (wasm tooling) may refuse.
                subprocess.run(["cargo", "update", "--manifest-path", str(encoder / "Cargo.toml"), "-p",
                                f"{name}@{next(iter(versions))}", "--precise", next(iter(ours[name]))])
        run("cargo", "cinstall", "--manifest-path", encoder / "Cargo.toml", "--release", "--no-default-features",
            "--features", "capi,threading", f"--prefix={install}", "--libdir=lib", "--target-dir", build.parent / "rav1e-build")
        os.environ["PKG_CONFIG_PATH"] = f"{install / 'lib/pkgconfig'}:{os.environ.get('PKG_CONFIG_PATH', '')}"
        flags += ["-DWITH_RAV1E=ON", "-DWITH_RAV1E_PLUGIN=OFF"]
    if a.avc:
        suffix = "-asm" if a.avc_asm else ""
        decoder = build.parent / f"openh264{suffix}-source"
        install = build.parent / f"openh264{suffix}-install"
        if not decoder.exists():
            run("git", "init", decoder)
            run("git", "-C", decoder, "remote", "add", "origin", "https://github.com/cisco/openh264.git")
            run("git", "-C", decoder, "fetch", "--depth=1", "origin", OPENH264)
            run("git", "-C", decoder, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(decoder), "rev-parse", "HEAD"], text=True).strip()
        if revision != OPENH264:
            raise SystemExit("Wrong OpenH264 reference revision")
        run("make", "-C", decoder, "-j", a.j, f"USE_ASM={'Yes' if a.avc_asm else 'No'}", "BUILDTYPE=Release", f"PREFIX={install}", "install-shared")
        flags += ["-DWITH_OpenH264_DECODER=ON", f"-DOpenH264_INCLUDE_DIR={install / 'include'}",
                  f"-DOpenH264_LIBRARY={install / 'lib/libopenh264.so'}"]
    if a.jpeg:
        decoder = build.parent / "libjpeg-turbo-source"
        install = build.parent / "libjpeg-turbo-install"
        dbuild = build.parent / "libjpeg-turbo-build"
        if not decoder.exists():
            run("git", "init", decoder)
            run("git", "-C", decoder, "remote", "add", "origin", "https://github.com/libjpeg-turbo/libjpeg-turbo.git")
            run("git", "-C", decoder, "fetch", "--depth=1", "origin", JPEG)
            run("git", "-C", decoder, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(decoder), "rev-parse", "HEAD"], text=True).strip()
        if revision != JPEG:
            raise SystemExit("Wrong libjpeg-turbo reference revision")
        run(cmake, "-S", decoder, "-B", dbuild, "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={install}",
            "-DCMAKE_INSTALL_LIBDIR=lib", "-DWITH_SIMD=OFF", "-DWITH_TURBOJPEG=OFF", "-DENABLE_SHARED=ON", "-DENABLE_STATIC=OFF")
        run(cmake, "--build", dbuild, "-j", a.j)
        run(cmake, "--install", dbuild)
        flags += ["-DWITH_JPEG_DECODER=ON", "-DWITH_JPEG_DECODER_PLUGIN=OFF", "-DWITH_JPEG_ENCODER=ON",
                  f"-DJPEG_INCLUDE_DIR={install / 'include'}", f"-DJPEG_LIBRARY_RELEASE={install / 'lib/libjpeg.so'}"]
    run(cmake, "-S", source, "-B", build, "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_TESTING=OFF", "-DBUILD_DOCUMENTATION=OFF", "-DWITH_EXAMPLES=OFF", "-DWITH_GDK_PIXBUF=OFF", f"-DENABLE_PLUGIN_LOADING={'ON' if a.plugins else 'OFF'}", *(["-DPLUGIN_DIRECTORY="] if a.plugins else []), f"-DWITH_LIBDE265={'ON' if a.hevc else 'OFF'}", "-DWITH_X265=OFF", "-DWITH_X264=OFF", *([] if a.avc else ["-DWITH_OpenH264_DECODER=OFF"]), f"-DWITH_DAV1D={'ON' if a.av1 else 'OFF'}", "-DWITH_DAV1D_PLUGIN=OFF", "-DWITH_AOM_DECODER=OFF", "-DWITH_AOM_ENCODER=OFF", "-DWITH_LIBSHARPYUV=OFF", "-DWITH_UNCOMPRESSED_CODEC=ON", *flags)
    run(cmake, "--build", build, "-j", a.j)


if __name__ == "__main__":
    main()
