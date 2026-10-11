"""Read-only asset experiment. Writes exclusively beneath this script's output/.

Python dependencies: Pillow, numpy. Native tools: rustc; optional tex3ds, tesseract.
The outputs are local game-derived assets, intentionally ignored by Git.
"""
from __future__ import annotations
import argparse, csv, hashlib, html, json, math, shutil, struct, subprocess, time, zlib
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
from functools import lru_cache

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = HERE / 'output'
FONT = ROOT / 'crates/rfvp-rebuilder/assets/fonts/BIZUDGothic-Regular.ttf'
PLATFORMS = {'psp': (480,272), 'ps2': (640,448), 'ps1': (320,240), '3ds': (400,240)}
METHODS = ['area_current','area_compact','lanczos','coverage','redraw_fit','redraw_min12']
PICKS = ['fc_save_1','fc_save_help','sys_config_1','sys_save_1','menu_yes_1',
         'menu_no_1','menu_exit_mes','title_continue_1','title_start_1','title_end_1']
# Confirmed by inspecting originals. Keep localized image content in fixture data,
# separate from English source code. No OCR-generated strings enter rendered assets.
FIXTURES = json.loads((HERE / 'label_fixtures.json').read_text())
LABELS = {(i['game'], i['name']): i['text'] for i in FIXTURES['annotations']}
MODES = [(640,480),(800,600),(1024,768),(1280,960),(1600,1200),(640,480),
         (1024,576),(1024,640),(1280,720),(1280,800),(1440,810),(1440,900),
         (1680,945),(1680,1050),(1920,1080),(1920,1200)]
BG = (72,80,96,255)

@lru_cache(maxsize=None)
def font_for(text):
    candidates=[FONT,Path('/System/Library/Fonts/STHeiti Medium.ttc'),
                Path('/System/Library/Fonts/Supplemental/Arial Unicode.ttf')]
    for path in candidates:
        if not path.exists(): continue
        font=ImageFont.truetype(str(path),24);missing=font.getmask('\uffff')
        if all(ch.isspace() or (font.getmask(ch).size,bytes(font.getmask(ch)))!=(missing.size,bytes(missing)) for ch in text):
            return path
    raise ValueError(f'No available font covers annotation: {text!r}')

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def round_size(x): return max(1,math.floor(x+.5))
def pow2(x): return max(8,1 << (x-1).bit_length())

def straight(p):
    a=p[:,:,3:4].astype(np.int64); rgb=p[:,:,:3].astype(np.int64)
    out=p.copy(); out[:,:,:3]=np.where(a>0,np.minimum(255,(rgb*255+a//2)//np.maximum(a,1)),0).astype(np.uint8)
    return Image.fromarray(out)

def premul(im):
    a=np.asarray(im).astype(np.int64)
    a[:,:,:3]=(a[:,:,:3]*a[:,:,3:4]+127)//255
    return a.astype(np.uint8)

def weights(src,dst):
    lo=np.arange(dst)[:,None]*src; hi=lo+src
    s=np.arange(src)[None,:]*dst
    return np.maximum(0,np.minimum(hi,s+dst)-np.maximum(lo,s)).astype(np.float64)

def area(p,size):
    # Same integer overlap geometry and one final rounding as current Rust resize_area.
    w,h=size; sh,sw=p.shape[:2]; wx=weights(sw,w); wy=weights(sh,h)
    result=np.stack([wy @ p[:,:,c].astype(float) @ wx.T for c in range(4)],axis=2)
    return np.floor((result+(sw*sh)/2)/(sw*sh)).clip(0,255).astype(np.uint8)

def lanczos(p,size):
    # Independent premultiplied channels; RGBA Pillow resize would premultiply again.
    a=np.stack([np.asarray(Image.fromarray(p[:,:,c]).resize(size,Image.Resampling.LANCZOS)) for c in range(4)],axis=2)
    a[:,:,:3]=np.minimum(a[:,:,:3],a[:,:,3:4])
    return a

def coverage(im):
    # Conservative local alpha boost. Deliberately tested as a heuristic, not a paper reproduction.
    a=np.asarray(im).copy(); old=a[:,:,3].astype(float)/255
    nearby=np.asarray(Image.fromarray(a[:,:,3]).filter(ImageFilter.MaxFilter(3))).astype(float)/255
    new=np.where(old>0,np.power(old+.12*(nearby-old),.85),0)
    a[:,:,3]=np.floor(new*255+.5).clip(0,255).astype(np.uint8)
    return Image.fromarray(a)

def repaint(size,text,min12=False):
    w,h=size; stroke=1
    font_path=font_for(text)
    # Same-canvas variant is forced to fit; min12 variant grows the box when necessary.
    if min12:
        font=ImageFont.truetype(str(font_path),12)
        box=font.getbbox(text,stroke_width=stroke); needed=(box[2]-box[0]+4,box[3]-box[1]+4)
        w=max(w,needed[0]);h=max(h,needed[1])
    chosen=1
    for fs in range(1,min(28,h+1)):
        f=ImageFont.truetype(str(font_path),fs);b=f.getbbox(text,stroke_width=stroke)
        if b[2]-b[0]<=w-2 and b[3]-b[1]<=h-2: chosen=fs
    if min12: chosen=max(12,chosen)
    f=ImageFont.truetype(str(font_path),chosen);b=f.getbbox(text,stroke_width=stroke)
    im=Image.new('RGBA',(w,h));d=ImageDraw.Draw(im)
    x=(w-(b[2]-b[0]))//2-b[0];y=(h-(b[3]-b[1]))//2-b[1]
    d.text((x,y),text,font=f,fill='white',stroke_width=stroke,stroke_fill=(18,25,35,255))
    return im,chosen

def extract_sources():
    items=[]; sources=OUT/'sources';sources.mkdir(exist_ok=True)
    for game in ['AKASEKA_HD','sakura','AngelWish']:
        hcbs=list((ROOT/game).glob('*.hcb'))
        if len(hcbs)!=1: raise ValueError(f'{game}: expected one HCB')
        hb=hcbs[0].read_bytes();off=struct.unpack_from('<I',hb)[0]; design=MODES[hb[off+8]]
        pack=ROOT/game/'graph.bin'
        with pack.open('rb') as f:
            n,ns=struct.unpack('<II',f.read(8));entries=f.read(n*12);names=f.read(ns)
            for i in range(n):
                no,off,sz=struct.unpack_from('<III',entries,i*12)
                name=names[no:].split(b'\0')[0].decode('cp932')
                if name not in PICKS:continue
                f.seek(off);b=f.read(sz)
                if b[:4]!=b'hzc1' or b[12:16]!=b'NVSG':continue
                hl=struct.unpack_from('<I',b,8)[0];kind,w,h=struct.unpack_from('<HHH',b,18)
                frames=max(1,struct.unpack_from('<I',b,32)[0]);depth=3 if kind==0 else 4 if kind in [1,2] else 1
                if depth==1:continue
                payload=zlib.decompress(b[12+hl:]);assert len(payload)==w*h*depth*frames
                a=np.frombuffer(payload,np.uint8).reshape(frames,h,w,depth)[0]
                p=np.zeros((h,w,4),np.uint8);p[:,:,:3]=a[:,:,:3][:,:,::-1];p[:,:,3]=a[:,:,3] if depth==4 else 255
                sid=f'{game}__{name}';straight(p).save(sources/f'{sid}.png')
                items.append(dict(id=sid,game=game,name=name,design=design,p=p,text=LABELS.get((game,name)),
                    sha256=hashlib.sha256(b).hexdigest(),source_size=[w,h],frame=0,frames=frames,kind=kind,pack_offset=off))
    # Controlled labels: known text, source sizes and font sizes, plus difficult digits/CJK.
    for control in FIXTURES['controls']:
        fs,text=control['font_px'],control['text']
        f=ImageFont.truetype(str(font_for(text)),fs);bb=f.getbbox(text,stroke_width=2)
        im=Image.new('RGBA',(bb[2]-bb[0]+32,fs+32));d=ImageDraw.Draw(im)
        d.text((16-bb[0],16-bb[1]),text,font=f,fill='white',stroke_width=2,stroke_fill=(18,25,35,255))
        sid=f'control__font{fs}__'+('cjk' if not text.isascii() else text.replace(' ','_'))
        im.save(sources/f'{sid}.png')
        items.append(dict(id=sid,game='control',name=text,design=(1920,1080),p=premul(im),text=text,
            sha256=hashlib.sha256(im.tobytes()).hexdigest(),source_size=list(im.size),frame=0,frames=1,kind=1,source_font_px=fs))
    return items

def encode_ps1(im,stem,dither):
    w,h=im.size;raw=stem.with_suffix('.rgba');packed=stem.with_suffix('.clutprobe')
    raw.write_bytes(im.tobytes())
    subprocess.run([str(OUT/'psx_quant_probe'),str(raw),str(packed),str(w),'dither' if dither else 'none'],check=True,capture_output=True)
    b=packed.read_bytes();colors=struct.unpack_from('<H',b)[0];error=b[2]
    pal=np.frombuffer(b[3:3+colors*2],'<u2');idx=np.frombuffer(b[3+colors*3:],np.uint8).reshape(h,w)
    c=pal[idx];rgb=np.stack([((c>>s)&31)*8+((c>>s)&31)//4 for s in (0,5,10)],axis=2).astype(np.uint8)
    # Actual normal sprite path enables semi-transparency ONLY for sprite alpha <192.
    # At full sprite opacity all nonzero CLUT words, including STP colors, are opaque.
    alpha=np.where(c!=0,255,0).astype(np.uint8)
    decoded=Image.fromarray(np.dstack([rgb,alpha]))
    tile_w=236 if colors==16 else 126;per=4 if colors==16 else 2;vram=0;tiles=0
    for y in range(0,h,224):
        for x in range(0,w,tile_w):
            tw=min(tile_w,w-x);th=min(224,h-y);units=(math.ceil(tw/per)+1)&~1
            vram+=units*th*2;tiles+=1
    vram+=colors*2*2 # ordinary + forced STP palettes in current runtime
    stride=math.ceil(w/(4 if colors==16 else 2))*(4 if colors==16 else 2)
    payload=4+colors*3+stride*h*(.5 if colors==16 else 1)
    return decoded,dict(codec=f'CLUT{4 if colors==16 else 8}',texture_bytes=vram,
                        payload_bytes=int(payload),alpha_error=error,tiles=tiles)

def palette(im):
    return im.quantize(colors=256,method=Image.Quantize.FASTOCTREE,dither=Image.Dither.NONE).convert('RGBA')

def encode(im,platform,stem,compact,dither):
    w,h=im.size
    if platform=='ps1': return encode_ps1(im,stem,dither)
    if platform=='psp':
        stride=math.ceil(min(w+1,pow2(w))/4)*4;rows=min(h+1,pow2(h))
        if compact: return palette(im),dict(codec='CLUT8-model',texture_bytes=stride*rows+1024,payload_bytes=w*h+1024)
        return im,dict(codec='RGBA8888-current',texture_bytes=stride*rows*4,payload_bytes=w*h*4)
    if platform=='ps2':
        a=np.asarray(palette(im) if compact else im).copy()
        a[:,:,3]=np.floor(np.floor(a[:,:,3].astype(float)*128/255+.5)*255/128+.5).astype(np.uint8)
        # EE/GS page layout overhead is deliberately not claimed by this payload estimate.
        return Image.fromarray(a),dict(codec='PSMT8-model' if compact else 'PSMCT32-current',
            texture_bytes=None,payload_bytes=w*h*(1 if compact else 4)+(1024 if compact else 0))
    a=np.asarray(im).copy()
    if compact: a=((a.astype(np.uint16)>>4)*17).astype(np.uint8)
    return Image.fromarray(a),dict(codec='RGBA4-tex3ds-verified' if compact else 'RGBA8-current',
        texture_bytes=pow2(w)*pow2(h)*(2 if compact else 4),payload_bytes=w*h*(2 if compact else 4))

def compose(im,bg=BG):
    base=Image.new('RGBA',im.size,bg);base.alpha_composite(im);return base.convert('RGB')

def distance(a,b):
    row=list(range(len(b)+1))
    for i,x in enumerate(a,1):
        new=[i]
        for j,y in enumerate(b,1):new.append(min(new[-1]+1,row[j]+1,row[j-1]+(x!=y)))
        row=new
    return row[-1]

def ocr(im,expected,stem):
    if not expected or not expected.isascii() or not shutil.which('tesseract'):return None,None
    # Same fixed nearest 4x display enlargement for EVERY method, no SR or per-image tuning.
    grey=compose(im,(0,0,0,255)).convert('L');grey=Image.fromarray(255-np.asarray(grey));grey=grey.resize((grey.width*4,grey.height*4),Image.Resampling.NEAREST)
    pad=Image.new('L',(grey.width+40,grey.height+40),255);pad.paste(grey,(20,20));path=stem.with_suffix('.ocr.png');pad.save(path)
    res=subprocess.run(['tesseract',str(path),'stdout','-l','eng','--psm','7'],capture_output=True,text=True,check=True)
    got=''.join(x for x in res.stdout.upper() if x.isalnum());want=''.join(x for x in expected.upper() if x.isalnum())
    return got,distance(want,got)/max(1,len(want))

def native_3ds(im,stem):
    exe=shutil.which('tex3ds') or '/opt/devkitpro/tools/bin/tex3ds'
    if not Path(exe).exists():return dict(status='unavailable')
    w,h=im.size;pw,ph=pow2(w),pow2(h);padded=Image.new('RGBA',(pw,ph));padded.paste(im,(0,0))
    pin=stem.with_suffix('.padded.png');raw=stem.with_suffix('.rgba4.bin');padded.save(pin)
    subprocess.run([exe,'-f','rgba4','-r','-z','none','-o',str(raw),str(pin)],capture_output=True,check=True)
    b=raw.read_bytes();expected=pw*ph*2
    # tex3ds --raw --compress none still writes a 4-byte compression header.
    if len(b)==expected+4 and b[0]==0: b=b[4:]
    assert len(b)==expected,(len(b),expected)
    data=np.frombuffer(b,'<u2');decoded=np.zeros((ph,pw,4),np.uint8)
    for y in range(ph):
        for x in range(pw):
            morton=sum(((x>>bit)&1)<<(2*bit) | ((y>>bit)&1)<<(2*bit+1) for bit in range(3))
            at=((y//8)*(pw//8)+x//8)*64+morton;v=int(data[at])
            decoded[y,x]=[((v>>12)&15)*17,((v>>8)&15)*17,((v>>4)&15)*17,(v&15)*17]
    # Compare channel precision to the same source at every padded texel, no preview shortcuts.
    src=np.asarray(padded); reference=(src.astype(np.uint16)>>4)*17
    orientations={'direct':decoded,'flip_y':decoded[::-1]}
    name,err=min(((n,int(np.abs(d.astype(int)-reference.astype(int)).max())) for n,d in orientations.items()),key=lambda x:x[1])
    assert err==0,(name,err,'unexpected native tex3ds layout/rounding')
    actual=orientations[name][:h,:w];Image.fromarray(actual).save(stem.with_suffix('.native-rgba4.png'))
    return dict(status='verified',raw_bytes=expected,orientation=name,max_decode_error=err)

def validations():
    # Validate experimental baseline against direct integer summation on a nontrivial grid.
    p=np.random.default_rng(17).integers(0,256,(7,11,4),dtype=np.uint8);a=area(p,(4,3))
    for y in range(3):
        for x in range(4):
            sums=np.zeros(4,np.int64);total=0
            for sy in range(7):
                wy=max(0,min((y+1)*7,(sy+1)*3)-max(y*7,sy*3))
                for sx in range(11):
                    wx=max(0,min((x+1)*11,(sx+1)*4)-max(x*11,sx*4));wt=wx*wy
                    sums+=p[sy,sx].astype(np.int64)*wt;total+=wt
            assert np.array_equal(a[y,x],(sums*2+total)//(total*2))
    assert np.array_equal(area(p,(11,7)),p)
    im=Image.new('RGBA',(5,5),(255,64,32,128));recovered=straight(premul(im))
    assert np.abs(np.asarray(recovered).astype(int)-np.asarray(im).astype(int)).max()<=1

def report(items,rows,provenance):
    with (OUT/'measurements.csv').open('w') as f:
        writer=csv.DictWriter(f,fieldnames=sorted(set().union(*(r.keys() for r in rows))));writer.writeheader();writer.writerows(rows)
    (OUT/'results.json').write_text(json.dumps(dict(provenance=provenance,measurements=rows),ensure_ascii=False,indent=2))
    clean=[{k:v for k,v in i.items() if k!='p'} for i in items]
    (OUT/'sources.json').write_text(json.dumps(clean,ensure_ascii=False,indent=2))
    chunks=[]
    for platform in PLATFORMS:
        for item in items:
            group=[r for r in rows if r['platform']==platform and r['source']==item['id']]
            cells=[]
            for r in group:
                path=r['image'];mem=r['texture_bytes'];size=f"{r['width']}×{r['height']}"
                detail=f"{r['codec']} · {size} · "+(f"{mem} B texture" if mem is not None else f"{r['payload_bytes']} B payload")
                if r['ocr_cer'] is not None:detail+=f" · OCR CER {r['ocr_cer']:.2f} ({r['ocr_text']})"
                cells.append(f'<td><b>{r["method"]}</b><div class="native"><img src="{path}" style="width:{r["width"]}px;height:{r["height"]}px"></div><div class="zoom"><img src="{path}" style="width:{r["width"]*4}px;height:{r["height"]*4}px"></div><small>{html.escape(detail)}</small></td>')
            chunks.append(f'<section data-platform="{platform}"><h3>{html.escape(platform.upper()+" / "+item["id"])}</h3><p>Source {item["source_size"]}; {html.escape(item.get("text") or "No semantic annotation: redraw columns keep the Lanczos result.")}</p><table><tr>{"".join(cells)}</tr></table></section>')
    intro='''<!doctype html><meta charset="utf-8"><title>RFVP low-resolution UI experiment</title>
<style>body{background:#111821;color:#edf2f8;font:15px system-ui;margin:24px}header{position:sticky;top:0;background:#111821;padding:12px;z-index:2}table{border-collapse:collapse}td{vertical-align:top;padding:12px;border:1px solid #394454;min-width:175px}section{overflow:auto;padding-bottom:20px}.native,.zoom{background:#485060;margin:12px 0;padding:8px;width:max-content}img{image-rendering:pixelated;display:block}small{display:block;max-width:270px}.zoom{display:none}body.zoomed .zoom{display:block}button{padding:9px;margin:4px}p{color:#b8c9da}</style>
<header><h1>Low-resolution UI — PSP / PS2 / PS1 / 3DS</h1><p>Native size first. 4× nearest zoom is a diagnostic view. redraw_min12 changes geometry and style; it is NOT a drop-in asset. All results are offline, with no hardware execution.</p>
<button onclick="select('all')">All</button>'''+''.join(f'<button onclick="select(\'{p}\')">{p.upper()}</button>' for p in PLATFORMS)+'''<button onclick="document.body.classList.toggle('zoomed')">Toggle 4× diagnostic zoom</button></header>
<script>function select(p){document.querySelectorAll('section').forEach(s=>s.hidden=p!='all'&&s.dataset.platform!=p)}</script>'''
    (OUT/'report.html').write_text(intro+'\n'.join(chunks))
    # Representative sheets have native size + complete 2x nearest enlargement.
    featured=['AKASEKA_HD__sys_config_1','AKASEKA_HD__menu_exit_mes','sakura__menu_exit_mes','AngelWish__title_start_1']
    for platform in PLATFORMS:
        canvas=Image.new('RGB',(2820,1070),(17,24,33));draw=ImageDraw.Draw(canvas)
        draw.text((18,12),f'{platform.upper()} | native (top) + 2x nearest (bottom) | not hardware captures',fill='white')
        for col,method in enumerate(METHODS):draw.text((220+col*430,45),method,fill='white')
        for j,sid in enumerate(featured):
            y=80+j*235;draw.text((15,y),sid.replace('__','\n'),fill='white')
            for col,method in enumerate(METHODS):
                r=next(r for r in rows if r['source']==sid and r['platform']==platform and r['method']==method)
                im=Image.open(OUT/r['image']);rgb=compose(im);x=220+col*430
                canvas.paste(rgb,(x,y));large=rgb.resize((rgb.width*2,rgb.height*2),Image.Resampling.NEAREST)
                assert large.width<=420 and large.height<=155, 'Contact sheet cell would clip'
                canvas.paste(large,(x,y+55))
                draw.text((x,y+198),f"{r['width']}x{r['height']}  {r['codec']}",fill='#b8c9da')
        canvas.save(OUT/f'{platform}-comparison.png')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--no-ocr',action='store_true');args=parser.parse_args()
    OUT.mkdir(exist_ok=True);(OUT/'variants').mkdir(exist_ok=True)
    snapshot=OUT/'quant_snapshot.rs';shutil.copyfile(ROOT/'crates/rfvp-psx/src/quant.rs',snapshot)
    provenance=dict(quant_sha256=sha(snapshot),font_sha256=sha(FONT),started=time.strftime('%Y-%m-%dT%H:%M:%S%z'),
        methods=METHODS,platforms=PLATFORMS,notes=['first frame only','PS2 non-square pixel mapping','PS1 current force4 decision, normal sprite alpha=255',
        'PSP CLUT8 and PS2 PSMT8 are mathematical candidate models, no native upload implementation',
        '3DS RGBA4 additionally encoded/decoded with native tex3ds','OCR English only, black backing, fixed nearest 4x; not CJK readability proof'])
    subprocess.run(['rustc','--edition=2021','-O',str(HERE/'psx_quant_probe.rs'),'-o',str(OUT/'psx_quant_probe')],capture_output=True,check=True)
    validations();items=extract_sources();rows=[]
    for platform,(sw,sh) in PLATFORMS.items():
        print('Processing',platform,flush=True)
        for item in items:
            dw,dh=item['design'];scale=min(sw/dw,sh/dh);sx=sy=scale
            if platform=='ps2':
                pixel_aspect=(sw/sh)/(4/3);sx=min(sw/dw,sh*pixel_aspect/dh);sy=sx/pixel_aspect
            w,h=item['source_size'];size=(round_size(w*sx),round_size(h*sy));p=item['p']
            base=straight(area(p,size));sharp=straight(lanczos(p,size))
            for method in METHODS:
                im=base if method.startswith('area_') else coverage(base) if method=='coverage' else sharp
                fs=None;annotated=bool(item['text'])
                if method.startswith('redraw') and annotated:im,fs=repaint(size,item['text'],method=='redraw_min12')
                stem=OUT/'variants'/f'{platform}__{item["id"]}__{method}'
                t0=time.perf_counter();decoded,meta=encode(im,platform,stem,method!='area_current',method=='area_current');ms=(time.perf_counter()-t0)*1000
                decoded.save(stem.with_suffix('.png'))
                expected=item['text'] if annotated else item['name'] if item['name'] in ['SAVE','OPTION'] else None
                got,cer=(None,None) if args.no_ocr else ocr(decoded,expected,stem)
                r=dict(platform=platform,source=item['id'],method=method,width=im.width,height=im.height,
                    source_width=w,source_height=h,source_scale_x=sx,source_scale_y=sy,font_px=fs,
                    semantic_annotation=annotated,geometry_changed=im.size!=size,image=str(stem.with_suffix('.png').relative_to(OUT)),
                    ocr_text=got,ocr_cer=cer,encode_ms=round(ms,3),
                    redraw_font=str(font_for(item['text'])) if fs is not None else None,**meta)
                if platform=='3ds':r['native_3ds']=json.dumps(native_3ds(im,stem))
                rows.append(r)
    provenance['annotation_fonts']={str(font_for(i['text'])):sha(font_for(i['text'])) for i in items if i['text']}
    report(items,rows,provenance)
    print(json.dumps(dict(sources=len(items),cases=len(rows),report=str(OUT/'report.html')),indent=2))

if __name__=='__main__':main()
