#!/usr/bin/env python3
"""Generate a small original boot scene, without external game assets or fonts."""
import argparse
from pathlib import Path
import struct
import zlib

GLYPHS = {
    'R': ['11110','10001','10001','11110','10100','10010','10001'],
    'F': ['11111','10000','10000','11110','10000','10000','10000'],
    'V': ['10001','10001','10001','10001','10001','01010','00100'],
    'P': ['11110','10001','10001','11110','10000','10000','10000'],
    'D': ['11110','10001','10001','10001','10001','10001','11110'],
    'E': ['11111','10000','10000','11110','10000','10000','11111'],
    'M': ['10001','11011','10101','10101','10001','10001','10001'],
    'O': ['01110','10001','10001','10001','10001','10001','01110'],
    ' ': ['00000'] * 7,
}


def make_demo(folder):
    folder = Path(folder)
    folder.mkdir(parents=True, exist_ok=True)
    width, height = 640, 480
    pixels = bytearray(bytes([42, 28, 18, 255]) * width * height)

    def box(x, y, w, h, bgra):
        row = bytes(bgra) * w
        for py in range(y, y + h):
            start = (py * width + x) * 4
            pixels[start:start + w * 4] = row

    for x, color in [(60, [64, 190, 255, 255]), (230, [160, 210, 70, 255]), (400, [245, 160, 90, 255])]:
        box(x, 340, 150, 24, color)
    for text, y, scale in [('RFVP', 100, 12), ('DEMO', 230, 6)]:
        x = (width - (len(text) * 6 - 1) * scale) // 2
        for character in text:
            for gy, row in enumerate(GLYPHS[character]):
                for gx, bit in enumerate(row):
                    if bit == '1': box(x + gx * scale, y + gy * scale, scale, scale, [245, 245, 245, 255])
            x += 6 * scale
    header = bytearray(32)
    header[:4] = b'NVSG'
    struct.pack_into('<HHH', header, 6, 1, width, height)
    struct.pack_into('<I', header, 20, 1)
    nvsg = b'hzc1' + struct.pack('<II', len(pixels), len(header)) + header + zlib.compress(pixels)
    # PS1 consumes BIN packs; the rebuilder can consume the same source pack.
    name = b'splash\0'
    pack = struct.pack('<IIIII', 1, len(name), 0, 20 + len(name), len(nvsg)) + name + nvsg
    (folder / 'graph.bin').write_bytes(pack)

    syscalls = [('GraphLoad', 2), ('PrimSetSprt', 4), ('PrimGroupIn', 2),
                ('PrimSetDraw', 2), ('ThreadSleep', 1)]
    code = bytearray(b'\x01\x00\x00')

    def call(index, *args):
        for arg in args:
            if isinstance(arg, str):
                blob = arg.encode('ascii') + b'\0'
                code.extend(bytes([14, len(blob)]) + blob)
            else:
                code.extend(b'\x0a' + struct.pack('<i', arg))
        code.extend(b'\x03' + struct.pack('<H', index))

    call(0, 0, 'graph/splash')
    call(1, 1, 0, 0, 0)
    call(2, 1, 0)
    call(3, 1, 1)
    loop = 4 + len(code)
    call(4, 1000)
    code.extend(b'\x06' + struct.pack('<I', loop))
    title = b'RFVP Demo\0'
    system = struct.pack('<IHHBBB', 4, 0, 0, 0, 0, len(title)) + title
    system += struct.pack('<H', len(syscalls))
    for name, count in syscalls:
        blob = name.encode('ascii') + b'\0'
        system += bytes([count, len(blob)]) + blob
    system += struct.pack('<H', 0)
    (folder / 'demo.hcb').write_bytes(struct.pack('<I', 4 + len(code)) + code + system)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    make_demo(parser.parse_args().output)
