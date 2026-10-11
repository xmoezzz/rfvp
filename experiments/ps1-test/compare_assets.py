"""Inspect real converted PS1 packs and loose console NVSGs without OCR."""
from pathlib import Path
import html
import json
import struct
import zlib
from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
OUT = HERE / "output"
PLATFORMS = {"ps1": (320, 240), "psp": (480, 272), "ps2": (640, 448), "3ds": (400, 240)}
PICKS = ["title_start", "title_continue", "title_option", "title_end", "menu_exit_mes", "menu_yes_1", "menu_no_1", "fc_save_help"]


def entries(path):
    data = Path(path).read_bytes()
    count, size = struct.unpack_from("<II", data)
    names = data[8 + count * 12:8 + count * 12 + size]
    result = {}
    for i in range(count):
        name, offset, length = struct.unpack_from("<III", data, 8 + i * 12)
        name = names[name:].split(b"\0", 1)[0].decode("cp932")
        result[name] = data[offset:offset + length]
    return result


def decode(data):
    header_size = struct.unpack_from("<I", data, 8)[0]
    kind, width, height = struct.unpack_from("<HHH", data, 18)
    offset = struct.unpack_from("<hh", data, 24)
    count = max(1, struct.unpack_from("<I", data, 32)[0])
    raw = data[12 + header_size:]
    vram = None
    if kind in (16, 17):
        colors, stride = struct.unpack_from("<HH", raw)
        palette = struct.unpack_from("<" + "H" * colors, raw, 4)
        indices = raw[4 + colors * 3:]
        pixels = []
        for y in range(height):
            for x in range(width):
                index = indices[y * stride + x] if kind == 16 else (indices[y * stride // 2 + x // 2] >> (4 * (x % 2))) & 15
                color = palette[index]
                # Normal fully opaque sprite path: STP only matters for blended prims.
                pixels.append(tuple(((color >> s) & 31) * 255 // 31 for s in (0, 5, 10)) + (255 if color else 0,))
        per, tile = (4, 236) if kind == 17 else (2, 126)
        row_bytes = sum((((min(tile, width - x) + per - 1) // per + 1) & ~1) * 2 for x in range(0, width, tile))
        vram = (row_bytes * height + colors * 4) * count
        image = Image.new("RGBA", (width, height)); image.putdata(pixels)
    else:
        raw = zlib.decompress(raw)
        mode = "RGB" if kind == 0 else "RGBA" if kind in (1, 2) else "L"
        depth = len(mode) if mode != "L" else 1
        image = Image.frombytes(mode, (width, height), raw[:width * height * depth], "raw", "BGR" if depth == 3 else "BGRA" if depth == 4 else "L")
        if depth == 4:
            # NVSG stores premultiplied BGRA; display it as straight RGBA.
            image.putdata([tuple(min(255, (c * 255 + a // 2) // a) if a else 0 for c in (r, g, b)) + (a,)
                           for r, g, b, a in image.get_flattened_data()])
        image = image.convert("RGBA")
    return image, dict(width=width, height=height, frames=count, kind=kind, offset=list(offset),
                      payload_bytes=len(data) - 12 - header_size, vram_bytes=vram)


def main():
    preview = OUT / "comparison"; preview.mkdir(exist_ok=True)
    baseline = entries(OUT / "baseline/GRAPH.BIN")
    optimized = entries(OUT / "disc/GRAPH.BIN")
    rows = []
    for name in PICKS:
        for variant, pack in [("before", baseline), ("after", optimized)]:
            image, details = decode(pack[name])
            file = f"ps1-{variant}-{name}.png"; image.save(preview / file)
            rows.append(dict(platform="ps1", variant=variant, name=name, image=file, **details))
        for platform in ("psp", "ps2", "3ds"):
            data = (OUT / platform / "graph" / name).read_bytes()
            image, details = decode(data)
            file = f"{platform}-after-{name}.png"; image.save(preview / file)
            rows.append(dict(platform=platform, variant="after", name=name, image=file, **details))

    # Check every state in the explicit profiles against the final viewport.
    checks = []
    for platform, (screen_w, screen_h) in PLATFORMS.items():
        scale = min(screen_w / 1920, screen_h / 1080)
        viewport_h = round(1080 * scale)
        profile = json.loads((HERE / "profiles" / f"{platform}.json").read_text())
        boxes = {}
        for resource in profile["assets"]:
            name = resource.split("/", 1)[1]
            data = optimized[name] if platform == "ps1" else (OUT / platform / resource).read_bytes()
            _, meta = decode(data)
            x, y = [round(v * scale) for v in meta["offset"]]
            box = [x, y, meta["width"], meta["height"]]
            assert 0 <= x and 0 <= y and x + meta["width"] <= screen_w and y + meta["height"] <= viewport_h, (platform, resource, box)
            boxes[resource] = box
        # Selected states must fit between neighboring title rows too.
        families = ["title_start", "title_continue", "title_album", "title_option", "title_end"]
        for first, second in zip(families, families[1:]):
            bottom = max(boxes[f"graph/{first}{suffix}"][1] + boxes[f"graph/{first}{suffix}"][3] for suffix in ["", "1", "2", "3", "4", "5"])
            top = min(boxes[f"graph/{second}{suffix}"][1] for suffix in ["", "1", "2", "3", "4", "5"])
            assert bottom <= top, (platform, first, second, bottom, top)
        checks.append(dict(platform=platform, bounds="passed", title_state_overlap="passed", boxes=boxes))
    (preview / "measurements.json").write_text(json.dumps(dict(rows=rows, checks=checks), indent=2))
    sections = []
    for name in PICKS:
        cards = []
        for row in [r for r in rows if r["name"] == name]:
            cost = f"VRAM {row['vram_bytes']} B" if row["vram_bytes"] is not None else f"payload {row['payload_bytes']} B"
            cards.append(f'<article><b>{row["platform"]} / {row["variant"]}</b><p>{row["width"]} x {row["height"]}; {cost}</p><img src="{row["image"]}"><img class="zoom" src="{row["image"]}" width="{row["width"]*4}" height="{row["height"]*4}"></article>')
        sections.append(f'<section><h2>{html.escape(name)}</h2><div class="cards">{"".join(cards)}</div></section>')
    page = '''<!doctype html><html lang="en"><meta charset="utf-8"><title>Console UI converter comparison</title>
<style>body{background:#18212b;color:#edf3fa;font:15px system-ui;margin:24px}.cards{display:flex;gap:18px;overflow:auto}article{background:#485060;padding:14px;min-width:220px}img{display:block;image-rendering:pixelated;margin:16px 0}.zoom{max-width:none}a{color:#92c7ff}</style>
<h1>Real converter output: DPID and explicit UI layout</h1><p>Original raster lettering; no OCR, generated text, or runtime filter. PS1 previews decode the actual new CLUT packs for normal sprites. Other platforms show converted NVSG pixels, before GPU format upload. Profiles cover all configured title button states. Bounds and title-row separation are checked; runtime clicks remain manual.</p>
<p><a href="measurements.json">Dimensions, placement and memory measurements</a></p>'''
    (preview / "report.html").write_text(page + "\n".join(sections) + "</html>")
    selected = [r for r in rows if r["platform"] == "ps1"]
    column_width = max(r["width"] * 4 for r in selected) + 32
    sheet = Image.new("RGB", (column_width * 2, len(PICKS) * 100 + 45), (35, 43, 58)); draw = ImageDraw.Draw(sheet)
    draw.text((16, 12), "PS1 actual CLUT textures: before / after (4x nearest)", fill="white")
    for i, name in enumerate(PICKS):
        draw.text((16, 45 + i * 100), name, fill="white")
        for column, variant in enumerate(("before", "after")):
            row = next(r for r in selected if r["name"] == name and r["variant"] == variant)
            image = Image.open(preview / row["image"])
            image = image.resize((image.width * 4, image.height * 4), Image.Resampling.NEAREST)
            sheet.paste(image, (16 + column * column_width, 65 + i * 100), image)
    sheet.save(preview / "ps1-assets.png")
    print(json.dumps(dict(entries=len(rows), geometry_checks="passed", report=str(preview / "report.html"))))


if __name__ == "__main__":
    main()
