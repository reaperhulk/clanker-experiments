"""Brotli test payloads from the pinned C brotli that tools/build_reference.py
builds for the native oracle (test-only, never a candidate dependency)."""
import ctypes
import sys
from pathlib import Path

LIBRARY = Path(__file__).resolve().parents[1] / ('.build/brotli-install/lib/libbrotlienc' + ('.dylib' if sys.platform == 'darwin' else '.so'))
_lib = None


def compress(data, quality=11, lgwin=22):
    """BrotliEncoderCompress; quality 11 and window 22 are libheif's encoder defaults."""
    global _lib
    if _lib is None:
        _lib = ctypes.CDLL(str(LIBRARY))
        _lib.BrotliEncoderCompress.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_size_t,
                                               ctypes.c_char_p, ctypes.POINTER(ctypes.c_size_t), ctypes.c_char_p]
        _lib.BrotliEncoderMaxCompressedSize.argtypes = [ctypes.c_size_t]
        _lib.BrotliEncoderMaxCompressedSize.restype = ctypes.c_size_t
    size = ctypes.c_size_t(_lib.BrotliEncoderMaxCompressedSize(len(data)) or len(data) + 1024)
    out = ctypes.create_string_buffer(size.value)
    if not _lib.BrotliEncoderCompress(quality, lgwin, 0, len(data), bytes(data), ctypes.byref(size), out):
        raise RuntimeError('brotli compression failed')
    return out.raw[:size.value]


# A corrupt stream whose overlong command C 1.1.0 reports as BLOCK_LENGTH_2
# only because its ring buffer grows per metablock (found by
# tests/brotli_differential against a decoder with a smaller first ring buffer).
BLOCK_LENGTH_STREAM = bytes.fromhex('1b8a00e82f0eec58195843aa078e1a366eed400c6cdbde64b50ece0bb9ef1407e7959c471c840ac70879af2c43942b2426d8c4b1bbbf0320c284322ea4d2c63a1f62caa5b63ee6dae7be0f80108ca0184e9014cdb01c2f8892aca89a6e9896edb89e1f84519ca4595e9455ddb45d3f8cd3bcacdb7e9cd7fdbcdfef0f201841319c20299a61395e1025595135dd302ddb713d3f08a33849b3bc28abba69bb7e18a77959b7fd38affb79bf1f408409655c48a58d753e08a33849b3bc28abba69bb7e18a77959b7fd38affb79bf1fc4a2e691d5b3f7')


def encoder_inputs():
    """Inputs on which each quality-11 parity patch in vendor/brotli changes the
    output: literal context mode (short ASCII), block-splitting iterations
    (image-like data), population costs (symbol counts above 65535) and H10
    stitching across 256 KiB input blocks (UTF-8 text)."""
    import random
    r = random.Random(0x62726f74)
    image = lambda n: bytes(((i // 3) % 97 * (i % 3 + 1) + i // 291 * 3 + r.randrange(9)) & 255 for i in range(n))
    inputs = {'spaces-7': b' ' * 7, 'ascending-7': bytes(range(7)), 'image-6000': image(6000), 'image-12000': image(12000)}
    inputs['skewed-300000'] = bytes(r.choices(range(6), weights=[60, 20, 10, 5, 3, 2], k=300000))
    u = random.Random(3)
    inputs['utf8-300000'] = ''.join(u.choice('aéж中😀 \nßΩ') for _ in range(300000)).encode()[:300000]
    return inputs
