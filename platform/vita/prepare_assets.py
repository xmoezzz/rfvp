"""Adapt the existing RFVP icon to PSV package sizes; no artwork is redrawn."""
from pathlib import Path
import hashlib
import json
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT
ART = SOURCE / 'platform/wasm/icon/icon-512.png'
STATIC = SOURCE / 'platform/vita/host/static/sce_sys'
REPORT = SOURCE / 'platform/vita'
REPORT.mkdir(parents=True, exist_ok=True)
image = Image.open(ART).convert('RGBA')
background = image.getpixel((image.width // 2, 0))[:3]

def original_at(size):
    resized = image.resize((size, size), Image.Resampling.LANCZOS)
    canvas = Image.new('RGB', resized.size, background)
    canvas.paste(resized, (0, 0), resized)
    return canvas

def save_png(name, canvas):
    path = STATIC / name
    path.parent.mkdir(parents=True, exist_ok=True)
    indexed = canvas.quantize(colors=128, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
    # A one-colour background otherwise gets emitted as PNG-1 by Pillow even
    # when bits=8 is requested. Keep a full RGB palette to require PNG-8.
    palette = indexed.getpalette() or []
    indexed.putpalette((palette + [0] * 768)[:768])
    indexed.save(path, format='PNG', bits=8, optimize=True)
    return {'path': path.relative_to(SOURCE).as_posix(), 'size': canvas.size,
            'mode': indexed.mode, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

assets = [save_png('icon0.png', original_at(128))]
startup = Image.new('RGB', (280, 158), background)
startup.paste(original_at(128), (76, 15))
assets.append(save_png('livearea/contents/startup.png', startup))
assets.append(save_png('livearea/contents/bg.png', Image.new('RGB', (840, 500), background)))
template = '''<?xml version="1.0" encoding="utf-8"?>
<livearea style="a1" format-ver="01.00" content-rev="1">
  <livearea-background><image>bg.png</image></livearea-background>
  <gate><startup-image>startup.png</startup-image></gate>
</livearea>
'''
(STATIC / 'livearea/contents/template.xml').write_bytes(template.replace('\n', '\r\n').encode('utf-8'))
(REPORT / 'assets.json').write_text(json.dumps({
    'source': ART.relative_to(SOURCE).as_posix(),
    'source_sha256': hashlib.sha256(ART.read_bytes()).hexdigest(),
    'source_size': image.size, 'background_rgb': background, 'assets': assets,
    'reference': 'https://github.com/vitasdk/samples/tree/master/hello_world/sce_sys',
}, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps(assets, ensure_ascii=False))
