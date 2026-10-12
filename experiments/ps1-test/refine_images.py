"""Compare actual converter output using original raster assets; no OCR or models."""
from pathlib import Path
import html, json, os, shutil, struct, subprocess, sys, zlib
from PIL import Image
import compare_assets

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = HERE / 'output/refinement'
TARGET = HERE / 'output/cargo'
REFERENCE = '3748fa3'


def command(args, log, **kwargs):
    with log.open('w') as output:
        subprocess.run(args, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT, check=True, **kwargs)


def label(name):
    return name.startswith(('title_', 'menu_yes', 'menu_no')) or name.endswith('_mes') or name == 'fc_save_help'


def prepare():
    OUT.mkdir(parents=True, exist_ok=True)
    records = json.loads((ROOT / 'experiments/ui-legibility-20261012-codex/output/papers/sources.json').read_text())
    records = [r for r in records if r['game'] != 'control']
    for game in sorted({r['game'] for r in records}):
        fixture = OUT / 'fixtures' / game
        (fixture / 'graph').mkdir(parents=True, exist_ok=True)
        shutil.copy2(next((ROOT / game).glob('*.hcb')), fixture)
        if not all((fixture / 'graph' / r['name']).exists() for r in records if r['game'] == game):
            pack = compare_assets.entries(ROOT / game / 'graph.bin')
            for record in records:
                if record['game'] == game:
                    (fixture / 'graph' / record['name']).write_bytes(pack[record['name']])
    for platform in compare_assets.PLATFORMS:
        original = json.loads((HERE / 'profiles' / (platform + '.json')).read_text())
        for rule in original['assets'].values():
            rule.pop('coverage_threshold', None)
        for game in sorted({r['game'] for r in records}):
            profile = original if game == 'AKASEKA_HD' else dict(design_width=next(r['design'][0] for r in records if r['game'] == game), design_height=next(r['design'][1] for r in records if r['game'] == game), assets={})
            for variant in ['before', 'after']:
                current = json.loads(json.dumps(profile))
                if variant == 'after':
                    for record in records:
                        if record['game'] == game and label(record['name']):
                            rule = current['assets'].setdefault('graph/' + record['name'], dict(scale=1.0))
                            rule['coverage_threshold'] = 192 if platform == 'ps1' else 128
                (OUT / f'{platform}-{game}-{variant}.json').write_text(json.dumps(current, indent=2))
    return records


def ps1_probe(records):
    probe = OUT / 'probe'; (probe / 'src').mkdir(parents=True, exist_ok=True)
    for name in ['image', 'quant']:
        source = subprocess.check_output(['git', 'show', f'{REFERENCE}:crates/rfvp-psx/src/{name}.rs'], cwd=ROOT).decode()
        if name == 'image': source = source.replace('use crate::quant;', 'use crate::quant_before as quant;')
        (probe / f'{name}_before.rs').write_text(source)
    (probe / 'Cargo.toml').write_text('''[workspace]\n[package]\nname = "image-refinement-probe"\nversion = "0.1.0"\nedition = "2024"\n[dependencies]\nanyhow = "1"\nflate2 = "1"\nencoding_rs = "0.8"\nserde_json = "1"\nwalkdir = "2"\nrfvp-image = { path = ''' + json.dumps(str(ROOT / 'crates/rfvp-image')) + ' }\n')
    modules = '\n'.join(f'#[path = {json.dumps(str(path))}] mod {name};' for name, path in [('game', ROOT / 'crates/rfvp-psx/src/game.rs'), ('image', ROOT / 'crates/rfvp-psx/src/image.rs'), ('quant', ROOT / 'crates/rfvp-psx/src/quant.rs'), ('image_before', probe / 'image_before.rs'), ('quant_before', probe / 'quant_before.rs')])
    source = modules + '''
use rfvp_image::{Filter, LayoutProfile, LayoutRule};
fn main() -> anyhow::Result<()> {
    let folder = std::env::args().nth(1).unwrap();
    let root = std::path::Path::new(&folder);
    let records: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("records.json"))?)?;
    for record in records.as_array().unwrap() {
        let game = record["game"].as_str().unwrap();
        let name = record["name"].as_str().unwrap();
        let design = record["design"][0].as_u64().unwrap() as u32;
        let source = std::fs::read(root.join("fixtures").join(game).join("graph").join(name))?;
        for variant in ["before", "palette", "after"] {
            let profile_variant = if variant == "after" { "after" } else { "before" };
            let profile = LayoutProfile::load(&root.join(format!("ps1-{game}-{profile_variant}.json"))).map_err(|e| anyhow::anyhow!("{e}"))?;
            let rule = profile.get(&format!("graph/{name}"));
            let output = if variant == "before" {
                image_before::convert_with_options(&source, 320, design, true, Filter::DpidBalanced, rule)?
            } else { image::convert_with_options(&source, 320, design, true, Filter::DpidBalanced, rule)? };
            let dest = root.join("ps1").join(game).join(variant).join("graph");
            std::fs::create_dir_all(&dest)?;
            std::fs::write(dest.join(name), output)?;
        }
    }
    Ok(())
}
'''
    (probe / 'src/main.rs').write_text(source)
    (OUT / 'records.json').write_text(json.dumps(records))
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    command(['cargo', 'run', '--release', '--manifest-path', str(probe / 'Cargo.toml'), '--', str(OUT)], OUT / 'ps1-probe.log', env=env)


def rebuild(records):
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    command(['cargo', 'build', '--release', '-p', 'rfvp-rebuilder', '-p', 'rfvp-psx'], OUT / 'converter-build.log', env=env)
    for platform in ['psp', 'ps2', '3ds']:
        for game in sorted({r['game'] for r in records}):
            for variant in ['before', 'after']:
                command([str(TARGET / 'release/rfvp-rebuilder'), '--platform', platform, '--input', str(OUT / 'fixtures' / game), '--output', str(OUT / platform / game / variant), '--images-only', '--image-pack', 'graph', '--image-filter', 'dpid-balanced', '--ui-layout', str(OUT / f'{platform}-{game}-{variant}.json')], OUT / f'{platform}-{game}-{variant}.log')


def texture_bytes(platform, stats):
    w, h, frames = stats['width'], stats['height'], stats['frames']
    if platform == 'psp':
        while (1 << (w - 1).bit_length()) > 512 or (1 << (h - 1).bit_length()) > 512:
            w, h = max(1, w // 2), max(1, h // 2)
        pw, ph = 1 << (w - 1).bit_length(), 1 << (h - 1).bit_length()
        return ((min(w + 1, pw) + 3) & ~3) * min(h + 1, ph) * 4 * frames
    if platform == '3ds':
        return max(8, 1 << (w - 1).bit_length()) * max(8, 1 << (h - 1).bit_length()) * 4 * frames
    if platform == 'ps1':
        return stats['vram_bytes'] or 0
    return w * h * frames * 4


def details(data, platform):
    image, stats = compare_assets.decode(data)
    header = struct.unpack_from('<I', data, 8)[0]
    raw = data[12 + header:]
    kind = stats['kind']
    if kind in [16, 17]:
        colors, stride = struct.unpack_from('<HH', raw)
        palette = struct.unpack_from('<' + 'H' * colors, raw, 4)
        alpha = raw[4 + colors * 2:4 + colors * 3]
        indices = raw[4 + colors * 3:]
        indices = list(indices) if kind == 16 else [n for b in indices for n in (b & 15, b >> 4)]
        indices = [n for row in range(stats['height'] * stats['frames']) for n in indices[row * stride:row * stride + stats['width']]]
        values = [alpha[i] for i in indices]
        stats['visible_gpu_pixels'] = sum(palette[i] != 0 for i in indices)
    else:
        raw = zlib.decompress(raw)
        values = raw[3::4] if kind in [1, 2] else [255] * (stats['width'] * stats['height'] * stats['frames'])
    stats['coverage64'] = sum(a >= 64 for a in values) / len(values)
    stats['coverage128'] = sum(a >= 128 for a in values) / len(values)
    stats['file_bytes'] = len(data)
    stats['decoded_bytes'] = len(raw)
    stats['texture_bytes'] = texture_bytes(platform, stats)
    return image, stats


def report(records):
    report = OUT / 'report'; report.mkdir(exist_ok=True)
    rows, cards = [], []
    for platform in compare_assets.PLATFORMS:
        cards.append(f'<h2>{html.escape(platform.upper())}</h2>')
        for record in records:
            game, name = record['game'], record['name']
            pictures = []
            reference_geometry = None
            variants = ['before', 'palette', 'after'] if platform == 'ps1' else ['before', 'after']
            for variant in variants:
                data = (OUT / platform / game / variant / 'graph' / name).read_bytes()
                image, stats = details(data, platform)
                geometry = [stats[k] for k in ['width', 'height', 'frames', 'offset']]
                if reference_geometry is None: reference_geometry = geometry
                assert geometry == reference_geometry, (platform, game, name, 'geometry changed')
                file = f'{platform}-{game}-{name}-{variant}.png'; image.save(report / file)
                rows.append(dict(platform=platform, game=game, name=name, variant=variant, **stats))
                pictures.append(f'<figure><img style="width:{stats["width"] * 3}px" src="{html.escape(file)}"><figcaption>{variant}: {stats["width"]} x {stats["height"]}, file {len(data)} B, texture storage {stats["texture_bytes"]} B, alpha coverage 64/128: {stats["coverage64"]:.1%}/{stats["coverage128"]:.1%}</figcaption></figure>')
            cards.append(f'<article><h3>{html.escape(game + " / " + name)}</h3><div class="row">' + ''.join(pictures) + '</div></article>')
    (OUT / 'measurements.json').write_text(json.dumps(rows, indent=2))
    page = '''<!doctype html><meta charset="utf-8"><title>Low-resolution UI refinement</title><style>body{background:#191d24;color:#eee;font:15px system-ui;margin:24px}article{border-top:1px solid #555;padding:12px}.row{display:flex;gap:24px;flex-wrap:wrap}figure{margin:0;max-width:420px}img{image-rendering:pixelated;max-width:420px;max-height:320px;background:repeating-conic-gradient(#58606c 0% 25%,#353c46 0% 50%) 50%/12px 12px;width:auto;height:auto;transform-origin:top left}figcaption{margin:10px 0}h3{font-size:15px}</style><h1>Low-resolution UI refinement</h1><p>Original raster assets, DPID and unchanged layout. PS1: previous quantizer / reserved alpha levels / optional coverage. PSP, PS2, 3DS: current converter / optional coverage. No OCR, redraw, or runtime changes. Images show decoded converter output, not hardware captures. PS1 preview shows normal fully opaque primitives; actual blended primitives also use STP.</p>'''
    totals = []
    for platform in compare_assets.PLATFORMS:
        values = [[sum(r[key] for r in rows if r['platform'] == platform and r['variant'] == variant)
                   for key in ['file_bytes', 'texture_bytes']] for variant in ['before', 'after']]
        totals.append(f'<tr><td>{platform.upper()}</td><td>{values[0][0]} / {values[1][0]}</td><td>{values[0][1]} / {values[1][1]}</td></tr>')
    table = '<h2>24-source totals</h2><table><tr><th>Platform</th><th>File bytes: before / after</th><th>Texture bytes: before / after</th></tr>' + ''.join(totals) + '</table><p>PS1: DMA-padded tiles and two CLUT copies. PSP: GPU-readable RAM buffer. 3DS: power-of-two RGBA8 storage. PS2: RGBA pixel bytes only; GS page allocation is not measured. These are summed assets, not peak resident memory. Alpha coverage is a diagnostic, not a readability score. All geometry and animation counts are unchanged.</p>'
    (report / 'index.html').write_text(page + table + '<p><a href="full.html">Full AKASEKA_HD pack audit</a></p>' + ''.join(cards))
    print(f'{len(records)} sources, {len(rows)} actual encoded variants: {report / "index.html"}', flush=True)


def audit_full():
    summaries, cards = [], []
    for platform in compare_assets.PLATFORMS:
        if platform == 'ps1':
            before = compare_assets.entries(HERE / 'output/disc/GRAPH.BIN')
            after = compare_assets.entries(OUT / 'full/ps1/GRAPH.BIN')
        else:
            before = {p.name: p.read_bytes() for p in (HERE / 'output' / platform / 'graph').iterdir() if p.is_file()}
            after = {p.name: p.read_bytes() for p in (OUT / 'full' / platform / 'graph').iterdir() if p.is_file()}
        assert before.keys() == after.keys(), platform
        totals = dict(before_file=0, after_file=0, before_texture=0, after_texture=0, changed_files=0)
        promotions, reductions = [], []
        for name, old in before.items():
            new = after[name]
            # Palette depth may change; placement, animation and pixel count may not.
            assert old[20:44] == new[20:44], (platform, name, 'geometry changed')
            assert old[:4] == new[:4] == b'hzc1', (platform, name)
            totals['changed_files'] += old != new
            sizes = []
            for data in [old, new]:
                kind, w, h = struct.unpack_from('<HHH', data, 18)
                n = max(1, struct.unpack_from('<I', data, 32)[0])
                vram = None
                if kind in [16, 17]:
                    per, tile, colors = (4, 236, 16) if kind == 17 else (2, 126, 256)
                    row = sum((((min(tile, w - x) + per - 1) // per + 1) & ~1) * 2 for x in range(0, w, tile))
                    vram = (row * h + colors * 4) * n
                size = texture_bytes(platform, dict(width=w, height=h, frames=n, vram_bytes=vram)) if kind in [0, 1, 2, 16, 17] else 0
                sizes.append(size)
            for variant, data, size in zip(['before', 'after'], [old, new], sizes):
                totals[variant + '_file'] += len(data)
                totals[variant + '_texture'] += size
            if sizes[1] > sizes[0]: promotions.append(dict(name=name, before=sizes[0], after=sizes[1]))
            elif sizes[1] < sizes[0]: reductions.append(dict(name=name, before=sizes[0], after=sizes[1]))
            if platform == 'ps1' and sizes[1] > sizes[0]:
                pictures = []
                for variant, data in [('before', old), ('after', new)]:
                    im, _ = compare_assets.decode(data)
                    file = f'full-{name}-{variant}.png'; im.save(OUT / 'report' / file)
                    pictures.append(f'<figure><img src="{html.escape(file)}"><figcaption>{variant}</figcaption></figure>')
                cards.append('<article><h3>' + html.escape(name) + '</h3>' + ''.join(pictures) + '</article>')
        summary = dict(platform=platform, entries=len(before), geometry='unchanged', **totals,
                       promotions=promotions, reductions=reductions)
        summaries.append(summary)
        print(json.dumps(summary), flush=True)
    (OUT / 'full-audit.json').write_text(json.dumps(summaries, indent=2))
    table = ''.join(f'<tr><td>{s["platform"].upper()}</td><td>{s["entries"]}</td><td>{s["before_file"]} / {s["after_file"]}</td><td>{s["before_texture"]} / {s["after_texture"]}</td><td>{len(s["promotions"])} / {len(s["reductions"])}</td></tr>' for s in summaries)
    page = '<!doctype html><meta charset="utf-8"><title>Full pack audit</title><style>body{font:16px system-ui;background:#202833;color:white;margin:24px}td,th{padding:12px;text-align:left}article{border-top:1px solid gray}figure{display:inline-block;vertical-align:top}img{image-rendering:pixelated}a{color:#9cd}</style><h1>Full AKASEKA_HD graph audit</h1><p>Baseline: previously converted packs in output/disc and output/platform. All 3,426 entries per platform retain placement, animation count and dimensions. File totals exclude outer pack index/padding. Texture totals sum all assets, not peak resident memory; masks are excluded. PSP is GPU-readable RAM; PS2 is RGBA bytes before GS page rounding.</p><table><tr><th>Platform</th><th>Entries</th><th>File bytes: before / after</th><th>Texture bytes: before / after</th><th>Larger / smaller textures</th></tr>' + table + '</table><p>The quality guard selects CLUT8 for four PS1 assets after correcting premultiplied colors and alpha allocation. They are shown below for review. The old alpha allocator is consulted only to guard existing 4-bit eligibility; it never supplies the written palette. This is not a universal per-asset memory reduction.</p><p><a href="index.html">24-source visual comparison</a></p>'
    (OUT / 'report/full.html').write_text(page + ''.join(cards))


if __name__ == '__main__':
    if '--audit-full' in sys.argv:
        audit_full()
        sys.exit(0)
    records = prepare()
    print(f'Prepared {len(records)} original assets.', flush=True)
    ps1_probe(records)
    rebuild(records)
    report(records)
