"""Offline layout schematic using original image lettering, without OCR or fonts.

This is not a captured game screen, extracted script layout, or runtime patch.
Panel primitives and button slots are schematic and identical in both methods.
Only first frames are shown. Writes exclusively to output/layout/.
"""
from pathlib import Path
import html, json, struct, zlib
import numpy as np
from PIL import Image, ImageDraw
import run as base

HERE=Path(__file__).resolve().parent
OUT=HERE/'output/layout'
GAMES=['AKASEKA_HD','sakura']
ROLES={'prompt':'menu_exit_mes','confirm':'menu_yes_1','cancel':'menu_no_1'}
INK_TARGET={'psp':18,'ps2':20,'ps1':16,'3ds':16}

def source(meta):
    with (base.ROOT/meta['game']/'graph.bin').open('rb') as f:
        f.seek(meta['pack_offset']);header=f.read(36)
        length=struct.unpack_from('<I',header,8)[0]
        f.seek(meta['pack_offset']+12+length);decoder=zlib.decompressobj();raw=b''
        while not decoder.eof:
            chunk=f.read(4096)
            if not chunk:raise ValueError('Truncated resource stream')
            raw+=decoder.decompress(chunk)
    w,h=meta['source_size'];a=np.frombuffer(raw,np.uint8).reshape(meta['frames'],h,w,4)[0]
    p=np.dstack([a[:,:,:3][:,:,::-1],a[:,:,3]]).copy()
    p[:,:,:3]=np.minimum(p[:,:,:3],p[:,:,3:4])
    return p

def crop_ink(p):
    ys,xs=np.nonzero(p[:,:,3])
    assert len(xs),'Empty UI image'
    box=(int(xs.min()),int(ys.min()),int(xs.max()+1),int(ys.max()+1))
    return p[box[1]:box[3],box[0]:box[2]],box

def overlap(a,b):
    return max(a[0],b[0])<min(a[0]+a[2],b[0]+b[2]) and max(a[1],b[1])<min(a[1]+a[3],b[1]+b[3])

def main():
    OUT.mkdir(exist_ok=True)
    metas=json.loads((HERE/'output/sources.json').read_text());rows=[]
    for platform,(sw,sh) in base.PLATFORMS.items():
        par=(sw/sh)/(4/3) if platform=='ps2' else 1.0
        for game in GAMES:
            original={role:next(m for m in metas if m['game']==game and m['name']==name) for role,name in ROLES.items()}
            dw,dh=original['prompt']['design'];sx=min(sw/dw,sh*par/dh);sy=sx/par
            originals={role:source(m) for role,m in original.items()}
            for method in ['uniform','reflow']:
                encoded={};details=[]
                for role,p in originals.items():
                    crop,box=crop_ink(p)
                    if method=='uniform':
                        size=(base.round_size(p.shape[1]*sx),base.round_size(p.shape[0]*sy))
                        filtered=base.area(p,size)
                    else:
                        # Give original alpha-bounded artwork a larger output
                        # raster; never enlarge an already reduced image.
                        h=INK_TARGET[platform];w=base.round_size(crop.shape[1]*h/crop.shape[0]*par)
                        assert w<sw-32,'Prompt needs a separate segmentation/page rule'
                        size=(w,h);filtered=base.area(crop,size)
                    im=base.straight(filtered);stem=OUT/f'{platform}__{game}__{method}__{role}'
                    actual,cost=base.encode(im,platform,stem,platform=='3ds',True)
                    native=base.native_3ds(im,stem) if platform=='3ds' else None
                    actual.save(stem.with_suffix('.png'));encoded[role]=actual
                    ink_y,ink_x=np.nonzero(filtered[:,:,3])
                    details.append(dict(role=role,resource=original[role]['id'],source_crop=list(box),width=size[0],height=size[1],
                        visible_alpha_height=int(ink_y.max()-ink_y.min()+1),native_3ds=native,**cost))
                prompt=encoded['prompt'];yes=encoded['confirm'];no=encoded['cancel']
                # A constraint layout: text first, then slots, then panel size.
                # Simple panel/slot shapes consume no extra bitmap texture.
                slot_w=max(yes.width,no.width)+16;slot_h=max(yes.height,no.height)+12
                panel_w=max(prompt.width+24,slot_w*2+36);panel_h=prompt.height+slot_h+40
                assert panel_w<=sw-12 and panel_h<=sh-12
                px=(sw-panel_w)//2;py=(sh-panel_h)//2
                slots={'prompt':[px+(panel_w-prompt.width)//2,py+12,prompt.width,prompt.height],
                       'confirm':[px+(panel_w-(slot_w*2+12))//2,py+24+prompt.height,slot_w,slot_h],
                       'cancel':[px+(panel_w-(slot_w*2+12))//2+slot_w+12,py+24+prompt.height,slot_w,slot_h]}
                assert all(0<=x and 0<=y and x+w<=sw and y+h<=sh for x,y,w,h in slots.values())
                assert not overlap(slots['confirm'],slots['cancel'])
                assert not overlap(slots['prompt'],slots['confirm']) and not overlap(slots['prompt'],slots['cancel'])
                canvas=Image.new('RGBA',(sw,sh),(35,43,58,255));draw=ImageDraw.Draw(canvas)
                draw.rectangle((px,py,px+panel_w-1,py+panel_h-1),fill=(48,61,82,255),outline=(131,151,175,255))
                for role in ['confirm','cancel']:
                    x,y,w,h=slots[role];draw.rectangle((x,y,x+w-1,y+h-1),fill=(64,79,104,255),outline=(131,151,175,255))
                for role,im in encoded.items():
                    x,y,w,h=slots[role];canvas.alpha_composite(im,(x+(w-im.width)//2,y+(h-im.height)//2))
                path=OUT/f'{platform}__{game}__{method}.png';canvas.convert('RGB').save(path)
                cost=sum(d['payload_bytes'] if d['texture_bytes'] is None else d['texture_bytes'] for d in details)
                rows.append(dict(platform=platform,game=game,method=method,image=path.name,panel=[px,py,panel_w,panel_h],
                    hit_slots=slots,texture_cost=cost,cost_kind='pixel payload' if platform=='ps2' else 'estimated texture allocation',
                    assets=details,coordinate_space='final framebuffer texels',runtime_applied=False))
    (OUT/'layout.json').write_text(json.dumps(dict(scope='Offline schematic, first frames only; no runtime hit test',rows=rows),indent=2))
    sections=[]
    for platform in base.PLATFORMS:
        for game in GAMES:
            cells=[]
            for r in rows:
                if r['platform']!=platform or r['game']!=game:continue
                prompt=r['assets'][0]
                cells.append(f'<td><h3>{r["method"]}</h3><img src="{r["image"]}"><p>Prompt: {prompt["width"]}x{prompt["height"]}; visible alpha height: {prompt["visible_alpha_height"]}<br>Three image textures: {r["texture_cost"]} B ({r["cost_kind"]})</p></td>')
            sections.append(f'<section><h2>{platform.upper()} / {html.escape(game)}</h2><table><tr>{"".join(cells)}</tr></table></section>')
    intro='''<!doctype html><html lang="en"><meta charset="utf-8"><title>RFVP UI reflow schematic</title>
<style>body{background:#111821;color:#edf2f8;font:15px system-ui;margin:24px}p{max-width:1050px;color:#bdcbdb}section{overflow:auto;margin:28px 0}td{vertical-align:top;padding:12px;border:1px solid #394454}img{display:block;image-rendering:pixelated}a{color:#83c4ff}</style>
<h1>UI reflow with original image lettering</h1><p>Uniform: globally reduced image assets. Reflow: alpha-crop the original high-resolution assets, give them a larger final raster, and lay out the prompt and buttons again. Same area filter and encoding within each platform. No text recognition, neural model, replacement font or resizing of a previously reduced image.</p>
<p>These are layout schematics, NOT captured game scenes or original script layouts. Both use identical simple panel/button shapes rather than the game's decorated background. Shapes are assumed to be renderer primitives and are excluded from texture costs. Only three first-frame image textures are counted; full menu states, scene memory, input mapping, crops and animation are not implemented.</p>
<p>PS1: frozen project quantizer, normal sprite alpha 255, dithering enabled, estimated tile/palette costs. PSP: RGBA8888 estimate. PS2: PSMCT32 pixel payload, texel-grid preview, non-square pixel scale accounted for. 3DS: native tex3ds RGBA4, min-8/power-of-two costs. These encodings match within each before/after pair.</p>
<p><a href="layout.json">Placement and schematic hit slots</a> · <a href="../papers/report.html">Pixel-filter experiment</a></p>'''
    (OUT/'report.html').write_text(intro+'\n'.join(sections)+'</html>')
    sw,sh=base.PLATFORMS['ps1'];sheet=Image.new('RGB',(sw*2+56,sh*2+104),(17,24,33));d=ImageDraw.Draw(sheet)
    d.text((16,12),'PS1 / same area filter / original image lettering / offline schematic',fill='white')
    for col,m in enumerate(['uniform','reflow']):d.text((16+col*(sw+24),34),m,fill='white')
    for j,game in enumerate(GAMES):
        y=58+j*(sh+20)
        for col,m in enumerate(['uniform','reflow']):sheet.paste(Image.open(OUT/f'ps1__{game}__{m}.png'),(16+col*(sw+24),y))
    sheet.save(OUT/'ps1-overview.png')
    assert len(rows)==16
    print(json.dumps(dict(cases=len(rows),report=str(OUT/'report.html'),bounds_and_overlap_checks='passed'),indent=2))

if __name__=='__main__':main()
