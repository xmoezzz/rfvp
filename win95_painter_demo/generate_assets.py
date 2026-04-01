#!/usr/bin/env python3
from pathlib import Path
from PIL import Image, ImageDraw
import struct
import zlib

ROOT = Path(__file__).resolve().parent
GRAPH = ROOT / "graph"
SRC = ROOT / "assets_src"

def new_icon(size=16):
    return Image.new("RGBA", (size, size), (0, 0, 0, 0))

def draw_pencil():
    img = new_icon()
    d = ImageDraw.Draw(img)
    d.line((3, 12, 11, 4), fill=(60, 40, 0, 255), width=3)
    d.line((4, 13, 12, 5), fill=(245, 210, 110, 255), width=1)
    d.polygon([(11, 4), (13, 2), (14, 3), (12, 5)], fill=(250, 230, 180, 255), outline=(0, 0, 0, 255))
    d.line((3, 12, 2, 14), fill=(230, 120, 120, 255), width=2)
    return img

def draw_eraser():
    img = new_icon()
    d = ImageDraw.Draw(img)
    d.polygon([(3, 11), (8, 6), (13, 11), (8, 14)], fill=(255, 160, 190, 255), outline=(0, 0, 0, 255))
    d.polygon([(3, 11), (8, 6), (10, 8), (5, 13)], fill=(255, 210, 220, 255), outline=(0, 0, 0, 255))
    return img

def draw_bucket():
    img = new_icon()
    d = ImageDraw.Draw(img)
    d.polygon([(4, 6), (11, 6), (13, 10), (8, 13), (3, 10)], fill=(220, 220, 220, 255), outline=(0, 0, 0, 255))
    d.arc((4, 2, 11, 8), start=200, end=340, fill=(0, 0, 0, 255), width=1)
    d.polygon([(11, 11), (14, 9), (15, 13), (12, 14)], fill=(0, 120, 215, 255), outline=(0, 0, 0, 255))
    return img

def draw_close():
    img = new_icon()
    d = ImageDraw.Draw(img)
    d.rectangle((1, 1, 14, 14), fill=(192, 192, 192, 255), outline=(0, 0, 0, 255))
    d.line((4, 4, 11, 11), fill=(128, 0, 0, 255), width=2)
    d.line((11, 4, 4, 11), fill=(128, 0, 0, 255), width=2)
    return img

def premultiply_rgba(img):
    img = img.convert("RGBA")
    out = []
    for r, g, b, a in list(img.getdata()):
        pr = (r * a + 127) // 255
        pg = (g * a + 127) // 255
        pb = (b * a + 127) // 255
        out.append((pr, pg, pb, a))
    return out

def pack_nvsg_rgba(img, out_path):
    img = img.convert("RGBA")
    pixels = premultiply_rgba(img)
    raw = bytearray()
    for r, g, b, a in pixels:
        raw += bytes((b, g, r, a))
    compressed = zlib.compress(bytes(raw), level=9)
    width, height = img.size
    nvsg_header = struct.pack(
        "<4s8H3I",
        b"NVSG",
        0,
        1,
        width,
        height,
        0,
        0,
        0,
        0,
        1,
        0,
        0,
    )
    hzc1_header = struct.pack("<4sII", b"hzc1", len(raw), 32)
    out_path.write_bytes(hzc1_header + nvsg_header + compressed)

def main():
    GRAPH.mkdir(exist_ok=True)
    SRC.mkdir(exist_ok=True)

    icons = {
        "w95_pencil": draw_pencil(),
        "w95_eraser": draw_eraser(),
        "w95_bucket": draw_bucket(),
        "w95_close": draw_close(),
    }

    for name, img in icons.items():
        img.save(SRC / f"{name}.png")
        pack_nvsg_rgba(img, GRAPH / name)

if __name__ == "__main__":
    main()
