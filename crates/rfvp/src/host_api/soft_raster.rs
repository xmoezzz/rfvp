//! A small software rasterizer behind [`RfvpRenderer`], for hosts without a GPU path.
//!
//! The engine hands the renderer sprites in *virtual* coordinates (for example 1920x1080). This
//! rasterizer draws straight into an output-sized framebuffer, scaling and letterboxing the
//! virtual frame as it goes, so a 1080p game on a 720p screen never touches 1080p worth of
//! pixels. Axis-aligned sprites (nearly all of them) take a scanline fast path; anything rotated
//! falls back to a per-pixel triangle rasterizer.
//!
//! Pixels are `u32`s whose little-endian bytes are `r, g, b, a`.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
#[cfg(feature = "no_std")]
use core_maths::CoreFloat;

use super::srgb_tables::{FROM_LINEAR, TO_LINEAR};
use super::error::{RfvpError, RfvpResult};
use super::render::{
    BlendMode, ColorRgba, DrawSolidCommand, DrawSpriteCommand, PixelFormat, RectI32, RfvpRenderer,
    TextureDesc, TextureFilter, TextureId, TextureRect, Vertex2D,
};

struct SoftTexture {
    width: u32,
    height: u32,
    /// Format of the bytes handed to `create_texture`/`update_texture`; pixels are kept as RGBA.
    format: PixelFormat,
    pixels: Vec<u32>,
    /// Every pixel has alpha 255.
    opaque: bool,
    /// The texture shrunk by the output scale, built the first time it is drawn at its natural
    /// size on a screen smaller than the game frame (see `SoftRaster::draw_sprite`).
    scaled: Option<ScaledCopy>,
}

struct ScaledCopy {
    /// The output scale this copy was made for.
    scale: f32,
    width: u32,
    height: u32,
    pixels: Vec<u32>,
    opaque: bool,
}

/// What the sampling loops read from: the texture or its scaled copy.
#[derive(Clone, Copy)]
struct TexView<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u32],
    opaque: bool,
}

impl<'a> TexView<'a> {
    fn of(tex: &'a SoftTexture) -> Self {
        Self {
            width: tex.width,
            height: tex.height,
            pixels: &tex.pixels,
            opaque: tex.opaque,
        }
    }
}

fn is_opaque(pixels: &[u32]) -> bool {
    pixels.iter().all(|p| p >> 24 == 255)
}

/// Shrink `src` to `new_width` x `new_height` by averaging the source pixels each destination
/// pixel covers, weighting colors by alpha so transparent pixels do not darken the edges.
fn downscale(src: &SoftTexture, new_width: u32, new_height: u32) -> ScaledCopy {
    fn spans(src_len: u32, dst_len: u32) -> Vec<(u32, Vec<f32>)> {
        let ratio = src_len as f32 / dst_len as f32;
        (0..dst_len)
            .map(|i| {
                let start = i as f32 * ratio;
                let end = (start + ratio).min(src_len as f32);
                let first = start.floor() as u32;
                let last = (end.ceil() as u32).max(first + 1) - 1;
                let weights = (first..=last)
                    .map(|j| ((j + 1) as f32).min(end) - (j as f32).max(start))
                    .map(|w| w.max(0.0))
                    .collect();
                (first, weights)
            })
            .collect()
    }
    let cols = spans(src.width, new_width);
    let rows = spans(src.height, new_height);
    let mut pixels = Vec::with_capacity(new_width as usize * new_height as usize);
    for (y0, wys) in &rows {
        for (x0, wxs) in &cols {
            let (mut sr, mut sg, mut sb, mut sa, mut sw) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (j, wy) in wys.iter().enumerate() {
                let row = (*y0 as usize + j).min(src.height as usize - 1) * src.width as usize;
                for (i, wx) in wxs.iter().enumerate() {
                    let p = src.pixels[row + (*x0 as usize + i).min(src.width as usize - 1)];
                    let w = wy * wx;
                    let wa = w * (p >> 24) as f32;
                    sr += (p & 255) as f32 * wa;
                    sg += ((p >> 8) & 255) as f32 * wa;
                    sb += ((p >> 16) & 255) as f32 * wa;
                    sa += wa;
                    sw += w;
                }
            }
            pixels.push(if sa <= 0.0 || sw <= 0.0 {
                0
            } else {
                pack(
                    ((sr / sa) + 0.5) as u32,
                    ((sg / sa) + 0.5) as u32,
                    ((sb / sa) + 0.5) as u32,
                    ((sa / sw) + 0.5).min(255.0) as u32,
                )
            });
        }
    }
    let opaque = is_opaque(&pixels);
    ScaledCopy {
        scale: 0.0,
        width: new_width,
        height: new_height,
        pixels,
        opaque,
    }
}

/// Shrink RGBA8 pixels (`width` x `height`, rows packed) to `new_width` x `new_height` with the
/// same box filter the rasterizer uses for its shrunk copies: transparent pixels do not bleed
/// their color. For hosts that keep textures at screen resolution.
pub fn downscale_rgba8(
    pixels: &[u8],
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
) -> Vec<u8> {
    let count = width as usize * height as usize;
    let tex = SoftTexture {
        width,
        height,
        format: PixelFormat::Rgba8,
        pixels: (0..count.min(pixels.len() / 4))
            .map(|i| convert_pixel(PixelFormat::Rgba8, pixels, i))
            .collect(),
        opaque: false,
        scaled: None,
    };
    let small = downscale(&tex, new_width.max(1), new_height.max(1));
    let mut out = Vec::with_capacity(small.pixels.len() * 4);
    for p in small.pixels {
        out.extend_from_slice(&p.to_le_bytes());
    }
    out
}

/// Linear blend of two pixels, `w` in `0..=256`, two channels at a time.
#[inline]
fn lerp_pixels(a: u32, b: u32, w: u32) -> u32 {
    let rb = ((a & 0x00ff_00ff) * (256 - w) + (b & 0x00ff_00ff) * w) >> 8;
    let ga = ((a >> 8) & 0x00ff_00ff) * (256 - w) + ((b >> 8) & 0x00ff_00ff) * w;
    (rb & 0x00ff_00ff) | (ga & 0xff00_ff00)
}

/// sRGB byte to linear light, 0..=65535.
#[inline]
fn lin(c: u32) -> u32 {
    TO_LINEAR[c as usize] as u32
}

/// Linear light (0..=65535, larger is clamped) to an sRGB byte.
#[inline]
fn enc(l: u32) -> u32 {
    FROM_LINEAR[(l.min(65535) >> 4) as usize] as u32
}

/// `s`'s color scaled by `a / 255` added to `d`, in linear light; the result is opaque.
#[inline]
fn add_linear(d: u32, s: u32, a: u32) -> u32 {
    if a == 0 {
        return d | 0xff00_0000;
    }
    let ch = |shift: u32| enc(lin((d >> shift) & 255) + lin((s >> shift) & 255) * a / 255);
    pack(ch(0), ch(8), ch(16), 255)
}

/// How a sprite's vertex color changes its texels.
#[derive(Clone, Copy, PartialEq)]
enum Tint {
    /// Texels are used as they are.
    None,
    /// Only the alpha is scaled (a fade).
    Alpha(u32),
    /// Every channel is scaled.
    Full([u32; 4]),
}

/// Draw `texels` onto `dst` (same length) with `mode`.
fn blit_row(dst: &mut [u32], texels: &[u32], mode: BlendMode, tint: Tint, opaque: bool) {
    match (mode, tint) {
        (BlendMode::Alpha | BlendMode::Opaque, Tint::None) if opaque => {
            for (d, &t) in dst.iter_mut().zip(texels) {
                *d = t | 0xff00_0000;
            }
        }
        (BlendMode::Alpha, Tint::None) => {
            for (d, &t) in dst.iter_mut().zip(texels) {
                *d = over(*d, t);
            }
        }
        (BlendMode::Alpha, Tint::Alpha(ta)) => {
            for (d, &t) in dst.iter_mut().zip(texels) {
                let a = div255((t >> 24) * ta);
                *d = over(*d, (t & 0x00ff_ffff) | (a << 24));
            }
        }
        (BlendMode::Add, Tint::None) => {
            for (d, &t) in dst.iter_mut().zip(texels) {
                *d = add_linear(*d, t, t >> 24);
            }
        }
        (BlendMode::Add, Tint::Alpha(ta)) => {
            for (d, &t) in dst.iter_mut().zip(texels) {
                *d = add_linear(*d, t, div255((t >> 24) * ta));
            }
        }
        (mode, Tint::None) => {
            for (d, &t) in dst.iter_mut().zip(texels) {
                *d = blend(*d, channels(t), mode);
            }
        }
        (mode, Tint::Alpha(ta)) => {
            for (d, &t) in dst.iter_mut().zip(texels) {
                let a = div255((t >> 24) * ta);
                *d = blend(*d, channels((t & 0x00ff_ffff) | (a << 24)), mode);
            }
        }
        (mode, Tint::Full(tint)) => {
            for (d, &t) in dst.iter_mut().zip(texels) {
                *d = blend(*d, tint_pixel(t, tint), mode);
            }
        }
    }
}

/// `src` over `dst` with straight alpha, mixed in linear light like the GPU renderer.
#[inline]
fn over(dst: u32, src: u32) -> u32 {
    let sa = src >> 24;
    if sa == 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }
    let ia = 255 - sa;
    let ch = |shift: u32| {
        enc((lin((src >> shift) & 255) * sa + lin((dst >> shift) & 255) * ia) / 255)
    };
    pack(ch(0), ch(8), ch(16), 255)
}

#[inline]
fn pack(r: u32, g: u32, b: u32, a: u32) -> u32 {
    r | (g << 8) | (b << 16) | (a << 24)
}

#[inline]
fn channels(p: u32) -> [u32; 4] {
    [p & 255, (p >> 8) & 255, (p >> 16) & 255, p >> 24]
}

/// Exact `x / 255` for `x <= 255 * 255`.
#[inline]
fn div255(x: u32) -> u32 {
    let x = x + 128;
    (x + (x >> 8)) >> 8
}

#[inline]
fn to_u8(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32
}

fn convert_pixel(format: PixelFormat, src: &[u8], index: usize) -> u32 {
    let b = |i: usize| src[i] as u32;
    match format {
        PixelFormat::Rgba8 => pack(
            b(index * 4),
            b(index * 4 + 1),
            b(index * 4 + 2),
            b(index * 4 + 3),
        ),
        PixelFormat::Bgra8 => pack(
            b(index * 4 + 2),
            b(index * 4 + 1),
            b(index * 4),
            b(index * 4 + 3),
        ),
        PixelFormat::Rgb8 => pack(b(index * 3), b(index * 3 + 1), b(index * 3 + 2), 255),
        PixelFormat::Luma8 => pack(b(index), b(index), b(index), 255),
        PixelFormat::LumaA8 => pack(b(index * 2), b(index * 2), b(index * 2), b(index * 2 + 1)),
        PixelFormat::Alpha8 => pack(255, 255, 255, b(index)),
    }
}

fn source_bytes_per_pixel(format: PixelFormat) -> usize {
    match format {
        PixelFormat::Rgba8 | PixelFormat::Bgra8 => 4,
        PixelFormat::Rgb8 => 3,
        PixelFormat::LumaA8 => 2,
        PixelFormat::Luma8 | PixelFormat::Alpha8 => 1,
    }
}

/// Blend one source pixel (straight alpha, already tinted) over `dst`, in linear light like the
/// GPU renderer (which draws sRGB textures into an sRGB target).
#[inline]
fn blend(dst: u32, src: [u32; 4], mode: BlendMode) -> u32 {
    let [sr, sg, sb, sa] = src;
    match mode {
        BlendMode::Opaque => pack(sr, sg, sb, 255),
        BlendMode::Alpha => over(dst, pack(sr, sg, sb, sa)),
        BlendMode::Add => add_linear(dst, pack(sr, sg, sb, 0), sa),
        BlendMode::Multiply | BlendMode::Screen => {
            let ia = 255 - sa;
            let ch = |d: u32, s: u32| {
                let (ld, ls) = (lin(d), lin(s));
                let mixed = if mode == BlendMode::Multiply {
                    ld * ls / 65535
                } else {
                    65535 - (65535 - ld) * (65535 - ls) / 65535
                };
                enc((ld * ia + mixed * sa) / 255)
            };
            let [dr, dg, db, _] = channels(dst);
            pack(ch(dr, sr), ch(dg, sg), ch(db, sb), 255)
        }
    }
}

#[inline]
fn tint_pixel(texel: u32, tint: [u32; 4]) -> [u32; 4] {
    let [r, g, b, a] = channels(texel);
    [
        div255(r * tint[0]),
        div255(g * tint[1]),
        div255(b * tint[2]),
        div255(a * tint[3]),
    ]
}

/// Bilinear sample at texel-space coordinates (`tx`, `ty` are in texels, pixel centers at +0.5).
#[inline]
fn sample_bilinear(tex: TexView<'_>, tx: f32, ty: f32) -> u32 {
    let fx = tx - 0.5;
    let fy = ty - 0.5;
    let x0f = fx.floor();
    let y0f = fy.floor();
    let wx = ((fx - x0f) * 256.0) as u32;
    let wy = ((fy - y0f) * 256.0) as u32;
    let max_x = tex.width as i32 - 1;
    let max_y = tex.height as i32 - 1;
    let x0 = (x0f as i32).clamp(0, max_x) as usize;
    let x1 = (x0f as i32 + 1).clamp(0, max_x) as usize;
    let y0 = (y0f as i32).clamp(0, max_y) as usize;
    let y1 = (y0f as i32 + 1).clamp(0, max_y) as usize;
    let w = tex.width as usize;
    let top = lerp_pixels(tex.pixels[y0 * w + x0], tex.pixels[y0 * w + x1], wx);
    let bottom = lerp_pixels(tex.pixels[y1 * w + x0], tex.pixels[y1 * w + x1], wx);
    lerp_pixels(top, bottom, wy)
}

#[inline]
fn sample_nearest(tex: TexView<'_>, tx: f32, ty: f32) -> u32 {
    let x = (tx as i32).clamp(0, tex.width as i32 - 1) as usize;
    let y = (ty as i32).clamp(0, tex.height as i32 - 1) as usize;
    tex.pixels[y * tex.width as usize + x]
}

pub struct SoftRaster {
    textures: BTreeMap<u32, SoftTexture>,
    out_width: u32,
    out_height: u32,
    frame: Vec<u32>,
    virt_width: u32,
    virt_height: u32,
    scale: f32,
    off_x: f32,
    off_y: f32,
}

impl SoftRaster {
    /// A rasterizer drawing into an `out_width` x `out_height` framebuffer.
    pub fn new(out_width: u32, out_height: u32) -> Self {
        let mut raster = Self {
            textures: BTreeMap::new(),
            out_width: out_width.max(1),
            out_height: out_height.max(1),
            frame: Vec::new(),
            virt_width: 0,
            virt_height: 0,
            scale: 1.0,
            off_x: 0.0,
            off_y: 0.0,
        };
        raster.frame.resize(
            raster.out_width as usize * raster.out_height as usize,
            pack(0, 0, 0, 255),
        );
        raster
    }

    pub fn output_size(&self) -> (u32, u32) {
        (self.out_width, self.out_height)
    }

    /// The finished frame, `output_size()` pixels in row-major order.
    pub fn frame(&self) -> &[u32] {
        &self.frame
    }

    /// Number of live textures and their total size in bytes.
    pub fn texture_stats(&self) -> (usize, usize) {
        let bytes = self.textures.values().map(|t| t.pixels.len() * 4).sum();
        (self.textures.len(), bytes)
    }

    /// The game frame drawn last (without the letterbox bars) averaged down to `width` x
    /// `height`, as RGBA8 bytes.
    pub fn thumbnail(&self, width: u32, height: u32) -> Vec<u8> {
        let width = width.max(1);
        let height = height.max(1);
        let (x0, y0, x1, y1) = self.clip_rect(None);
        let (cw, ch) = ((x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32);
        let mut out = Vec::with_capacity(width as usize * height as usize * 4);
        for ty in 0..height {
            let sy0 = y0 as u32 + ty * ch / height;
            let sy1 = (y0 as u32 + (ty + 1) * ch / height).max(sy0 + 1);
            for tx in 0..width {
                let sx0 = x0 as u32 + tx * cw / width;
                let sx1 = (x0 as u32 + (tx + 1) * cw / width).max(sx0 + 1);
                let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
                for sy in sy0..sy1.min(self.out_height) {
                    for sx in sx0..sx1.min(self.out_width) {
                        let p = self.frame[(sy * self.out_width + sx) as usize];
                        r += p & 255;
                        g += (p >> 8) & 255;
                        b += (p >> 16) & 255;
                        n += 1;
                    }
                }
                let n = n.max(1);
                out.extend_from_slice(&[(r / n) as u8, (g / n) as u8, (b / n) as u8, 255]);
            }
        }
        out
    }

    /// Map a point on the output (screen) to virtual coordinates.
    pub fn output_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        if self.scale <= 0.0 {
            return (x, y);
        }
        ((x - self.off_x) / self.scale, (y - self.off_y) / self.scale)
    }

    /// Map virtual coordinates to a point on the output (screen).
    pub fn virtual_to_output(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale + self.off_x, y * self.scale + self.off_y)
    }

    fn update_mapping(&mut self, virt_width: u32, virt_height: u32) {
        self.virt_width = virt_width;
        self.virt_height = virt_height;
        let sx = self.out_width as f32 / virt_width as f32;
        let sy = self.out_height as f32 / virt_height as f32;
        self.scale = sx.min(sy);
        self.off_x = (self.out_width as f32 - virt_width as f32 * self.scale) * 0.5;
        self.off_y = (self.out_height as f32 - virt_height as f32 * self.scale) * 0.5;
    }

    #[inline]
    fn map(&self, v: &Vertex2D) -> (f32, f32) {
        (
            v.position[0] * self.scale + self.off_x,
            v.position[1] * self.scale + self.off_y,
        )
    }

    /// The part of the output the virtual frame covers, clipped by an optional scissor given in
    /// virtual coordinates.
    fn clip_rect(&self, scissor: Option<RectI32>) -> (i32, i32, i32, i32) {
        let mut x0 = self.off_x.round() as i32;
        let mut y0 = self.off_y.round() as i32;
        let mut x1 = (self.off_x + self.virt_width as f32 * self.scale).round() as i32;
        let mut y1 = (self.off_y + self.virt_height as f32 * self.scale).round() as i32;
        if let Some(s) = scissor {
            let sx0 = (s.x as f32 * self.scale + self.off_x).floor() as i32;
            let sy0 = (s.y as f32 * self.scale + self.off_y).floor() as i32;
            let sx1 = ((s.x + s.width) as f32 * self.scale + self.off_x).ceil() as i32;
            let sy1 = ((s.y + s.height) as f32 * self.scale + self.off_y).ceil() as i32;
            x0 = x0.max(sx0);
            y0 = y0.max(sy0);
            x1 = x1.min(sx1);
            y1 = y1.min(sy1);
        }
        (
            x0.max(0),
            y0.max(0),
            x1.min(self.out_width as i32),
            y1.min(self.out_height as i32),
        )
    }

    fn draw_axis_aligned(
        &mut self,
        tex: TexView<'_>,
        quad: [(f32, f32); 4],
        uvs: [[f32; 2]; 4],
        tint: [u32; 4],
        mode: BlendMode,
        linear: bool,
        clip: (i32, i32, i32, i32),
    ) {
        // Vertex order is the engine's: 0 = left/bottom, 1 = left/top, 2 = right/bottom,
        // 3 = right/top.
        let (xl, yb) = quad[0];
        let (xr, yt) = quad[3];
        let (u_left, v_bottom) = (uvs[0][0], uvs[0][1]);
        let (u_right, v_top) = (uvs[3][0], uvs[3][1]);

        let (xmin, xmax) = if xl <= xr { (xl, xr) } else { (xr, xl) };
        let (ymin, ymax) = if yt <= yb { (yt, yb) } else { (yb, yt) };
        let dx = xr - xl;
        let dy = yb - yt;
        if dx.abs() < 1e-6 || dy.abs() < 1e-6 {
            return;
        }

        // Pixels whose centers are inside the rectangle.
        let px0 = ((xmin - 0.5).ceil() as i32).max(clip.0);
        let px1 = ((xmax - 0.5).ceil() as i32).min(clip.2);
        let py0 = ((ymin - 0.5).ceil() as i32).max(clip.1);
        let py1 = ((ymax - 0.5).ceil() as i32).min(clip.3);
        if px0 >= px1 || py0 >= py1 {
            return;
        }

        let tw = tex.width as f32;
        let th = tex.height as f32;
        // Texel-space step per output pixel.
        let du = (u_right - u_left) * tw / dx;
        let dv = (v_bottom - v_top) * th / dy;
        let first_tx = (u_left + (u_right - u_left) * ((px0 as f32 + 0.5 - xl) / dx)) * tw;
        let first_ty = (v_top + (v_bottom - v_top) * ((py0 as f32 + 0.5 - yt) / dy)) * th;
        // A copy at (nearly) the pixel grid is exact with a plain lookup and much cheaper.
        let one_to_one = (du - 1.0).abs() < 1e-3
            && (dv - 1.0).abs() < 1e-3
            && (first_tx - first_tx.floor() - 0.5).abs() < 1e-2
            && (first_ty - first_ty.floor() - 0.5).abs() < 1e-2;
        // A scaled-down copy is already at the right size, so a lookup is enough there too.
        let near_one = (du - 1.0).abs() < 0.01 && (dv - 1.0).abs() < 0.01;
        let use_bilinear = linear && !one_to_one && !near_one;

        let tint_kind = if tint == [255, 255, 255, 255] {
            Tint::None
        } else if tint[0] == 255 && tint[1] == 255 && tint[2] == 255 {
            Tint::Alpha(tint[3])
        } else {
            Tint::Full(tint)
        };
        let width = (px1 - px0) as usize;
        let tex_w = tex.width as usize;
        let max_x = tex.width as i32 - 1;
        let max_y = tex.height as i32 - 1;

        // One texel per output pixel: every row is a straight run of the texture.
        let first_x = first_tx.floor() as i32;
        if !use_bilinear && first_x >= 0 && first_x as usize + width <= tex_w {
            let xs = first_x as usize;
            for py in py0..py1 {
                let ty = first_ty + (py - py0) as f32 * dv;
                let y = (ty as i32).clamp(0, max_y) as usize;
                let texels = &tex.pixels[y * tex_w + xs..y * tex_w + xs + width];
                let out_row = py as usize * self.out_width as usize;
                let dst = &mut self.frame[out_row + px0 as usize..out_row + px1 as usize];
                blit_row(dst, texels, mode, tint_kind, tex.opaque);
            }
            return;
        }

        // Per-column sampling positions, shared by every row.
        let mut cols: Vec<(u32, u32, u32)> = Vec::with_capacity(width);
        for i in 0..width {
            let tx = first_tx + i as f32 * du;
            if use_bilinear {
                let fx = tx - 0.5;
                let x0f = fx.floor();
                cols.push((
                    (x0f as i32).clamp(0, max_x) as u32,
                    (x0f as i32 + 1).clamp(0, max_x) as u32,
                    ((fx - x0f) * 256.0) as u32,
                ));
            } else {
                let x = (tx as i32).clamp(0, max_x) as u32;
                cols.push((x, x, 0));
            }
        }

        let mut row_texels: Vec<u32> = vec![0; width];
        for py in py0..py1 {
            let ty = first_ty + (py - py0) as f32 * dv;
            let (y0, y1, wy) = if use_bilinear {
                let fy = ty - 0.5;
                let y0f = fy.floor();
                (
                    (y0f as i32).clamp(0, max_y) as usize,
                    (y0f as i32 + 1).clamp(0, max_y) as usize,
                    ((fy - y0f) * 256.0) as u32,
                )
            } else {
                let y = (ty as i32).clamp(0, max_y) as usize;
                (y, y, 0)
            };
            let row0 = &tex.pixels[y0 * tex_w..y0 * tex_w + tex_w];
            let row1 = &tex.pixels[y1 * tex_w..y1 * tex_w + tex_w];
            for (texel, &(x0, x1, wx)) in row_texels.iter_mut().zip(cols.iter()) {
                *texel = if use_bilinear {
                    let top = lerp_pixels(row0[x0 as usize], row0[x1 as usize], wx);
                    let bottom = lerp_pixels(row1[x0 as usize], row1[x1 as usize], wx);
                    lerp_pixels(top, bottom, wy)
                } else {
                    row0[x0 as usize]
                };
            }
            let out_row = py as usize * self.out_width as usize;
            let dst = &mut self.frame[out_row + px0 as usize..out_row + px1 as usize];
            blit_row(dst, &row_texels, mode, tint_kind, tex.opaque);
        }
    }

    fn draw_triangle(
        &mut self,
        tex: TexView<'_>,
        tri: [(f32, f32, [f32; 2], ColorRgba); 3],
        mode: BlendMode,
        linear: bool,
        clip: (i32, i32, i32, i32),
    ) {
        let [a, b, c] = tri;
        let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if area.abs() < 1e-6 {
            return;
        }
        let inv = 1.0 / area;
        let min_x = (a.0.min(b.0).min(c.0).floor() as i32).max(clip.0);
        let max_x = (a.0.max(b.0).max(c.0).ceil() as i32).min(clip.2);
        let min_y = (a.1.min(b.1).min(c.1).floor() as i32).max(clip.1);
        let max_y = (a.1.max(b.1).max(c.1).ceil() as i32).min(clip.3);
        let tw = tex.width as f32;
        let th = tex.height as f32;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = ((b.0 - px) * (c.1 - py) - (b.1 - py) * (c.0 - px)) * inv;
                let w1 = ((c.0 - px) * (a.1 - py) - (c.1 - py) * (a.0 - px)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let u = a.2[0] * w0 + b.2[0] * w1 + c.2[0] * w2;
                let v = a.2[1] * w0 + b.2[1] * w1 + c.2[1] * w2;
                let tx = u * tw;
                let ty = v * th;
                let texel = if linear {
                    sample_bilinear(tex, tx, ty)
                } else {
                    sample_nearest(tex, tx, ty)
                };
                let tint = [
                    to_u8(a.3.r * w0 + b.3.r * w1 + c.3.r * w2),
                    to_u8(a.3.g * w0 + b.3.g * w1 + c.3.g * w2),
                    to_u8(a.3.b * w0 + b.3.b * w1 + c.3.b * w2),
                    to_u8(a.3.a * w0 + b.3.a * w1 + c.3.a * w2),
                ];
                let idx = y as usize * self.out_width as usize + x as usize;
                self.frame[idx] = blend(self.frame[idx], tint_pixel(texel, tint), mode);
            }
        }
    }
}

impl RfvpRenderer for SoftRaster {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        let count = (desc.width as usize)
            .checked_mul(desc.height as usize)
            .ok_or(RfvpError::CapacityExceeded)?;
        let bpp = source_bytes_per_pixel(desc.format);
        let mut out = vec![0u32; count];
        if let Some(src) = pixels {
            if src.len() < count * bpp {
                return Err(RfvpError::InvalidArgument);
            }
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = convert_pixel(desc.format, src, i);
            }
        }
        let opaque = is_opaque(&out);
        self.textures.insert(
            id.0,
            SoftTexture {
                width: desc.width,
                height: desc.height,
                format: desc.format,
                pixels: out,
                opaque,
                scaled: None,
            },
        );
        Ok(())
    }

    fn update_texture(
        &mut self,
        id: TextureId,
        rect: TextureRect,
        pixels: &[u8],
    ) -> RfvpResult<()> {
        let tex = self.textures.get_mut(&id.0).ok_or(RfvpError::NotFound)?;
        let format = tex.format;
        if rect
            .x
            .checked_add(rect.width)
            .ok_or(RfvpError::InvalidArgument)?
            > tex.width
            || rect
                .y
                .checked_add(rect.height)
                .ok_or(RfvpError::InvalidArgument)?
                > tex.height
        {
            return Err(RfvpError::InvalidArgument);
        }
        let bpp = source_bytes_per_pixel(format);
        if pixels.len() < rect.width as usize * rect.height as usize * bpp {
            return Err(RfvpError::InvalidArgument);
        }
        let mut all_opaque = true;
        for row in 0..rect.height as usize {
            for col in 0..rect.width as usize {
                let dst = (rect.y as usize + row) * tex.width as usize + rect.x as usize + col;
                let pixel = convert_pixel(format, pixels, row * rect.width as usize + col);
                all_opaque &= pixel >> 24 == 255;
                tex.pixels[dst] = pixel;
            }
        }
        // The pixels changed, so the shrunk copy is stale.
        tex.scaled = None;
        tex.opaque = if all_opaque { tex.opaque } else { false };
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        self.textures.remove(&id.0);
    }

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        if width == 0 || height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        if width != self.virt_width || height != self.virt_height {
            self.update_mapping(width, height);
        }
        let black = pack(0, 0, 0, 255);
        self.frame.fill(black);
        if let Some(c) = clear {
            let color = pack(to_u8(c.r), to_u8(c.g), to_u8(c.b), 255);
            let (x0, y0, x1, y1) = self.clip_rect(None);
            for y in y0..y1 {
                let row = y as usize * self.out_width as usize;
                self.frame[row + x0 as usize..row + x1.max(x0) as usize].fill(color);
            }
        }
        Ok(())
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        // A texture that is not there yet is skipped rather than failing the whole frame.
        let Some(mut tex) = self.textures.remove(&command.texture.0) else {
            return Ok(());
        };
        let v = &command.vertices;
        let quad = [
            self.map(&v[0]),
            self.map(&v[1]),
            self.map(&v[2]),
            self.map(&v[3]),
        ];
        let clip = self.clip_rect(command.scissor);
        let linear = command.filter == TextureFilter::Linear;
        let eq = |a: f32, b: f32| (a - b).abs() < 1e-3;
        let same_color = v.iter().all(|vx| vx.color == v[0].color);
        let aligned = eq(quad[0].0, quad[1].0)
            && eq(quad[2].0, quad[3].0)
            && eq(quad[0].1, quad[2].1)
            && eq(quad[1].1, quad[3].1)
            && eq(v[0].tex_coord[0], v[1].tex_coord[0])
            && eq(v[2].tex_coord[0], v[3].tex_coord[0])
            && eq(v[0].tex_coord[1], v[2].tex_coord[1])
            && eq(v[1].tex_coord[1], v[3].tex_coord[1]);

        if aligned && same_color {
            let c = v[0].color;
            let tint = [to_u8(c.r), to_u8(c.g), to_u8(c.b), to_u8(c.a)];
            let uvs = [
                v[0].tex_coord,
                v[1].tex_coord,
                v[2].tex_coord,
                v[3].tex_coord,
            ];

            // Drawn at its natural size on a screen smaller than the game frame, the texture is
            // looked up in a copy shrunk once, instead of being filtered down on every frame.
            let natural_w = (uvs[3][0] - uvs[0][0]).abs() * tex.width as f32 * self.scale;
            let natural_h = (uvs[0][1] - uvs[1][1]).abs() * tex.height as f32 * self.scale;
            let drawn_w = (quad[3].0 - quad[0].0).abs();
            let drawn_h = (quad[0].1 - quad[3].1).abs();
            let at_natural_size = natural_w > 0.0
                && natural_h > 0.0
                && (drawn_w - natural_w).abs() <= natural_w * 0.005 + 0.5
                && (drawn_h - natural_h).abs() <= natural_h * 0.005 + 0.5;
            if linear && self.scale < 0.999 && at_natural_size {
                let stale = tex
                    .scaled
                    .as_ref()
                    .map_or(true, |s| (s.scale - self.scale).abs() > 1e-4);
                if stale {
                    let nw = ((tex.width as f32 * self.scale).round() as u32).max(1);
                    let nh = ((tex.height as f32 * self.scale).round() as u32).max(1);
                    let mut copy = downscale(&tex, nw, nh);
                    copy.scale = self.scale;
                    tex.scaled = Some(copy);
                }
            }
            let scaled = tex.scaled.as_ref().filter(|s| {
                linear
                    && self.scale < 0.999
                    && at_natural_size
                    && (s.scale - self.scale).abs() <= 1e-4
            });
            let view = match scaled {
                Some(s) => TexView {
                    width: s.width,
                    height: s.height,
                    pixels: &s.pixels,
                    opaque: s.opaque,
                },
                None => TexView::of(&tex),
            };
            self.draw_axis_aligned(view, quad, uvs, tint, command.blend, linear, clip);
        } else {
            // The engine's vertex order is a strip: (0, 1, 2) and (2, 1, 3).
            let vert = |i: usize| (quad[i].0, quad[i].1, v[i].tex_coord, v[i].color);
            let view = TexView::of(&tex);
            self.draw_triangle(
                view,
                [vert(0), vert(1), vert(2)],
                command.blend,
                linear,
                clip,
            );
            self.draw_triangle(
                view,
                [vert(2), vert(1), vert(3)],
                command.blend,
                linear,
                clip,
            );
        }
        self.textures.insert(command.texture.0, tex);
        Ok(())
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        let (x0, y0, x1, y1) = self.clip_rect(command.scissor);
        let r = command.rect;
        let rx0 = ((r.x as f32 * self.scale + self.off_x).round() as i32).max(x0);
        let ry0 = ((r.y as f32 * self.scale + self.off_y).round() as i32).max(y0);
        let rx1 = (((r.x + r.width) as f32 * self.scale + self.off_x).round() as i32).min(x1);
        let ry1 = (((r.y + r.height) as f32 * self.scale + self.off_y).round() as i32).min(y1);
        let c = command.color;
        let src = [to_u8(c.r), to_u8(c.g), to_u8(c.b), to_u8(c.a)];
        for y in ry0..ry1 {
            let row = y as usize * self.out_width as usize;
            for slot in
                &mut self.frame[row + rx0.max(0) as usize..row + rx1.max(rx0.max(0)) as usize]
            {
                *slot = blend(*slot, src, command.blend);
            }
        }
        Ok(())
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        Ok(())
    }

    fn present(&mut self) -> RfvpResult<()> {
        Ok(())
    }

    fn capture_thumbnail(&mut self, width: u32, height: u32) -> Option<Vec<u8>> {
        Some(self.thumbnail(width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::render::Vertex2D;

    fn vertex(x: f32, y: f32, u: f32, v: f32, color: ColorRgba) -> Vertex2D {
        Vertex2D {
            position: [x, y],
            tex_coord: [u, v],
            color,
        }
    }

    const WHITE: ColorRgba = ColorRgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    /// The engine's vertex order: left/bottom, left/top, right/bottom, right/top.
    fn quad(x0: f32, y0: f32, x1: f32, y1: f32) -> [Vertex2D; 4] {
        [
            vertex(x0, y1, 0.0, 1.0, WHITE),
            vertex(x0, y0, 0.0, 0.0, WHITE),
            vertex(x1, y1, 1.0, 1.0, WHITE),
            vertex(x1, y0, 1.0, 0.0, WHITE),
        ]
    }

    fn solid_texture(raster: &mut SoftRaster, id: u32, w: u32, h: u32, rgba: [u8; 4]) {
        let mut data = Vec::new();
        for _ in 0..w * h {
            data.extend_from_slice(&rgba);
        }
        raster
            .create_texture(
                TextureId(id),
                TextureDesc {
                    width: w,
                    height: h,
                    format: PixelFormat::Rgba8,
                    mip_count: 1,
                },
                Some(&data),
            )
            .unwrap();
    }

    fn sprite(texture: u32, vertices: [Vertex2D; 4], blend: BlendMode) -> DrawSpriteCommand {
        DrawSpriteCommand {
            texture: TextureId(texture),
            vertices,
            blend,
            filter: TextureFilter::Linear,
            scissor: None,
        }
    }

    fn at(raster: &SoftRaster, x: u32, y: u32) -> [u32; 4] {
        channels(raster.frame()[(y * raster.output_size().0 + x) as usize])
    }

    #[test]
    fn axis_aligned_sprite_covers_every_pixel() {
        let mut r = SoftRaster::new(64, 48);
        solid_texture(&mut r, 1, 4, 4, [200, 100, 50, 255]);
        r.begin_frame(64, 48, None).unwrap();
        r.draw_sprite(&sprite(1, quad(8.0, 8.0, 40.0, 30.0), BlendMode::Alpha))
            .unwrap();
        for y in 8..30 {
            for x in 8..40 {
                assert_eq!(at(&r, x, y), [200, 100, 50, 255], "pixel {x},{y}");
            }
        }
        assert_eq!(at(&r, 7, 8), [0, 0, 0, 255]);
        assert_eq!(at(&r, 40, 8), [0, 0, 0, 255]);
        assert_eq!(at(&r, 8, 30), [0, 0, 0, 255]);
    }

    #[test]
    fn rotated_sprite_has_no_hole() {
        // A diamond: the strip order (0,1,2)+(2,1,3) must cover its whole interior. With the
        // fan order (0,1,2)+(0,2,3) a triangle at the top of the quad was left undrawn.
        let mut r = SoftRaster::new(64, 64);
        solid_texture(&mut r, 1, 2, 2, [255, 255, 255, 255]);
        r.begin_frame(64, 64, None).unwrap();
        let v = [
            vertex(32.0, 56.0, 0.0, 1.0, WHITE), // bottom
            vertex(8.0, 32.0, 0.0, 0.0, WHITE),  // left
            vertex(56.0, 32.0, 1.0, 1.0, WHITE), // right
            vertex(32.0, 8.0, 1.0, 0.0, WHITE),  // top
        ];
        r.draw_sprite(&sprite(1, v, BlendMode::Alpha)).unwrap();
        // Inside the diamond, including the part near the top vertex.
        for (x, y) in [
            (32, 32),
            (32, 14),
            (32, 50),
            (14, 32),
            (50, 32),
            (26, 20),
            (38, 20),
        ] {
            assert_eq!(at(&r, x, y), [255, 255, 255, 255], "pixel {x},{y}");
        }
        // Outside it.
        assert_eq!(at(&r, 10, 10), [0, 0, 0, 255]);
        assert_eq!(at(&r, 54, 54), [0, 0, 0, 255]);
    }

    #[test]
    fn virtual_frame_is_scaled_and_letterboxed() {
        // 4:3 virtual frame on a 16:9 screen: 0.5x scale, 40px bars at the sides.
        let mut r = SoftRaster::new(160, 90);
        solid_texture(&mut r, 1, 2, 2, [255, 0, 0, 255]);
        r.begin_frame(
            160,
            120,
            Some(ColorRgba {
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            }),
        )
        .unwrap();
        // scale = min(160/160, 90/120) = 0.75, so the frame is 120x90, offset 20 horizontally.
        assert_eq!(at(&r, 5, 45), [0, 0, 0, 255]);
        assert_eq!(at(&r, 80, 45), [0, 0, 255, 255]);
        r.draw_sprite(&sprite(1, quad(0.0, 0.0, 160.0, 120.0), BlendMode::Alpha))
            .unwrap();
        assert_eq!(at(&r, 80, 45), [255, 0, 0, 255]);
        assert_eq!(at(&r, 5, 45), [0, 0, 0, 255]);
        let (vx, vy) = r.output_to_virtual(80.0, 45.0);
        assert!((vx - 80.0).abs() < 0.01 && (vy - 60.0).abs() < 0.01);
    }

    #[test]
    fn additive_blend_adds_to_the_background() {
        let mut r = SoftRaster::new(16, 16);
        solid_texture(&mut r, 1, 2, 2, [100, 50, 0, 255]);
        r.begin_frame(
            16,
            16,
            Some(ColorRgba {
                r: 0.2,
                g: 0.2,
                b: 0.2,
                a: 1.0,
            }),
        )
        .unwrap();
        let before = at(&r, 8, 8);
        r.draw_sprite(&sprite(1, quad(0.0, 0.0, 16.0, 16.0), BlendMode::Add))
            .unwrap();
        let after = at(&r, 8, 8);
        // Light adds up in linear light: 51 + 100 and 51 + 50 in sRGB bytes.
        assert_eq!(before[0], 51);
        assert!((after[0] as i32 - 112).abs() <= 1, "{after:?}");
        assert!((after[1] as i32 - 72).abs() <= 1, "{after:?}");
        assert_eq!(after[2], before[2]);
    }

    #[test]
    fn alpha_blend_mixes_with_the_background() {
        let mut r = SoftRaster::new(16, 16);
        solid_texture(&mut r, 1, 2, 2, [255, 255, 255, 128]);
        r.begin_frame(16, 16, None).unwrap();
        r.draw_sprite(&sprite(1, quad(0.0, 0.0, 16.0, 16.0), BlendMode::Alpha))
            .unwrap();
        let px = at(&r, 8, 8);
        // Half the light of white over black, in sRGB.
        assert!((px[0] as i32 - 188).abs() <= 1, "{px:?}");
    }

    #[test]
    fn one_to_one_blit_copies_texels_exactly() {
        let mut r = SoftRaster::new(8, 8);
        let mut data = Vec::new();
        for i in 0..16u32 {
            data.extend_from_slice(&[(i * 16) as u8, 0, 0, 255]);
        }
        r.create_texture(
            TextureId(1),
            TextureDesc {
                width: 4,
                height: 4,
                format: PixelFormat::Rgba8,
                mip_count: 1,
            },
            Some(&data),
        )
        .unwrap();
        r.begin_frame(8, 8, None).unwrap();
        r.draw_sprite(&sprite(1, quad(2.0, 2.0, 6.0, 6.0), BlendMode::Alpha))
            .unwrap();
        for y in 0..4u32 {
            for x in 0..4u32 {
                assert_eq!(at(&r, 2 + x, 2 + y)[0], (y * 4 + x) * 16, "texel {x},{y}");
            }
        }
    }

    #[test]
    fn missing_texture_is_skipped() {
        let mut r = SoftRaster::new(8, 8);
        r.begin_frame(8, 8, None).unwrap();
        r.draw_sprite(&sprite(99, quad(0.0, 0.0, 8.0, 8.0), BlendMode::Alpha))
            .unwrap();
    }

    #[test]
    fn packed_over_matches_the_channel_blend() {
        let values = [0u32, 1, 17, 64, 127, 128, 200, 254, 255];
        for &r in &values {
            for &a in &values {
                for &d in &values {
                    let dst = pack(d, 255 - d, d / 2, 255);
                    let src = pack(r, 255 - r, r / 3, a);
                    let packed = channels(over(dst, src));
                    let reference = channels(blend(dst, channels(src), BlendMode::Alpha));
                    for k in 0..3 {
                        assert!(
                            (packed[k] as i32 - reference[k] as i32).abs() <= 1,
                            "dst {dst:08x} src {src:08x} channel {k}: {packed:?} vs {reference:?}"
                        );
                    }
                    assert_eq!(packed[3], 255);
                }
            }
        }
    }

    #[test]
    fn lerp_pixels_hits_both_ends_and_the_middle() {
        let a = pack(0, 100, 200, 255);
        let b = pack(200, 0, 100, 0);
        assert_eq!(lerp_pixels(a, b, 0), a);
        assert_eq!(channels(lerp_pixels(a, b, 128)), [100, 50, 150, 127]);
    }

    #[test]
    fn downscale_averages_and_ignores_the_color_of_transparent_pixels() {
        let tex = SoftTexture {
            width: 2,
            height: 2,
            format: PixelFormat::Rgba8,
            // One opaque red pixel and three fully transparent ones with junk colors.
            pixels: vec![
                pack(255, 0, 0, 255),
                pack(0, 255, 0, 0),
                pack(0, 0, 255, 0),
                pack(9, 9, 9, 0),
            ],
            opaque: false,
            scaled: None,
        };
        let copy = downscale(&tex, 1, 1);
        let [r, g, b, a] = channels(copy.pixels[0]);
        assert_eq!(
            [r, g, b],
            [255, 0, 0],
            "transparent pixels must not tint the result"
        );
        assert!((a as i32 - 64).abs() <= 1, "alpha {a}");
    }

    #[test]
    fn downscale_of_a_flat_color_keeps_the_color() {
        let tex = SoftTexture {
            width: 7,
            height: 5,
            format: PixelFormat::Rgba8,
            pixels: vec![pack(10, 200, 30, 255); 35],
            opaque: true,
            scaled: None,
        };
        let copy = downscale(&tex, 3, 2);
        assert_eq!((copy.width, copy.height), (3, 2));
        assert!(copy.opaque);
        assert!(copy
            .pixels
            .iter()
            .all(|p| channels(*p) == [10, 200, 30, 255]));
    }

    #[test]
    fn natural_size_sprites_use_a_shrunk_copy_on_a_smaller_screen() {
        // A 200x100 game frame shown on a 100x50 screen (scale 0.5).
        let mut r = SoftRaster::new(100, 50);
        solid_texture(&mut r, 1, 40, 40, [30, 60, 90, 255]);
        r.begin_frame(200, 100, None).unwrap();
        r.draw_sprite(&sprite(1, quad(20.0, 20.0, 60.0, 60.0), BlendMode::Alpha))
            .unwrap();
        assert!(
            r.textures.get(&1).unwrap().scaled.is_some(),
            "no shrunk copy was made"
        );
        // The sprite covers 20x20 output pixels starting at (10, 10).
        assert_eq!(at(&r, 10, 10), [30, 60, 90, 255]);
        assert_eq!(at(&r, 29, 29), [30, 60, 90, 255]);
        assert_eq!(at(&r, 9, 10), [0, 0, 0, 255]);
        assert_eq!(at(&r, 30, 29), [0, 0, 0, 255]);

        // Drawn at another size, the original texture is used instead.
        r.begin_frame(200, 100, None).unwrap();
        r.draw_sprite(&sprite(1, quad(0.0, 0.0, 100.0, 100.0), BlendMode::Alpha))
            .unwrap();
        assert_eq!(at(&r, 25, 25), [30, 60, 90, 255]);
    }

    #[test]
    fn updating_a_texture_drops_its_shrunk_copy() {
        let mut r = SoftRaster::new(100, 50);
        solid_texture(&mut r, 1, 40, 40, [30, 60, 90, 255]);
        r.begin_frame(200, 100, None).unwrap();
        r.draw_sprite(&sprite(1, quad(20.0, 20.0, 60.0, 60.0), BlendMode::Alpha))
            .unwrap();
        assert!(r.textures.get(&1).unwrap().scaled.is_some());
        r.update_texture(
            TextureId(1),
            TextureRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &[255, 0, 0, 255],
        )
        .unwrap();
        assert!(r.textures.get(&1).unwrap().scaled.is_none());
    }

    #[test]
    fn blending_happens_in_linear_light_like_the_gpu_renderer() {
        // Half-transparent black over white: half the light, which is sRGB 188, not 128.
        let half_black = channels(over(pack(255, 255, 255, 255), pack(0, 0, 0, 128)));
        assert_eq!(half_black, [187, 187, 187, 255]);
        // The ends are exact.
        assert_eq!(over(pack(10, 20, 30, 255), pack(1, 2, 3, 0)), pack(10, 20, 30, 255));
        assert_eq!(over(pack(10, 20, 30, 255), pack(1, 2, 3, 255)), pack(1, 2, 3, 255));
        // Adding: nothing at zero alpha, light adds up and clamps.
        assert_eq!(add_linear(pack(40, 50, 60, 255), pack(255, 255, 255, 0), 0), pack(40, 50, 60, 255));
        assert_eq!(channels(add_linear(pack(188, 0, 255, 255), pack(188, 255, 255, 0), 255)), [255, 255, 255, 255]);
        // Every byte survives a trip through linear light.
        for c in 0..=255 {
            assert_eq!(enc(lin(c)), c);
        }
    }

    #[test]
    fn faded_sprites_blend_like_the_plain_path() {
        // A sprite drawn with 50% alpha tint over a background.
        let mut r = SoftRaster::new(16, 16);
        solid_texture(&mut r, 1, 2, 2, [200, 100, 0, 255]);
        r.begin_frame(
            16,
            16,
            Some(ColorRgba {
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            }),
        )
        .unwrap();
        let half = ColorRgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.5,
        };
        let mut v = quad(0.0, 0.0, 16.0, 16.0);
        for vertex in &mut v {
            vertex.color = half;
        }
        r.draw_sprite(&sprite(1, v, BlendMode::Alpha)).unwrap();
        let px = at(&r, 8, 8);
        // Half of each in linear light, as the GPU renderer mixes them.
        assert!((px[0] as i32 - 146).abs() <= 2, "{px:?}");
        assert!((px[1] as i32 - 71).abs() <= 2, "{px:?}");
        assert!((px[2] as i32 - 188).abs() <= 2, "{px:?}");
    }

    #[test]
    fn thumbnail_averages_the_content_and_skips_the_bars() {
        // 160x90 screen showing a 4:3 frame: bars at the sides.
        let mut r = SoftRaster::new(160, 90);
        solid_texture(&mut r, 1, 2, 2, [200, 100, 0, 255]);
        r.begin_frame(160, 120, None).unwrap();
        r.draw_sprite(&sprite(1, quad(0.0, 0.0, 160.0, 120.0), BlendMode::Alpha))
            .unwrap();
        let thumb = r.thumbnail(8, 6);
        assert_eq!(thumb.len(), 8 * 6 * 4);
        for px in thumb.chunks_exact(4) {
            assert_eq!(
                px,
                [200, 100, 0, 255],
                "the bars must not bleed into the thumbnail"
            );
        }
    }
}
