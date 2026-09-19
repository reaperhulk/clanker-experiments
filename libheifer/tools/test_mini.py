#!/usr/bin/env python3
"""Minimized HEIF original-header context, metadata, color and lifetime comparisons."""
import itertools
import struct
import sys
import zlib
import test_context


class Bits:
    def __init__(self):
        self.bits = ''

    def put(self, value, count=1):
        assert 0 <= value < 1 << count
        self.bits += f'{int(value):0{count}b}'

    def bytes(self):
        bits = self.bits.ljust((len(self.bits) + 7) // 8 * 8, '0')
        return int(bits, 2).to_bytes(len(bits) // 8, 'big')


def fixture(width=31, height=17, orientation=1, chroma=1, depth=8,
            float_depth=False, alpha=False, premultiplied=False, cicp=None,
            icc=b'', exif=b'', xmp=b'', compressed=False, brand=b'avif',
            explicit=False, config=None, alpha_config=b'', hdr=0,
            gain=False, tmap_icc=b'', version=0, large=False, coded_data=b'coded-image-placeholder'):
    if config is None:
        config = bytes.fromhex('81000c00') if brand != b'heic' else bytes.fromhex('01016000000090000000000078f000fcfdf8f800000f00')
    if compressed:
        exif = zlib.compress(exif)[2:-4] if exif else b''
        xmp = zlib.compress(xmp)[2:-4] if xmp else b''
    data = coded_data
    alpha_data = b'alpha-placeholder' if alpha else b''
    gain_data = b'gain-placeholder' if gain else b''
    gain_meta = b'gain-metadata' if gain else b''
    b = Bits()
    b.put(version, 2)
    for value in [explicit, float_depth, True, alpha, cicp is not None, bool(hdr or gain), bool(icc), bool(exif), bool(xmp)]:
        b.put(value)
    b.put(chroma, 2)
    b.put(orientation - 1, 3)
    dimensions = 15 if large or max(width, height) > 128 else 7
    b.put(dimensions == 15)
    b.put(width - 1, dimensions)
    b.put(height - 1, dimensions)
    if chroma in (1, 2): b.put(1)
    if chroma == 1: b.put(0)
    if float_depth:
        b.put(depth.bit_length() - 5, 2)
    else:
        b.put(depth != 8)
        if depth != 8: b.put(depth - 9, 3)
    if alpha: b.put(premultiplied)
    if cicp is not None:
        for value in cicp: b.put(value, 8)
    if explicit:
        b.put(int.from_bytes(b'zzzz', 'big'), 32)
        b.put(int.from_bytes(b'xxxx', 'big'), 32)
    if hdr or gain:
        b.put(gain)
        if gain:
            b.put(0)
            b.put(6, dimensions)
            b.put(8, dimensions)
            b.put(6, 8)
            b.put(1)
            b.put(1, 2)
            b.put(1)
            b.put(0)
            b.put(0)
            b.put(1)
            b.put(1, 3)
            b.put(bool(tmap_icc))
            b.put(1)
            for value in (9, 16, 9): b.put(value, 8)
            b.put(1)
        for tone_map in range(2 if gain else 1):
            for bit in range(6): b.put(bool(hdr & (1 << bit)))
            if hdr & 1:
                b.put(1000 + tone_map, 16)
                b.put(200, 16)
            if hdr & 2:
                for value in range(8): b.put(value * 3000, 16)
                b.put(10000000, 32)
                b.put(100, 32)
            if hdr & 4:
                b.put(0xff, 8)
                for value in range(9): b.put(0xff000000 + value, 32)
            if hdr & 8:
                b.put(10000, 32)
                b.put(15635, 16)
                b.put(16450, 16)
            if hdr & 16:
                for value in range(4): b.put(value, 32)
            if hdr & 32: b.put(2030000, 32)
    if icc or exif or xmp or gain: b.put(large)
    config_bits = 12 if large or max(len(config), len(alpha_config)) >= 8 else 3
    b.put(config_bits == 12)
    b.put(large)
    metadata_bits, item_bits = (20, 28) if large else (10, 15)
    if icc: b.put(len(icc) - 1, metadata_bits)
    if gain and tmap_icc: b.put(len(tmap_icc) - 1, metadata_bits)
    if gain:
        b.put(len(gain_meta), metadata_bits)
        b.put(len(gain_data), item_bits)
        b.put(0, config_bits)
    b.put(len(config), config_bits)
    b.put(len(data) - 1, item_bits)
    if alpha:
        b.put(len(alpha_data), item_bits)
        b.put(len(alpha_config), config_bits)
    if exif or xmp: b.put(compressed)
    if exif: b.put(len(exif) - 1, metadata_bits)
    if xmp: b.put(len(xmp) - 1, metadata_bits)
    payload = b.bytes() + config + alpha_config + icc + tmap_icc + gain_meta + alpha_data + gain_data + data + exif + xmp
    return test_context.box(b'ftyp', b'mif3' + brand) + test_context.box(b'mini', payload)


def corpus(source):
    cases = [(p.name, p.read_bytes()) for p in sorted((source/'tests/data').glob('*'))
             if p.suffix in ('.avif', '.heif') and b'mini' in p.read_bytes()[:40]]
    for brand, orientation, alpha, large in itertools.product([b'avif', b'heic'], range(1, 9), [False, True], [False, True]):
        cases.append((f'orientation-{brand}-{orientation}-{alpha}-{large}', fixture(brand=brand, orientation=orientation, alpha=alpha, large=large)))
    for chroma, depth, cicp, icc in itertools.product(range(4), [8, 10, 12, 16], [None, (9, 16, 9), (255, 255, 255)], [b'', b'ICC-profile']):
        cases.append((f'color-{chroma}-{depth}-{cicp}-{bool(icc)}', fixture(chroma=chroma, depth=depth, cicp=cicp, icc=icc)))
    for hdr, gain in itertools.product(range(64), [False, True]):
        cases.append((f'hdr-{hdr}-{gain}', fixture(hdr=hdr, gain=gain, tmap_icc=b'ICC' if gain else b'')))
    for exif, xmp, compressed, alpha in itertools.product([b'', b'\0\0\0\0Exif payload'], [b'', b'<rdf>metadata</rdf>'], [False, True], [False, True]):
        cases.append((f'metadata-{bool(exif)}-{bool(xmp)}-{compressed}-{alpha}', fixture(exif=exif, xmp=xmp, compressed=compressed, alpha=alpha)))
    for depth in [16, 32, 64, 128]:
        cases.append((f'float-{depth}', fixture(float_depth=True, depth=depth)))
    for version, explicit in itertools.product(range(4), [False, True]):
        cases.append((f'version-{version}-{explicit}', fixture(version=version, explicit=explicit)))
    for size in range(5):
        cases.append((f'config-{size}', fixture(config=bytes.fromhex('81000c00')[:size])))
        cases.append((f'alpha-config-{size}', fixture(alpha=True, alpha_config=bytes.fromhex('81000c00')[:size])))
    for brand in [b'heix', b'jpeg', b'zzzz']:
        cases.append((f'brand-{brand}', fixture(brand=brand)))
    base = fixture(icc=b'ICC', exif=b'\0\0\0\0Exif', xmp=b'XMP', alpha=True, hdr=63, gain=True, large=True)
    for size in range(len(base) + 1):
        cases.append((f'truncate-{size}', base[:size]))
        if size >= 24:
            cases.append((f'truncate-resized-{size}', base[:16] + struct.pack('>I', size - 16) + base[20:size]))
    for name, data in list(cases[:4]):
        cases.append((name+'-size-zero', data[:16] + bytes(4) + data[20:]))
    return cases


def main():
    test_context.corpus = corpus
    test_context.SCOPE = __doc__
    if '--output' not in sys.argv: sys.argv += ['--output', '.build/mini-report.json']
    test_context.main()


if __name__ == '__main__':
    main()
