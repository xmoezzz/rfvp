"""DPID paper experiment; never uses labels/fonts/OCR/models to process images.

Runs isolated rustc probes, reuses the earlier frozen PS1 quantizer, and writes
only output/papers/. No Cargo build and no changes to shared converter files.
Requires existing run.py outputs, Pillow, numpy and rustc; tex3ds is optional.
"""
from pathlib import Path
import csv, hashlib, html, json, math, struct, subprocess, time, urllib.request, zlib
import numpy as np
from PIL import Image, ImageDraw
import run as previous

HERE=Path(__file__).resolve().parent
OUT=HERE/'output/papers'
METHODS={'area':(0,1),'lanczos':None,'dpid_balanced':(.5,.5),
         'dpid_025':(.25,1),'dpid_050':(.5,1),'dpid_100':(1,1),
         'dpid_budget':(.5,.5)}
EXTRA={'AKASEKA_HD':['icon_play_bar','icon_play_frame','main_window','menu_bg'],
       'sakura':['icon_play_bar','icon_play_frame','menu_bg']}

def ensure_references():
    commit='dba82881a1797a12df123a7de4079e54da24bcc1'
    refs=[('matab/dpid.m','524b58e41d3e3054caa337d1df7c994ab8fc57ba402e70a1a463a246df300e16'),
          ('LICENSE.txt','f220ebc2eebcd7b99201201110f21dbf942671e9e7055ce6aaddbcc0675f4d97')]
    folder=OUT/'references';folder.mkdir(exist_ok=True);records=[]
    for rel,expected in refs:
        path=folder/Path(rel).name;url=f'https://raw.githubusercontent.com/mergian/dpid/{commit}/{rel}'
        if not path.exists():path.write_bytes(urllib.request.urlopen(url,timeout=30).read())
        assert previous.sha(path)==expected,f'Unexpected author reference hash: {path}'
        records.append(dict(url=url,sha256=expected,file=path.name))
    (folder/'provenance.json').write_text(json.dumps(dict(commit=commit,files=records),indent=2))

def load_sources():
    sources=[]
    for meta in json.loads((HERE/'output/sources.json').read_text()):
        # Strip annotations before processing. Read original premultiplied NVSG
        # data rather than lossily unpremultiply/re-premultiply saved previews.
        meta={k:v for k,v in meta.items() if k not in ['text','source_font_px']}
        if meta['game']=='control':
            p=previous.premul(Image.open(HERE/'output/sources'/f'{meta["id"]}.png').convert('RGBA'))
        else:
            with (previous.ROOT/meta['game']/'graph.bin').open('rb') as f:
                f.seek(meta['pack_offset']);b=f.read(36)
                head=struct.unpack_from('<I',b,8)[0];f.seek(meta['pack_offset']+12+head)
                # zlib stops at this stream's end; using bounded size prevents
                # reading an entire large pack per item.
                dec=zlib.decompressobj();data=b''
                while not dec.eof: data+=dec.decompress(f.read(4096))
            w,h=meta['source_size'];depth=3 if meta['kind']==0 else 4
            a=np.frombuffer(data,np.uint8).reshape(meta['frames'],h,w,depth)[0]
            p=np.zeros((h,w,4),np.uint8);p[:,:,:3]=a[:,:,:3][:,:,::-1];p[:,:,3]=a[:,:,3] if depth==4 else 255
        p[:,:,:3]=np.minimum(p[:,:,:3],p[:,:,3:4]);meta['p']=p;sources.append(meta)
    for game,picks in EXTRA.items():
        with (previous.ROOT/game/'graph.bin').open('rb') as f:
            n,ns=struct.unpack('<II',f.read(8));e=f.read(n*12);names=f.read(ns)
            design=next(s['design'] for s in sources if s['game']==game)
            for j in range(n):
                no,off,sz=struct.unpack_from('<III',e,j*12);name=names[no:].split(b'\0')[0].decode('cp932')
                if name not in picks:continue
                f.seek(off);b=f.read(sz);assert b[:4]==b'hzc1' and b[12:16]==b'NVSG'
                head=struct.unpack_from('<I',b,8)[0];kind,w,h=struct.unpack_from('<HHH',b,18);frames=max(1,struct.unpack_from('<I',b,32)[0]);depth=3 if kind==0 else 4
                a=np.frombuffer(zlib.decompress(b[12+head:]),np.uint8).reshape(frames,h,w,depth)[0]
                p=np.zeros((h,w,4),np.uint8);p[:,:,:3]=a[:,:,:3][:,:,::-1];p[:,:,3]=a[:,:,3] if depth==4 else 255
                p[:,:,:3]=np.minimum(p[:,:,:3],p[:,:,3:4])
                sources.append(dict(id=f'{game}__{name}',game=game,name=name,design=design,p=p,source_size=[w,h],frames=frames,frame=0,kind=kind,pack_offset=off,sha256=hashlib.sha256(b).hexdigest()))
    for s in sources:
        previous.straight(s['p']).save(OUT/'sources'/f'{s["id"]}.png')
    return sources

def dpid(p,size,lam,mix,stem):
    pin=stem.with_suffix('.input');pout=stem.with_suffix('.premul')
    pin.write_bytes(p.tobytes());w,h=size;sh,sw=p.shape[:2]
    subprocess.run([str(OUT/'dpid'),str(pin),str(pout),str(sw),str(sh),str(w),str(h),str(lam),str(mix)],check=True,capture_output=True)
    out=np.frombuffer(pout.read_bytes(),np.uint8).reshape(h,w,4).copy();pin.unlink();pout.unlink()
    return out

def reference_rgb(rgb,size,lam):
    # Independent scalar transcription of the authors' MATLAB loops. Fractional
    # coordinates (not the Rust integer overlap) and 3-channel distance metric.
    h,w=rgb.shape[:2];dw,dh=size;sx=w/dw;sy=h/dh;avg=np.zeros((dh,dw,3))
    def patch(x,y):
        for iy in range(math.floor(y*sy),min(h,math.ceil((y+1)*sy))):
            for ix in range(math.floor(x*sx),min(w,math.ceil((x+1)*sx))):
                wt=max(0,min(ix+1,(x+1)*sx)-max(ix,x*sx))*max(0,min(iy+1,(y+1)*sy)-max(iy,y*sy))
                yield rgb[iy,ix].astype(float),wt
    for y in range(dh):
        for x in range(dw):
            n=0
            for c,wt in patch(x,y):avg[y,x]+=c*wt;n+=wt
            avg[y,x]/=n
    out=np.zeros_like(avg)
    for y in range(dh):
        for x in range(dw):
            guide=np.zeros(3);n=0
            for gy in range(max(0,y-1),min(dh,y+2)):
                for gx in range(max(0,x-1),min(dw,x+2)):
                    wt=(2 if gy==y else 1)*(2 if gx==x else 1);guide+=avg[gy,gx]*wt;n+=wt
            guide/=n;n=0
            for c,wt in patch(x,y):
                wt*=1 if lam==0 else (np.linalg.norm(guide-c)/(255*math.sqrt(3)))**lam
                out[y,x]+=c*wt;n+=wt
            out[y,x]=out[y,x]/n if n else guide
    return np.floor(out+.5).clip(0,255).astype(np.uint8)

def validate():
    subprocess.run(['rustc','--edition=2021','-O',str(HERE/'dpid.rs'),'-o',str(OUT/'dpid')],check=True,capture_output=True)
    subprocess.run(['rustc','--edition=2021','--test',str(HERE/'dpid.rs'),'-o',str(OUT/'dpid-tests')],check=True,capture_output=True)
    result=subprocess.run([str(OUT/'dpid-tests')],check=True,capture_output=True,text=True)
    rng=np.random.default_rng(196);records=[]
    for sh,sw,dw,dh in [(7,11,4,3),(9,13,5,4),(4,4,1,1)]:
        rgb=rng.integers(0,256,(sh,sw,3),dtype=np.uint8);p=np.dstack([rgb,np.full((sh,sw),255,np.uint8)])
        for lam in [0,.25,.5,1]:
            got=dpid(p,(dw,dh),lam,1,OUT/'reference-probe')[:,:,:3];want=reference_rgb(rgb,(dw,dh),lam)
            error=int(np.abs(got.astype(int)-want.astype(int)).max());assert error==0
            records.append(dict(source=[sw,sh],target=[dw,dh],lambda_=lam,max_error=error))
    return dict(rust_tests=result.stdout,rgb_matlab_transcription=records)

def add_budget_guard(rows):
    guarded=[]
    for candidate in rows:
        if candidate['method']!='dpid_balanced':continue
        base=next(r for r in rows if r['platform']==candidate['platform'] and r['lane']==candidate['lane'] and r['source']==candidate['source'] and r['method']=='area')
        chosen=base if candidate['over_budget'] else candidate
        guard=dict(chosen,method='dpid_budget',selected_method=chosen['method'],budget_fallback=bool(candidate['over_budget']))
        assert not guard['over_budget']
        guarded.append(guard)
    return rows+guarded

def make_report(items,rows,provenance):
    (OUT/'results.json').write_text(json.dumps(dict(provenance=provenance,rows=rows),ensure_ascii=False,indent=2))
    (OUT/'sources.json').write_text(json.dumps([{k:v for k,v in i.items() if k!='p'} for i in items],ensure_ascii=False,indent=2))
    with (OUT/'measurements.csv').open('w') as f:
        wr=csv.DictWriter(f,fieldnames=sorted(set().union(*(r.keys() for r in rows))));wr.writeheader();wr.writerows(rows)
    content=[]
    for platform in previous.PLATFORMS:
        for lane in ['current','compact']:
            for item in items:
                group=[r for r in rows if r['platform']==platform and r['lane']==lane and r['source']==item['id']]
                cells=[]
                for r in group:
                    cost=f"{r['texture_bytes']} B texture" if r['texture_bytes'] is not None else f"{r['payload_bytes']} B payload"
                    more=' · OVER AREA BUDGET' if r['over_budget'] else ''
                    if r.get('budget_fallback'):more+=' · Budget fallback: Area'
                    cells.append(f'<td><b>{r["method"]}</b><div class="tile native"><img src="{r["image"]}"></div><div class="tile zoom"><img src="{r["image"]}" width="{r["width"]*4}" height="{r["height"]*4}"></div><small>{r["width"]}×{r["height"]} · {r["codec"]}<br>{cost}{more}<br>Filter {r["filter_ms"]:.2f} ms (includes process startup) · alpha sum delta {r["alpha_sum_delta_percent"]:+.1f}%</small><details><summary>Before encoding</summary><div class="tile"><img src="{r["filtered"]}" width="{r["width"]*4}" height="{r["height"]*4}"></div></details></td>')
                source=f'sources/{item["id"]}.png'
                content.append(f'<section data-platform="{platform}" data-lane="{lane}"><h3>{platform.upper()} / {html.escape(item["id"])}</h3><details><summary>Source {item["source_size"][0]}×{item["source_size"][1]} (first frame only)</summary><div class="tile original"><img src="{source}"></div></details><table><tr>{"".join(cells)}</tr></table></section>')
    intro='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>RFVP · Pixel-only DPID downscaling experiment</title>
<style>body{background:#111821;color:#edf2f8;font:15px system-ui;margin:20px;--tile:#485060}header{position:sticky;top:0;background:#111821;padding:8px;z-index:2;border-bottom:1px solid #394454}h1{font-size:24px}p{max-width:1050px;color:#bdcbdb}table{border-collapse:collapse}td{vertical-align:top;padding:12px;border:1px solid #394454;min-width:180px}section{overflow:auto;padding-bottom:25px}.tile{background:var(--tile);margin:12px 0;padding:8px;width:max-content}img{image-rendering:pixelated;display:block}.original img{max-width:1050px}small{display:block;max-width:300px}.zoom{display:none}body.zoomed .zoom{display:block}select,button{padding:7px;margin:3px;background:#24344a;color:white;border:1px solid #66798d}details{margin:10px 0}a{color:#83c4ff}</style>
<header><h1>DPID: pixel-only downscaling</h1><p>Every method uses the same target canvas. balanced = 50% Area + 50% DPID lambda 0.5 (our conservative blend, not VRChat's implementation). budget falls back to Area when the encoded balanced result exceeds the Area memory estimate. The RGB port matched an independent reference; transparent inputs use our premultiplied RGBA extension. No OCR, LLM, font replacement or canvas expansion.</p>
<select id="platform" onchange="select()"><option value="psp">PSP</option><option value="ps2">PS2</option><option value="ps1">PS1</option><option value="3ds">3DS</option><option value="all">All platforms</option></select>
<select id="lane" onchange="select()"><option value="current">Current formats: filter comparison</option><option value="compact">Compact candidates: encoded comparison</option><option value="all">All formats</option></select>
<button onclick="document.body.classList.toggle('zoomed')">Toggle 4x nearest zoom</button>
<select onchange="document.body.style.setProperty('--tile',this.value)"><option value="#485060">Gray background</option><option value="#000">Black background</option><option value="#fff">White background</option></select></header>
<p>Current: PSP RGBA8888; PS2 PSMCT32; PS1 frozen project quantizer (normal sprite alpha 255, dithering enabled); 3DS RGBA8. Compact: mathematical PSP CLUT8 / PS2 PSMT8 models; PS1 same quantizer without dithering; 3DS native tex3ds RGBA4 round trips. PS2 costs are pixel payloads, not GS VRAM. PS1 auto-selects bit depth; increases over Area are marked. Equivalent-color ordering may be nondeterministic. 3DS costs include min-8 power-of-two dimensions. No hardware execution. PS2 previews show texel grids rather than non-square television pixels.</p>
<p>Alpha sum delta describes transparency drift, not readability. Inspect native size first, then use zoom to check joined strokes, colors and edges. Text, icons and frames use the same algorithm.</p>
<p><a href="https://www.mergian.de/2016/siggraphasia-dpid/">DPID paper</a> · <a href="https://github.com/mergian/dpid">Author code (BSD-3-Clause)</a> · <a href="https://creators.vrchat.com/releases/release-3-7-1/">VRChat implementation notes</a> · <a href="../report.html">Earlier experiment with min12</a></p>
<script>function select(){let p=document.getElementById('platform').value,l=document.getElementById('lane').value;document.querySelectorAll('section').forEach(s=>s.hidden=(p!='all'&&s.dataset.platform!=p)||(l!='all'&&s.dataset.lane!=l))}addEventListener('DOMContentLoaded',select)</script>'''
    (OUT/'report.html').write_text(intro+'\n'.join(content)+'</html>')
    featured=['AKASEKA_HD__sys_config_1','AKASEKA_HD__menu_exit_mes','sakura__menu_exit_mes','AKASEKA_HD__icon_play_frame','AngelWish__title_start_1']
    for platform in previous.PLATFORMS:
        cellw=450;cellh=210;canvas=Image.new('RGB',(250+cellw*len(METHODS),70+cellh*len(featured)),(17,24,33));d=ImageDraw.Draw(canvas)
        d.text((14,8),f'{platform.upper()} / CURRENT FORMAT / native + 2x nearest / same canvas / offline preview',fill='white')
        for col,m in enumerate(METHODS):d.text((250+col*cellw,40),m,fill='white')
        for j,sid in enumerate(featured):
            y=70+j*cellh;d.text((14,y),sid.replace('__','\n'),fill='white')
            for col,m in enumerate(METHODS):
                r=next(r for r in rows if r['platform']==platform and r['lane']=='current' and r['source']==sid and r['method']==m)
                im=previous.compose(Image.open(OUT/r['image']));assert im.width*2<cellw and im.height*2<125
                x=250+col*cellw;canvas.paste(im,(x,y));canvas.paste(im.resize((im.width*2,im.height*2),Image.Resampling.NEAREST),(x,y+55))
                d.text((x,y+170),f"{r['width']}x{r['height']} {r['codec']}",fill='#b8c9da')
        canvas.save(OUT/f'{platform}-comparison.png')

def main():
    for folder in ['','sources','variants']: (OUT/folder).mkdir(parents=True,exist_ok=True)
    ensure_references()
    checks=validate();items=load_sources();rows=[]
    assert (HERE/'output/psx_quant_probe').exists(),'Run the first experiment to create a frozen PS1 quantizer probe'
    for platform,(sw,sh) in previous.PLATFORMS.items():
        print('Processing',platform,flush=True)
        for item in items:
            dw,dh=item['design'];sx=sy=min(sw/dw,sh/dh)
            if platform=='ps2':
                par=(sw/sh)/(4/3);sx=min(sw/dw,sh*par/dh);sy=sx/par
            w,h=item['source_size'];size=(previous.round_size(w*sx),previous.round_size(h*sy));p=item['p'];filtered={};timings={}
            for method,params in METHODS.items():
                if method=='dpid_budget':continue # selected after encoding, below
                t=time.perf_counter();stem=OUT/'variants'/f'{platform}__{item["id"]}__{method}'
                a=previous.lanczos(p,size) if params is None else dpid(p,size,*params,stem)
                assert a.shape==(size[1],size[0],4) and np.all(a[:,:,:3]<=a[:,:,3:4])
                if method=='area':assert np.array_equal(a,previous.area(p,size))
                filtered[method]=a;timings[method]=(time.perf_counter()-t)*1000
                previous.straight(a).save(stem.with_suffix('.filtered.png'))
            base_alpha=float(filtered['area'][:,:,3].sum())
            for lane in ['current','compact']:
                baseline_cost=None
                for method,a in filtered.items():
                    stem=OUT/'variants'/f'{platform}__{item["id"]}__{method}__{lane}';im=previous.straight(a)
                    decoded,meta=previous.encode(im,platform,stem,lane=='compact',lane=='current')
                    decoded.save(stem.with_suffix('.png'))
                    cost=meta['payload_bytes'] if meta['texture_bytes'] is None else meta['texture_bytes']
                    if baseline_cost is None:baseline_cost=cost
                    native=previous.native_3ds(im,stem) if platform=='3ds' and lane=='compact' else None
                    rows.append(dict(platform=platform,lane=lane,source=item['id'],method=method,width=size[0],height=size[1],filter_ms=round(timings[method],3),
                        image=str(stem.with_suffix('.png').relative_to(OUT)),filtered=f'variants/{platform}__{item["id"]}__{method}.filtered.png',
                        alpha_sum_delta_percent=round((float(a[:,:,3].sum())/base_alpha-1)*100,3) if base_alpha else 0,
                        over_budget=cost>baseline_cost,native_3ds=json.dumps(native),**meta))
    rows=add_budget_guard(rows)
    provenance=dict(started=time.strftime('%Y-%m-%dT%H:%M:%S%z'),sources=len(items),cases=len(rows),methods=METHODS,
        dpid_source=json.loads((OUT/'references/provenance.json').read_text()),rust_port_sha256=previous.sha(HERE/'dpid.rs'),
        quantizer_sha256=previous.sha(HERE/'output/quant_snapshot.rs'),checks=checks,
        notes=['No OCR/LLM/labels/fonts in processing','RGBA premultiplied extension is ours, not original paper or VRChat alpha implementation',
        'balanced mix is ours; does not reproduce VRChat conservative mode','dpid_budget reuses balanced or Area encoded pixels after cost comparison; no new encoding',
        'Frozen quantizer preserves earlier run; no shared source modifications','No hardware execution','Current and compact are separate comparisons'])
    make_report(items,rows,provenance)
    print(json.dumps(dict(sources=len(items),cases=len(rows),report=str(OUT/'report.html'),over_budget=sum(r['over_budget'] for r in rows)),indent=2))

if __name__=='__main__':main()
