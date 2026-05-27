use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use nx::gpu;
use nx::gpu::canvas::{CanvasManager, RGBA8};
use nx::sync::RwLock;
use rfvp::host_api::{
    BlendMode, ColorRgba, DrawSolidCommand, DrawSpriteCommand, PixelFormat, RectI32, RfvpError,
    RfvpRenderer, RfvpResult, TextureDesc, TextureId, TextureRect,
};

const DISPLAY_WIDTH: u32 = gpu::SCREEN_WIDTH;
const DISPLAY_HEIGHT: u32 = gpu::SCREEN_HEIGHT;
const BUFFER_COUNT: u32 = 3;

#[derive(Clone, Copy, Default)]
struct ColorU8 {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl ColorU8 {
    const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    fn from_rfvp(color: ColorRgba) -> Self {
        Self {
            r: float_channel_to_u8(color.r),
            g: float_channel_to_u8(color.g),
            b: float_channel_to_u8(color.b),
            a: float_channel_to_u8(color.a),
        }
    }

    fn to_rgba8(self) -> RGBA8 {
        RGBA8::new_scaled(self.r, self.g, self.b, self.a)
    }

    fn multiply(self, other: ColorU8) -> Self {
        Self {
            r: mul_u8(self.r, other.r),
            g: mul_u8(self.g, other.g),
            b: mul_u8(self.b, other.b),
            a: mul_u8(self.a, other.a),
        }
    }
}

struct TextureEntry {
    id: TextureId,
    desc: TextureDesc,
    pixels: Vec<u8>,
}

pub struct HorizonRenderer {
    frame_width: u32,
    frame_height: u32,
    frame_index: u64,
    framebuffer: Vec<ColorU8>,
    display_pixels: Vec<RGBA8>,
    textures: Vec<TextureEntry>,
    canvas: Option<CanvasManager<RGBA8>>,
}

impl HorizonRenderer {
    pub fn new() -> Self {
        Self {
            frame_width: 0,
            frame_height: 0,
            frame_index: 0,
            framebuffer: Vec::new(),
            display_pixels: Vec::new(),
            textures: Vec::new(),
            canvas: None,
        }
    }

    pub const fn frame_width(&self) -> u32 {
        self.frame_width
    }

    pub const fn frame_height(&self) -> u32 {
        self.frame_height
    }

    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    fn ensure_canvas(&mut self) -> RfvpResult<()> {
        if self.canvas.is_some() {
            return Ok(());
        }
        let gpu_ctx = gpu::Context::new(
            gpu::NvDrvServiceKind::Applet,
            gpu::ViServiceKind::System,
            0x800000,
        )
        .map_err(|_| RfvpError::Backend)?;
        let canvas = CanvasManager::new_stray(
            Arc::new(RwLock::new(gpu_ctx)),
            Default::default(),
            BUFFER_COUNT,
            gpu::BlockLinearHeights::FourGobs,
        )
        .map_err(|_| RfvpError::Backend)?;
        self.canvas = Some(canvas);
        Ok(())
    }

    fn texture_index(&self, id: TextureId) -> Option<usize> {
        self.textures.iter().position(|entry| entry.id == id)
    }

    fn texture(&self, id: TextureId) -> Option<&TextureEntry> {
        self.textures.iter().find(|entry| entry.id == id)
    }

    fn framebuffer_len(width: u32, height: u32) -> RfvpResult<usize> {
        width
            .checked_mul(height)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(RfvpError::CapacityExceeded)
    }

    fn clear_framebuffer(&mut self, clear: Option<ColorRgba>) {
        let clear = clear.map(ColorU8::from_rfvp).unwrap_or(ColorU8::BLACK);
        for pixel in &mut self.framebuffer {
            *pixel = clear;
        }
    }

    fn put_pixel(&mut self, x: i32, y: i32, color: ColorU8, blend: BlendMode) {
        if x < 0 || y < 0 || x >= self.frame_width as i32 || y >= self.frame_height as i32 {
            return;
        }
        let idx = y as usize * self.frame_width as usize + x as usize;
        let dst = self.framebuffer[idx];
        self.framebuffer[idx] = blend_pixel(dst, color, blend);
    }

    fn scissor_contains(scissor: Option<RectI32>, x: i32, y: i32) -> bool {
        let Some(rect) = scissor else {
            return true;
        };
        x >= rect.x
            && y >= rect.y
            && x < rect.x.saturating_add(rect.width)
            && y < rect.y.saturating_add(rect.height)
    }

    fn render_triangle(
        &mut self,
        texture: &TextureEntry,
        command: &DrawSpriteCommand,
        indices: [usize; 3],
    ) {
        let v0 = command.vertices[indices[0]];
        let v1 = command.vertices[indices[1]];
        let v2 = command.vertices[indices[2]];

        let min_x = f32_floor_to_i32(f32_min3(v0.position[0], v1.position[0], v2.position[0]));
        let max_x = f32_ceil_to_i32(f32_max3(v0.position[0], v1.position[0], v2.position[0]));
        let min_y = f32_floor_to_i32(f32_min3(v0.position[1], v1.position[1], v2.position[1]));
        let max_y = f32_ceil_to_i32(f32_max3(v0.position[1], v1.position[1], v2.position[1]));

        let denom = edge_function(v0.position, v1.position, v2.position);
        if denom == 0.0 {
            return;
        }

        let x0 = min_x.max(0);
        let y0 = min_y.max(0);
        let x1 = max_x.min(self.frame_width as i32 - 1);
        let y1 = max_y.min(self.frame_height as i32 - 1);

        for y in y0..=y1 {
            for x in x0..=x1 {
                if !Self::scissor_contains(command.scissor, x, y) {
                    continue;
                }
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let w0 = edge_function(v1.position, v2.position, p) / denom;
                let w1 = edge_function(v2.position, v0.position, p) / denom;
                let w2 = edge_function(v0.position, v1.position, p) / denom;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }

                let u = v0.tex_coord[0] * w0 + v1.tex_coord[0] * w1 + v2.tex_coord[0] * w2;
                let v = v0.tex_coord[1] * w0 + v1.tex_coord[1] * w1 + v2.tex_coord[1] * w2;
                let vertex_color = interpolate_color(v0.color, v1.color, v2.color, w0, w1, w2);
                let texel = sample_texture(texture, u, v).multiply(vertex_color);
                self.put_pixel(x, y, texel, command.blend);
            }
        }
    }

    fn prepare_display_pixels(&mut self) -> RfvpResult<&[RGBA8]> {
        let display_len = Self::framebuffer_len(DISPLAY_WIDTH, DISPLAY_HEIGHT)?;
        if self.display_pixels.len() != display_len {
            self.display_pixels
                .resize(display_len, RGBA8::new_scaled(0, 0, 0, 255));
        }
        if self.frame_width == DISPLAY_WIDTH && self.frame_height == DISPLAY_HEIGHT {
            for (dst, src) in self
                .display_pixels
                .iter_mut()
                .zip(self.framebuffer.iter().copied())
            {
                *dst = src.to_rgba8();
            }
            return Ok(&self.display_pixels);
        }
        if self.frame_width == 0 || self.frame_height == 0 || self.framebuffer.is_empty() {
            for pixel in &mut self.display_pixels {
                *pixel = ColorU8::BLACK.to_rgba8();
            }
            return Ok(&self.display_pixels);
        }

        for y in 0..DISPLAY_HEIGHT as usize {
            let src_y = y * self.frame_height as usize / DISPLAY_HEIGHT as usize;
            for x in 0..DISPLAY_WIDTH as usize {
                let src_x = x * self.frame_width as usize / DISPLAY_WIDTH as usize;
                let src_idx = src_y * self.frame_width as usize + src_x;
                self.display_pixels[y * DISPLAY_WIDTH as usize + x] =
                    self.framebuffer[src_idx].to_rgba8();
            }
        }
        Ok(&self.display_pixels)
    }
}

impl Default for HorizonRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpRenderer for HorizonRenderer {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        if desc.width == 0 || desc.height == 0 || desc.mip_count == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        let size = texture_byte_len(desc)?;
        let mut storage = vec![0; size];
        if let Some(src) = pixels {
            if src.len() < size {
                return Err(RfvpError::InvalidArgument);
            }
            storage.copy_from_slice(&src[..size]);
        }
        if let Some(index) = self.texture_index(id) {
            self.textures[index] = TextureEntry {
                id,
                desc,
                pixels: storage,
            };
        } else {
            self.textures.push(TextureEntry {
                id,
                desc,
                pixels: storage,
            });
        }
        Ok(())
    }

    fn update_texture(
        &mut self,
        id: TextureId,
        rect: TextureRect,
        pixels: &[u8],
    ) -> RfvpResult<()> {
        let Some(index) = self.texture_index(id) else {
            return Err(RfvpError::NotFound);
        };
        let desc = self.textures[index].desc;
        if rect
            .x
            .checked_add(rect.width)
            .ok_or(RfvpError::InvalidArgument)?
            > desc.width
            || rect
                .y
                .checked_add(rect.height)
                .ok_or(RfvpError::InvalidArgument)?
                > desc.height
        {
            return Err(RfvpError::InvalidArgument);
        }
        let bpp = bytes_per_pixel(desc.format);
        let row_bytes = rect
            .width
            .checked_mul(bpp as u32)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(RfvpError::CapacityExceeded)?;
        let required = row_bytes
            .checked_mul(rect.height as usize)
            .ok_or(RfvpError::CapacityExceeded)?;
        if pixels.len() < required {
            return Err(RfvpError::InvalidArgument);
        }
        let tex_width = desc.width as usize;
        let tex_pixels = &mut self.textures[index].pixels;
        for row in 0..rect.height as usize {
            let dst_start = ((rect.y as usize + row) * tex_width + rect.x as usize) * bpp;
            let src_start = row * row_bytes;
            tex_pixels[dst_start..dst_start + row_bytes]
                .copy_from_slice(&pixels[src_start..src_start + row_bytes]);
        }
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(index) = self.texture_index(id) {
            self.textures.swap_remove(index);
        }
    }

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        if width == 0 || height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        let len = Self::framebuffer_len(width, height)?;
        if self.framebuffer.len() != len {
            self.framebuffer.resize(len, ColorU8::BLACK);
        }
        self.frame_width = width;
        self.frame_height = height;
        self.clear_framebuffer(clear);
        Ok(())
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        let Some(texture) = self.texture(command.texture) else {
            return Err(RfvpError::NotFound);
        };
        let local_texture = TextureEntry {
            id: texture.id,
            desc: texture.desc,
            pixels: texture.pixels.clone(),
        };
        self.render_triangle(&local_texture, command, [0, 1, 2]);
        self.render_triangle(&local_texture, command, [0, 2, 3]);
        Ok(())
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        let color = ColorU8::from_rfvp(command.color);
        let x0 = command.rect.x.max(0);
        let y0 = command.rect.y.max(0);
        let x1 = command
            .rect
            .x
            .saturating_add(command.rect.width)
            .min(self.frame_width as i32);
        let y1 = command
            .rect
            .y
            .saturating_add(command.rect.height)
            .min(self.frame_height as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                if Self::scissor_contains(command.scissor, x, y) {
                    self.put_pixel(x, y, color, command.blend);
                }
            }
        }
        Ok(())
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }

    fn present(&mut self) -> RfvpResult<()> {
        self.ensure_canvas()?;
        let pixels = self.prepare_display_pixels()?.to_vec();
        let canvas = self.canvas.as_mut().ok_or(RfvpError::Backend)?;
        canvas
            .render_prepared_buffer(&pixels)
            .map_err(|_| RfvpError::Backend)?;
        canvas
            .wait_vsync_event(None)
            .map_err(|_| RfvpError::Backend)
    }
}

fn texture_byte_len(desc: TextureDesc) -> RfvpResult<usize> {
    let bpp = bytes_per_pixel(desc.format);
    desc.width
        .checked_mul(desc.height)
        .and_then(|v| v.checked_mul(bpp as u32))
        .and_then(|v| usize::try_from(v).ok())
        .ok_or(RfvpError::CapacityExceeded)
}

fn bytes_per_pixel(format: PixelFormat) -> usize {
    match format {
        PixelFormat::Rgba8 | PixelFormat::Bgra8 => 4,
        PixelFormat::Rgb8 => 3,
        PixelFormat::LumaA8 => 2,
        PixelFormat::Luma8 | PixelFormat::Alpha8 => 1,
    }
}

fn sample_texture(texture: &TextureEntry, u: f32, v: f32) -> ColorU8 {
    let x = f32_round_to_u32(u.clamp(0.0, 1.0) * (texture.desc.width.saturating_sub(1)) as f32)
        .min(texture.desc.width.saturating_sub(1));
    let y = f32_round_to_u32(v.clamp(0.0, 1.0) * (texture.desc.height.saturating_sub(1)) as f32)
        .min(texture.desc.height.saturating_sub(1));
    let index = y as usize * texture.desc.width as usize + x as usize;
    match texture.desc.format {
        PixelFormat::Rgba8 => {
            let base = index * 4;
            ColorU8 {
                r: texture.pixels[base],
                g: texture.pixels[base + 1],
                b: texture.pixels[base + 2],
                a: texture.pixels[base + 3],
            }
        }
        PixelFormat::Bgra8 => {
            let base = index * 4;
            ColorU8 {
                r: texture.pixels[base + 2],
                g: texture.pixels[base + 1],
                b: texture.pixels[base],
                a: texture.pixels[base + 3],
            }
        }
        PixelFormat::Rgb8 => {
            let base = index * 3;
            ColorU8 {
                r: texture.pixels[base],
                g: texture.pixels[base + 1],
                b: texture.pixels[base + 2],
                a: 255,
            }
        }
        PixelFormat::Luma8 => {
            let l = texture.pixels[index];
            ColorU8 {
                r: l,
                g: l,
                b: l,
                a: 255,
            }
        }
        PixelFormat::LumaA8 => {
            let base = index * 2;
            let l = texture.pixels[base];
            ColorU8 {
                r: l,
                g: l,
                b: l,
                a: texture.pixels[base + 1],
            }
        }
        PixelFormat::Alpha8 => ColorU8 {
            r: 255,
            g: 255,
            b: 255,
            a: texture.pixels[index],
        },
    }
}

fn blend_pixel(dst: ColorU8, src: ColorU8, mode: BlendMode) -> ColorU8 {
    match mode {
        BlendMode::Opaque => ColorU8 { a: 255, ..src },
        BlendMode::Alpha => alpha_blend(dst, src),
        BlendMode::Add => ColorU8 {
            r: dst.r.saturating_add(src.r),
            g: dst.g.saturating_add(src.g),
            b: dst.b.saturating_add(src.b),
            a: dst.a.max(src.a),
        },
        BlendMode::Multiply => ColorU8 {
            r: mul_u8(dst.r, src.r),
            g: mul_u8(dst.g, src.g),
            b: mul_u8(dst.b, src.b),
            a: dst.a.max(src.a),
        },
        BlendMode::Screen => ColorU8 {
            r: 255 - mul_u8(255 - dst.r, 255 - src.r),
            g: 255 - mul_u8(255 - dst.g, 255 - src.g),
            b: 255 - mul_u8(255 - dst.b, 255 - src.b),
            a: dst.a.max(src.a),
        },
    }
}

fn alpha_blend(dst: ColorU8, src: ColorU8) -> ColorU8 {
    let inv = 255u16.saturating_sub(src.a as u16);
    ColorU8 {
        r: ((src.r as u16 * src.a as u16 + dst.r as u16 * inv) / 255) as u8,
        g: ((src.g as u16 * src.a as u16 + dst.g as u16 * inv) / 255) as u8,
        b: ((src.b as u16 * src.a as u16 + dst.b as u16 * inv) / 255) as u8,
        a: src.a.saturating_add(((dst.a as u16 * inv) / 255) as u8),
    }
}

fn mul_u8(a: u8, b: u8) -> u8 {
    ((a as u16 * b as u16) / 255) as u8
}

fn float_channel_to_u8(value: f32) -> u8 {
    f32_round_to_u32(value.clamp(0.0, 1.0) * 255.0).min(255) as u8
}

fn f32_floor_to_i32(value: f32) -> i32 {
    let truncated = value as i32;
    if (truncated as f32) > value {
        truncated.saturating_sub(1)
    } else {
        truncated
    }
}

fn f32_ceil_to_i32(value: f32) -> i32 {
    let truncated = value as i32;
    if (truncated as f32) < value {
        truncated.saturating_add(1)
    } else {
        truncated
    }
}

fn f32_round_to_u32(value: f32) -> u32 {
    if value <= 0.0 {
        0
    } else {
        (value + 0.5) as u32
    }
}

fn interpolate_color(
    c0: ColorRgba,
    c1: ColorRgba,
    c2: ColorRgba,
    w0: f32,
    w1: f32,
    w2: f32,
) -> ColorU8 {
    ColorU8::from_rfvp(ColorRgba {
        r: c0.r * w0 + c1.r * w1 + c2.r * w2,
        g: c0.g * w0 + c1.g * w1 + c2.g * w2,
        b: c0.b * w0 + c1.b * w1 + c2.b * w2,
        a: c0.a * w0 + c1.a * w1 + c2.a * w2,
    })
}

fn edge_function(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
}

fn f32_min3(a: f32, b: f32, c: f32) -> f32 {
    a.min(b).min(c)
}

fn f32_max3(a: f32, b: f32, c: f32) -> f32 {
    a.max(b).max(c)
}
