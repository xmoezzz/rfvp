"""Decode the game's TTY framebuffer dump; no desktop capture or input APIs."""
import argparse
from pathlib import Path
from PIL import Image


def decode(log, output):
    image = None
    complete = False
    rows = set()
    for line in Path(log).read_text(errors="replace").splitlines():
        if "TTY: " not in line:
            continue
        payload = line.split("TTY: ", 1)[1].strip()
        if payload == "RFVPSHOT end":
            complete = True
            break
        if payload.startswith("RFVPSHOT "):
            width, height = map(int, payload.split()[1:])
            image = Image.new("RGB", (width, height))
        elif image is not None and payload.startswith("S "):
            _, y, x, data = payload.split()
            y, x = int(y), int(x)
            for offset in range(0, len(data), 4):
                color = int(data[offset:offset + 4], 16)
                image.putpixel((x + offset // 4, y), tuple(
                    ((color >> shift) & 31) * 255 // 31 for shift in (0, 5, 10)))
            rows.add((y, x))
    if image is None or not complete or len(rows) != image.height * ((image.width + 63) // 64):
        raise RuntimeError("No complete framebuffer dump in log")
    image.save(output)
    return image


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("log")
    parser.add_argument("output")
    args = parser.parse_args()
    decode(args.log, args.output)
