#!/usr/bin/env python3
"""Generate owned AV1 test images with native libaom; never used by the candidate.

The checked-in hex payloads let comparisons run without an installed encoder.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    ffmpeg = shutil.which('ffmpeg')
    if not ffmpeg:
        raise SystemExit('ffmpeg with libaom-av1 is required to regenerate fixtures')
    fixtures = []
    with tempfile.TemporaryDirectory() as directory:
        for depth in (8, 10, 12):
            for sampling in ('gray', '420', '422', '444', 'rgb'):
                for width, height in [(31, 17)]:
                    suffix = '' if depth == 8 else f'{depth}le'
                    fmt = 'gray' + suffix if sampling == 'gray' else ('gbrp' + suffix if sampling == 'rgb' else 'yuv' + sampling + 'p' + suffix)
                    planes = [(width, height)]
                    if sampling != 'gray':
                        cw = (width + 1) // 2 if sampling in ('420', '422') else width
                        ch = (height + 1) // 2 if sampling == '420' else height
                        planes += [(cw, ch)] * 2
                    raw = bytearray()
                    for channel, (pw, ph) in enumerate(planes):
                        for y in range(ph):
                            for x in range(pw):
                                value = ((x * 137 + y * 311 + x * y * 17 + channel * 601) ^ (x << 5)) & ((1 << depth) - 1)
                                raw += value.to_bytes(1 if depth == 8 else 2, 'little')
                    path = Path(directory) / 'image.avif'
                    command = ['-hide_banner', '-loglevel', 'error', '-y', '-f', 'rawvideo', '-pixel_format', fmt,
                               '-video_size', f'{width}x{height}', '-i', 'pipe:0', '-frames:v', '1', '-c:v', 'libaom-av1',
                               '-threads', '1', '-cpu-used', '6', '-still-picture', '1', '-crf', '23', '-color_range', 'pc',
                               '-color_primaries', 'bt709', '-color_trc', 'bt709', '-colorspace', 'rgb' if sampling == 'rgb' else 'bt709', '-f', 'avif']
                    subprocess.run([ffmpeg, *command, str(path)], input=raw, check=True)
                    data = path.read_bytes()
                    fixtures.append({'name': f'aom-{sampling}-{depth}-{width}x{height}.avif', 'width': width, 'height': height,
                                     'bit_depth': depth, 'sampling': sampling, 'command': command,
                                     'input_sha256': hashlib.sha256(raw).hexdigest(),
                                     'sha256': hashlib.sha256(data).hexdigest(), 'hex': data.hex()})
    result = {'scope': 'Deterministic generated sample patterns, encoded by native libaom through FFmpeg; no external image licenses.',
              'generator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'ffmpeg_sha256': hashlib.sha256(Path(ffmpeg).read_bytes()).hexdigest(),
              'ffmpeg_version': subprocess.check_output([ffmpeg, '-version'], text=True), 'fixtures': fixtures}
    Path('tests/fixtures/av1-generated.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
