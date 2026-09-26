"""Brotli test payloads from the pinned C brotli that tools/build_reference.py
builds for the native oracle (test-only, never a candidate dependency)."""
import ctypes
from pathlib import Path

LIBRARY = Path(__file__).resolve().parents[1] / '.build/brotli-install/lib/libbrotlienc.so'
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
