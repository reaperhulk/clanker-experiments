#!/usr/bin/env python3
"""Build pinned native test oracles. These are NOT candidate dependencies."""
import argparse
from pathlib import Path
import shutil
import os
import subprocess
import sys

HEIF = "4e14f5942c1732ace9611b9522cc991501445463"
DE265 = "7ba65889d3d6d8a0d99b5360b028243ba843be3a"
OPENJPEG = "6c4a29b00211eb0430fa0e5e890f1ce5c80f409f"
JPEG = "7723f50f3f66b9da74376e6d8badb6162464212c"
DAV1D = "42b2b24fb8819f1ed3643aa9cf2a62f03868e3aa"
OPENH264 = "652bdb7719f30b52b08e506645a7322ff1b2cc6f"  # v2.6.0
OPENJPH = "8c2826fdaaac3b0334ff5bc2ed2a8ec153c99a35"  # 0.32.0
RAV1E = "1fe82de02510767539e89b2ee6fa846920ae2686"  # v0.8.1
VVDEC = "649f0b2fafee977c998d7e4d674f8b88d952e3a3"  # v3.2.0
VVENC = "9428ea8636ae7f443ecde89999d16b2dfc421524"  # v1.14.0
X264 = "b35605ace3ddf7c1a5d67a2eb553f034aef41d55"  # the AVC fixture generator revision
SO = ".dylib" if sys.platform == "darwin" else ".so"  # shared library suffix
X265 = "1d117bed4747758b51bd2c124d738527e30392cb"  # 4.1
BROTLI = "ed738e842d2fbdf2d6459e39267a633c4a9b2f5d"  # v1.1.0
ZLIB = "51b7f2abdade71cd9bb0e7a373ef2610ec6f9daf"  # v1.3.1


def run(*args, cwd=None):
    subprocess.run(list(map(str, args)), check=True, cwd=cwd)


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
    p.add_argument("--rav1e", action="store_true", help="enable libheif's rav1e AV1 encoder (rav1e C API via cargo-c, with assembly like the candidate)")
    p.add_argument("--htj2k", action="store_true", help="enable libheif's OpenJPH HTJ2K encoder (scalar)")
    p.add_argument("--vvc", action="store_true", help="enable libheif's vvdec VVC decoder and vvenc VVC encoder (fixture generator)")
    p.add_argument("--x264", action="store_true", help="enable libheif's x264 AVC encoder (scalar, no assembly; built-in AVC encoder oracle)")
    p.add_argument("--x265", action="store_true", help="enable libheif's x265 HEVC encoder (scalar, no assembly; rate/distortion oracle)")
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
    # Keep the oracle feature set deterministic: build libheif against a pinned
    # brotli rather than whatever the host provides.
    brotli = build.parent / "brotli-source"
    binstall = build.parent / "brotli-install"
    if not brotli.exists():
        run("git", "init", brotli)
        run("git", "-C", brotli, "remote", "add", "origin", "https://github.com/google/brotli.git")
        run("git", "-C", brotli, "fetch", "--depth=1", "origin", BROTLI)
        run("git", "-C", brotli, "checkout", "--detach", "FETCH_HEAD")
    revision = subprocess.check_output(["git", "-C", str(brotli), "rev-parse", "HEAD"], text=True).strip()
    if revision != BROTLI:
        raise SystemExit("Wrong brotli reference revision")
    run(cmake, "-S", brotli, "-B", build.parent / "brotli-build", "-DCMAKE_BUILD_TYPE=Release",
        f"-DCMAKE_INSTALL_PREFIX={binstall}", "-DCMAKE_INSTALL_LIBDIR=lib", "-DBUILD_SHARED_LIBS=ON",
        "-DBROTLI_DISABLE_TESTS=ON")
    run(cmake, "--build", build.parent / "brotli-build", "-j", a.j)
    run(cmake, "--install", build.parent / "brotli-build")
    # zlib too: host zlib builds differ (Apple's reports "invalid
    # literal/length/distance code" where zlib reports the two cases apart).
    zlib = build.parent / "zlib-source"
    zinstall = build.parent / "zlib-install"
    if not zlib.exists():
        run("git", "init", zlib)
        run("git", "-C", zlib, "remote", "add", "origin", "https://github.com/madler/zlib.git")
        run("git", "-C", zlib, "fetch", "--depth=1", "origin", ZLIB)
        run("git", "-C", zlib, "checkout", "--detach", "FETCH_HEAD")
    revision = subprocess.check_output(["git", "-C", str(zlib), "rev-parse", "HEAD"], text=True).strip()
    if revision != ZLIB:
        raise SystemExit("Wrong zlib reference revision")
    run(cmake, "-S", zlib, "-B", build.parent / "zlib-build", "-DCMAKE_BUILD_TYPE=Release",
        f"-DCMAKE_INSTALL_PREFIX={zinstall}", "-DINSTALL_LIB_DIR=" + str(zinstall / "lib"))
    run(cmake, "--build", build.parent / "zlib-build", "-j", a.j)
    run(cmake, "--install", build.parent / "zlib-build")
    flags = ["-DCMAKE_DISABLE_FIND_PACKAGE_Brotli=OFF", "-DCMAKE_REQUIRE_FIND_PACKAGE_Brotli=ON", "-DCMAKE_REQUIRE_FIND_PACKAGE_ZLIB=ON",
             f"-DZLIB_INCLUDE_DIR={zinstall / 'include'}", f"-DZLIB_LIBRARY={zinstall / f'lib/libz{SO}'}",
             f"-DBROTLI_DEC_INCLUDE_DIR={binstall / 'include'}", f"-DBROTLI_ENC_INCLUDE_DIR={binstall / 'include'}",
             f"-DBROTLI_COMMON_LIB={binstall / f'lib/libbrotlicommon{SO}'}",
             f"-DBROTLI_DEC_LIB={binstall / f'lib/libbrotlidec{SO}'}", f"-DBROTLI_ENC_LIB={binstall / f'lib/libbrotlienc{SO}'}",
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
        flags += [f"-DLIBDE265_INCLUDE_DIR={install / 'include'}", f"-DLIBDE265_LIBRARY={install / f'lib/libde265{SO}'}"]
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
            # With assembly, like the candidate's rav1d: dav1d's assembly and C paths
            # decode some rav1e 10/12-bit streams differently, and rav1d follows
            # dav1d on each path.
            "-Denable_tests=false", "-Denable_asm=true", "-Ddefault_library=shared")
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
            "--features", "asm,capi,threading", f"--prefix={install}", "--libdir=lib", "--target-dir", build.parent / "rav1e-build")
        os.environ["PKG_CONFIG_PATH"] = f"{install / 'lib/pkgconfig'}:{os.environ.get('PKG_CONFIG_PATH', '')}"
        flags += ["-DWITH_RAV1E=ON", "-DWITH_RAV1E_PLUGIN=OFF"]
    if a.x265:
        codec = build.parent / "x265-source"
        install = build.parent / "x265-install"
        cbuild = build.parent / "x265-build"
        if not codec.exists():
            run("git", "init", codec)
            run("git", "-C", codec, "remote", "add", "origin", "https://bitbucket.org/multicoreware/x265_git.git")
            run("git", "-C", codec, "fetch", "--depth=1", "origin", X265)
            run("git", "-C", codec, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(codec), "rev-parse", "HEAD"], text=True).strip()
        if revision != X265:
            raise SystemExit("Wrong x265 reference revision")
        # Multilib build (x265's build/linux/multilib.sh): 10- and 12-bit static
        # libraries linked into the 8-bit shared library, as distributions ship it.
        common = ["-DCMAKE_BUILD_TYPE=Release", "-DENABLE_ASSEMBLY=OFF", "-DENABLE_CLI=OFF",
                  "-DCMAKE_POLICY_VERSION_MINIMUM=3.5"]
        for depth in (10, 12):
            dbuild = build.parent / f"x265-build-{depth}bit"
            run(cmake, "-S", codec / "source", "-B", dbuild, *common, "-DHIGH_BIT_DEPTH=ON", "-DEXPORT_C_API=OFF",
                "-DENABLE_SHARED=OFF", *(["-DMAIN12=ON"] if depth == 12 else []))
            run(cmake, "--build", dbuild, "-j", a.j)
        extra = f"{build.parent / 'x265-build-10bit/libx265.a'};{build.parent / 'x265-build-12bit/libx265.a'}"
        run(cmake, "-S", codec / "source", "-B", cbuild, *common, f"-DCMAKE_INSTALL_PREFIX={install}",
            "-DCMAKE_INSTALL_LIBDIR=lib", "-DENABLE_SHARED=ON", f"-DEXTRA_LIB={extra}", "-DEXTRA_LINK_FLAGS=-L.",
            "-DLINKED_10BIT=ON", "-DLINKED_12BIT=ON")
        run(cmake, "--build", cbuild, "-j", a.j)
        run(cmake, "--install", cbuild)
        # x265 installs its shared library (and x265.pc) only when git describe
        # finds a release tag, which a shallow fetch by commit does not have.
        for built in sorted(cbuild.glob("libx265.so*")):
            target = install / "lib" / built.name
            if not target.exists() and not target.is_symlink():
                if built.is_symlink():
                    target.symlink_to(os.readlink(built))
                else:
                    shutil.copy2(built, target)
        # Explicit paths, like the other codecs, since x265.pc may be missing.
        library = next(install.rglob("libx265.so"), None)
        if library is None:
            raise SystemExit("x265 install has no libx265.so")
        flags += [f"-DX265_INCLUDE_DIR={install / 'include'}", f"-DX265_LIBRARY={library}"]
    if a.x264:
        codec = build.parent / "x264-oracle-source"
        install = build.parent / "x264-oracle-install"
        if not codec.exists():
            run("git", "init", codec)
            run("git", "-C", codec, "remote", "add", "origin", "https://code.videolan.org/videolan/x264.git")
            run("git", "-C", codec, "fetch", "--depth=1", "origin", X264)
            run("git", "-C", codec, "checkout", "--detach", "FETCH_HEAD")
        revision = subprocess.check_output(["git", "-C", str(codec), "rev-parse", "HEAD"], text=True).strip()
        if revision != X264:
            raise SystemExit("Wrong x264 reference revision")
        # 8 and 10 bit in one library (x264's --bit-depth=all), without assembly.
        run("./configure", f"--prefix={install}", "--enable-shared", "--disable-static", "--disable-cli",
            "--disable-asm", "--bit-depth=all", "--chroma-format=all", cwd=codec)
        run("make", "-C", codec, "-j", a.j)
        run("make", "-C", codec, "install")
        flags += [f"-DX264_INCLUDE_DIR={install / 'include'}", f"-DX264_LIBRARY={install / 'lib/libx264.so'}"]
    if a.vvc:
        for name, pin, url in (("vvdec", VVDEC, "https://github.com/fraunhoferhhi/vvdec.git"),
                               ("vvenc", VVENC, "https://github.com/fraunhoferhhi/vvenc.git")):
            codec = build.parent / f"{name}-source"
            install = build.parent / f"{name}-install"
            cbuild = build.parent / f"{name}-build"
            if not codec.exists():
                run("git", "init", codec)
                run("git", "-C", codec, "remote", "add", "origin", url)
                run("git", "-C", codec, "fetch", "--depth=1", "origin", pin)
                run("git", "-C", codec, "checkout", "--detach", "FETCH_HEAD")
            revision = subprocess.check_output(["git", "-C", str(codec), "rev-parse", "HEAD"], text=True).strip()
            if revision != pin:
                raise SystemExit(f"Wrong {name} reference revision")
            run(cmake, "-S", codec, "-B", cbuild, "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={install}",
                "-DCMAKE_INSTALL_LIBDIR=lib", "-DBUILD_SHARED_LIBS=ON", f"-D{name.upper()}_ENABLE_LINK_TIME_OPT=OFF",
                f"-D{name.upper()}_LIBRARY_ONLY=ON")
            run(cmake, "--build", cbuild, "-j", a.j)
            run(cmake, "--install", cbuild)
            flags += [f"-DWITH_{name.upper()}=ON", f"-DWITH_{name.upper()}_PLUGIN=OFF", f"-D{name}_DIR={install / 'lib/cmake' / name}"]
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
    run(cmake, "-S", source, "-B", build, "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_TESTING=OFF", "-DBUILD_DOCUMENTATION=OFF", "-DWITH_EXAMPLES=OFF", "-DWITH_GDK_PIXBUF=OFF", f"-DENABLE_PLUGIN_LOADING={'ON' if a.plugins else 'OFF'}", *(["-DPLUGIN_DIRECTORY="] if a.plugins else []), f"-DWITH_LIBDE265={'ON' if a.hevc else 'OFF'}", f"-DWITH_X265={'ON' if a.x265 else 'OFF'}", "-DWITH_X265_PLUGIN=OFF", f"-DWITH_X264={'ON' if a.x264 else 'OFF'}", "-DWITH_X264_PLUGIN=OFF", *([] if a.avc else ["-DWITH_OpenH264_DECODER=OFF"]), f"-DWITH_DAV1D={'ON' if a.av1 else 'OFF'}", "-DWITH_DAV1D_PLUGIN=OFF", "-DWITH_AOM_DECODER=OFF", "-DWITH_AOM_ENCODER=OFF", "-DWITH_LIBSHARPYUV=OFF", "-DWITH_UNCOMPRESSED_CODEC=ON", *flags)
    run(cmake, "--build", build, "-j", a.j)
    # libheif silently drops an encoder CMake cannot find; the x265 oracle must have it.
    if a.x265 or a.x264:
        needed = subprocess.check_output(["readelf", "-d", str(build / "libheif/libheif.so")], text=True)
        for enabled, library in ((a.x265, "libx265"), (a.x264, "libx264")):
            if enabled and library not in needed:
                raise SystemExit(f"libheif was built without {library}")
    if SO != ".so":
        # The differential suites link the oracle as libheif/libheif.so.
        link = build / "libheif/libheif.so"
        link.unlink(missing_ok=True)
        link.symlink_to(f"libheif{SO}")


if __name__ == "__main__":
    main()
