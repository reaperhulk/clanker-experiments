#!/usr/bin/env python3
"""Build the pinned test-only H.264 decoders that check libheifer's High-profile AVC encoder:
the JM reference decoder (ldecod) and FFmpeg's H.264 decoder (decode-only, no assembly).
libheif's OpenH264 decoder cannot decode High 10, 4:2:2 or 4:4:4 streams. These are NOT
candidate dependencies."""
import argparse
import shutil
import subprocess
from pathlib import Path

JM = ("https://vcgit.hhi.fraunhofer.de/jvet/JM.git", "8b34eee1576952dc2a04cd2fdb52febfde4030b2")  # master, after JM 19.1
FFMPEG = ("https://git.ffmpeg.org/ffmpeg.git", "3a0867c2bfda4a4d4309ca1a8cbdc6175e67f587")  # n7.1.5


def run(*args, cwd=None):
    subprocess.run(list(map(str, args)), check=True, cwd=cwd)


def fetch(url, pin, directory):
    if not (directory / ".git").exists():
        directory.mkdir(parents=True, exist_ok=True)
        run("git", "init", "-q", directory)
        run("git", "-C", directory, "remote", "add", "origin", url)
    run("git", "-C", directory, "fetch", "-q", "--depth=1", "origin", pin)
    run("git", "-C", directory, "checkout", "-q", "FETCH_HEAD")
    if subprocess.check_output(["git", "-C", str(directory), "rev-parse", "HEAD"], text=True).strip() != pin:
        raise SystemExit(f"wrong revision of {url}")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--build", default=".build")
    p.add_argument("-j", default="4")
    a = p.parse_args()
    build = Path(a.build).resolve()
    jm = build / "jm-oracle-source"
    fetch(*JM, jm)
    run("cmake", "-S", jm, "-B", jm / "build", "-DCMAKE_BUILD_TYPE=Release")
    run("cmake", "--build", jm / "build", "-j", a.j, "--target", "ldecod")
    install = build / "jm-install/bin"
    install.mkdir(parents=True, exist_ok=True)
    built = next(p for p in (jm / "bin").rglob("ldecod") if p.is_file())
    shutil.copy2(built, install / "ldecod")
    ffmpeg = build / "ffmpeg-oracle-source"
    fetch(*FFMPEG, ffmpeg)
    run("./configure", f"--prefix={build / 'ffmpeg-install'}", "--disable-everything", "--disable-autodetect",
        "--disable-asm", "--disable-doc", "--disable-network", "--disable-debug", "--enable-decoder=h264",
        "--enable-parser=h264", "--enable-demuxer=h264", "--enable-encoder=rawvideo", "--enable-muxer=rawvideo",
        "--enable-protocol=file", "--enable-protocol=pipe", "--enable-filter=null", "--enable-filter=format",
        "--enable-filter=scale", cwd=ffmpeg)
    run("make", f"-j{a.j}", cwd=ffmpeg)
    run("make", "install", cwd=ffmpeg)


if __name__ == "__main__":
    main()
