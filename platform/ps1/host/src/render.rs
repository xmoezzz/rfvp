use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::ffi::c_void;

use rfvp::host_api::{
    BlendMode, ColorRgba, DrawSolidCommand, DrawSpriteCommand, PixelFormat, RectI32, RfvpError, RfvpRenderer,
    RfvpResult, TextureDesc, TextureId, TextureRect,
};

extern "C" {
    // main.c
    fn rfvp_ps1_gpu_screen_size(width: *mut u32, height: *mut u32);
    fn rfvp_ps1_gpu_create_texture(format: i32, width: u32, height: u32, pixels: *const u8) -> *mut c_void;
    fn rfvp_ps1_gpu_update_texture(texture: *mut c_void, x: u32, y: u32, w: u32, h: u32, pixels: *const u16, stride: u32);
    fn rfvp_ps1_gpu_destroy_texture(texture: *mut c_void);
    fn rfvp_ps1_gpu_begin_frame();
    fn rfvp_ps1_gpu_set_clip(x0: i32, y0: i32, x1: i32, y1: i32);
    fn rfvp_ps1_gpu_draw_sprite(texture: *mut c_void, corners: *const f32, rgba: u32, additive: i32);
    fn rfvp_ps1_gpu_draw_rect(x0: i32, y0: i32, x1: i32, y1: i32, rgba: u32, additive: i32);
    fn rfvp_ps1_gpu_end_frame();
}

/// main.c's texture formats.
const FORMAT_DIRECT: i32 = 0;
const FORMAT_CLUT4: i32 = 1;
const FORMAT_CLUT8: i32 = 2;
const FORMAT_MASK8: i32 = 3;

struct GpuTexture {
    handle: *mut c_void,
    width: u32,
    height: u32,
    /// A direct-color texture's pixels, which updates are made from (the GPU takes whole
    /// words: a rectangle of odd width takes a neighbour along).
    shadow: Option<Vec<u16>>,
}

/// Draws with the PlayStation's GPU (main.c): pictures stay in the CLUT formats the converter
/// made; anything the engine draws itself (text, solid fills) becomes 15-bit color.
pub struct Ps1Renderer {
    textures: BTreeMap<u32, GpuTexture>,
    screen: (u32, u32),
    virtual_size: (u32, u32),
    scale: f32,
    off_x: f32,
    off_y: f32,
}

impl Ps1Renderer {
    pub fn new() -> Self {
        let (mut width, mut height) = (320, 240);
        unsafe { rfvp_ps1_gpu_screen_size(&mut width, &mut height) };
        Self {
            textures: BTreeMap::new(),
            screen: (width, height),
            virtual_size: (0, 0),
            scale: 1.0,
            off_x: 0.0,
            off_y: 0.0,
        }
    }

    pub fn screen_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.off_x) / self.scale, (y - self.off_y) / self.scale)
    }

    pub const fn screen_size(&self) -> (u32, u32) {
        self.screen
    }

    fn set_virtual_size(&mut self, width: u32, height: u32) {
        self.virtual_size = (width, height);
        let (sw, sh) = (self.screen.0 as f32, self.screen.1 as f32);
        self.scale = (sw / width as f32).min(sh / height as f32);
        self.off_x = (sw - width as f32 * self.scale) * 0.5;
        self.off_y = (sh - height as f32 * self.scale) * 0.5;
    }

    fn screen_rect(&self, rect: Option<RectI32>) -> (i32, i32, i32, i32) {
        let (vw, vh) = self.virtual_size;
        let r = rect.unwrap_or(RectI32 { x: 0, y: 0, width: vw as i32, height: vh as i32 });
        let map = |v: i32, off: f32| (v as f32 * self.scale + off + 0.5) as i32;
        (
            map(r.x, self.off_x).max(0),
            map(r.y, self.off_y).max(0),
            map(r.x + r.width, self.off_x).min(self.screen.0 as i32),
            map(r.y + r.height, self.off_y).min(self.screen.1 as i32),
        )
    }

    fn clip(&self, scissor: Option<RectI32>) {
        let (x0, y0, x1, y1) = self.screen_rect(scissor);
        unsafe { rfvp_ps1_gpu_set_clip(x0, y0, x1, y1) };
    }
}

impl Drop for Ps1Renderer {
    fn drop(&mut self) {
        for (_, texture) in core::mem::take(&mut self.textures) {
            unsafe { rfvp_ps1_gpu_destroy_texture(texture.handle) };
        }
    }
}

/// A pixel as a 15-bit color: transparent (0) below a quarter alpha, STP (half blended) below
/// three quarters, opaque above (opaque black is the darkest gray that is not transparent).
fn color15(r: u8, g: u8, b: u8, a: u8) -> u16 {
    let c = (r as u16 >> 3) | ((g as u16 >> 3) << 5) | ((b as u16 >> 3) << 10);
    match a {
        0..=63 => 0,
        64..=191 => c | 0x8000,
        _ if c == 0 => 0x0421,
        _ => c,
    }
}

/// Pixels of `format` as 15-bit colors.
fn to_color15(format: PixelFormat, src: &[u8], count: usize) -> RfvpResult<Vec<u16>> {
    let bpp = match format {
        PixelFormat::Rgba8 | PixelFormat::Bgra8 => 4,
        PixelFormat::Rgb8 => 3,
        PixelFormat::LumaA8 => 2,
        PixelFormat::Luma8 | PixelFormat::Alpha8 => 1,
        _ => return Err(RfvpError::Unsupported),
    };
    if src.len() < count * bpp {
        return Err(RfvpError::InvalidArgument);
    }
    Ok(src[..count * bpp]
        .chunks_exact(bpp)
        .map(|p| match format {
            PixelFormat::Rgba8 => color15(p[0], p[1], p[2], p[3]),
            PixelFormat::Bgra8 => color15(p[2], p[1], p[0], p[3]),
            PixelFormat::Rgb8 => color15(p[0], p[1], p[2], 255),
            PixelFormat::LumaA8 => color15(p[0], p[0], p[0], p[1]),
            PixelFormat::Luma8 => color15(p[0], p[0], p[0], 255),
            _ => color15(255, 255, 255, p[0]),
        })
        .collect())
}

fn to_u8(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32
}

/// A color as RGBA8 (bytes r, g, b, a).
fn rgba8(c: ColorRgba) -> u32 {
    to_u8(c.r) | (to_u8(c.g) << 8) | (to_u8(c.b) << 16) | (to_u8(c.a) << 24)
}

impl RfvpRenderer for Ps1Renderer {
    fn create_texture(&mut self, id: TextureId, desc: TextureDesc, pixels: Option<&[u8]>) -> RfvpResult<()> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        let count = desc.width as usize * desc.height as usize;
        let (format, owned_direct, raw): (i32, Option<Vec<u16>>, Option<&[u8]>) = match desc.format {
            PixelFormat::Clut4 => (FORMAT_CLUT4, None, pixels),
            PixelFormat::Clut8 => (FORMAT_CLUT8, None, pixels),
            PixelFormat::Ps1Mask8 => (FORMAT_MASK8, None, pixels),
            other => {
                let direct = match pixels {
                    Some(src) => to_color15(other, src, count)?,
                    None => alloc::vec![0u16; count],
                };
                (FORMAT_DIRECT, Some(direct), None)
            }
        };
        let ptr = match (&owned_direct, raw) {
            (Some(direct), _) => direct.as_ptr() as *const u8,
            (None, Some(raw)) => raw.as_ptr(),
            (None, None) => return Err(RfvpError::InvalidArgument),
        };
        let handle = unsafe { rfvp_ps1_gpu_create_texture(format, desc.width, desc.height, ptr) };
        if handle.is_null() {
            return Err(RfvpError::OutOfMemory);
        }
        let texture = GpuTexture { handle, width: desc.width, height: desc.height, shadow: owned_direct };
        if let Some(old) = self.textures.insert(id.0, texture) {
            unsafe { rfvp_ps1_gpu_destroy_texture(old.handle) };
        }
        Ok(())
    }

    fn update_texture(&mut self, id: TextureId, rect: TextureRect, pixels: &[u8]) -> RfvpResult<()> {
        let texture = self.textures.get_mut(&id.0).ok_or(RfvpError::NotFound)?;
        if rect.width == 0 || rect.height == 0 {
            return Ok(());
        }
        let (width, height) = (texture.width, texture.height);
        let Some(shadow) = texture.shadow.as_mut() else {
            return Err(RfvpError::InvalidArgument);
        };
        if rect.x + rect.width > width || rect.y + rect.height > height {
            return Err(RfvpError::InvalidArgument);
        }
        // The engine's updates are RGBA8 (text).
        let colors = to_color15(PixelFormat::Rgba8, pixels, rect.width as usize * rect.height as usize)?;
        for row in 0..rect.height as usize {
            let dst = (rect.y as usize + row) * width as usize + rect.x as usize;
            shadow[dst..dst + rect.width as usize]
                .copy_from_slice(&colors[row * rect.width as usize..(row + 1) * rect.width as usize]);
        }
        unsafe {
            rfvp_ps1_gpu_update_texture(texture.handle, rect.x, rect.y, rect.width, rect.height, shadow.as_ptr(), width)
        };
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(texture) = self.textures.remove(&id.0) {
            unsafe { rfvp_ps1_gpu_destroy_texture(texture.handle) };
        }
    }

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        if width == 0 || height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        if (width, height) != self.virtual_size {
            self.set_virtual_size(width, height);
        }
        unsafe { rfvp_ps1_gpu_begin_frame() };
        self.clip(None);
        if let Some(c) = clear {
            let (x0, y0, x1, y1) = self.screen_rect(None);
            unsafe { rfvp_ps1_gpu_draw_rect(x0, y0, x1, y1, rgba8(ColorRgba { a: 1.0, ..c }), 0) };
        }
        Ok(())
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        let Some(texture) = self.textures.get(&command.texture.0) else {
            return Ok(());
        };
        let mut corners = [0f32; 16];
        for (i, v) in command.vertices.iter().enumerate() {
            corners[i * 4] = v.position[0] * self.scale + self.off_x;
            corners[i * 4 + 1] = v.position[1] * self.scale + self.off_y;
            corners[i * 4 + 2] = v.tex_coord[0] * texture.width as f32;
            corners[i * 4 + 3] = v.tex_coord[1] * texture.height as f32;
        }
        if command.scissor.is_some() {
            self.clip(command.scissor);
        }
        let additive = (command.blend == BlendMode::Add) as i32;
        unsafe { rfvp_ps1_gpu_draw_sprite(texture.handle, corners.as_ptr(), rgba8(command.vertices[0].color), additive) };
        if command.scissor.is_some() {
            self.clip(None);
        }
        Ok(())
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        let (x0, y0, x1, y1) = self.screen_rect(Some(command.rect));
        if x1 <= x0 || y1 <= y0 {
            return Ok(());
        }
        if command.scissor.is_some() {
            self.clip(command.scissor);
        }
        let additive = (command.blend == BlendMode::Add) as i32;
        unsafe { rfvp_ps1_gpu_draw_rect(x0, y0, x1, y1, rgba8(command.color), additive) };
        if command.scissor.is_some() {
            self.clip(None);
        }
        Ok(())
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        unsafe { rfvp_ps1_gpu_end_frame() };
        Ok(())
    }

    /// main.c shows the frame after `rfvp_ps1_frame` returns.
    fn present(&mut self) -> RfvpResult<()> {
        Ok(())
    }
}
