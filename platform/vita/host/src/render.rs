use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::ffi::c_void;

use rfvp::host_api::soft_raster::downscale_rgba8;
use rfvp::host_api::{
    BlendMode, ColorRgba, DrawSolidCommand, DrawSpriteCommand, PixelFormat, RectI32, RfvpError,
    RfvpRenderer, RfvpResult, TextureDesc, TextureId, TextureRect,
};

/// The Vita screen.
const SCREEN_WIDTH: u32 = 960;
const SCREEN_HEIGHT: u32 = 544;

extern "C" {
    // rfvp_psv_gpu.c
    fn rfvp_psv_gpu_create_texture(width: u32, height: u32, rgba: *const u8, linear: i32) -> *mut c_void;
    fn rfvp_psv_gpu_update_texture(texture: *mut c_void, x: u32, y: u32, w: u32, h: u32, rgba: *const u8);
    fn rfvp_psv_gpu_destroy_texture(texture: *mut c_void);
    fn rfvp_psv_gpu_begin_frame();
    fn rfvp_psv_gpu_set_clip(enabled: i32, x0: i32, y0: i32, x1: i32, y1: i32);
    fn rfvp_psv_gpu_draw_sprite(texture: *mut c_void, corners: *const f32, tint_rgba: u32, additive: i32);
    fn rfvp_psv_gpu_draw_rect(x: f32, y: f32, w: f32, h: f32, rgba: u32, additive: i32);
    fn rfvp_psv_gpu_end_frame();
    fn rfvp_psv_gpu_present();
    fn rfvp_psv_gpu_frame_pixels() -> *const u8;
}

struct GpuTexture {
    handle: *mut c_void,
    /// Size of the texture as the engine made it.
    full_width: u32,
    full_height: u32,
    /// Size kept on the GPU: shrunk to the screen scale.
    width: u32,
    height: u32,
}

/// Draws the game with the Vita's GPU (through vita2d, see rfvp_psv_gpu.c).
///
/// Textures are kept at screen resolution: the game's images and text (often 1920x1080) are
/// shrunk to the 960x544 screen when they are uploaded, which saves most of the texture memory
/// and the GPU's sampling work.
pub struct PsvRenderer {
    textures: BTreeMap<u32, GpuTexture>,
    virtual_size: (u32, u32),
    scale: f32,
    off_x: f32,
    off_y: f32,
    /// The frame drawn last could be read back (`end_frame` ran since `begin_frame`).
    frame_done: bool,
}

impl PsvRenderer {
    pub fn new() -> Self {
        Self {
            textures: BTreeMap::new(),
            virtual_size: (0, 0),
            scale: 1.0,
            off_x: 0.0,
            off_y: 0.0,
            frame_done: false,
        }
    }

    /// Map a point on the screen to the game's virtual coordinates.
    pub fn screen_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.off_x) / self.scale, (y - self.off_y) / self.scale)
    }

    fn set_virtual_size(&mut self, width: u32, height: u32) {
        self.virtual_size = (width, height);
        let sx = SCREEN_WIDTH as f32 / width as f32;
        let sy = SCREEN_HEIGHT as f32 / height as f32;
        self.scale = sx.min(sy);
        self.off_x = (SCREEN_WIDTH as f32 - width as f32 * self.scale) * 0.5;
        self.off_y = (SCREEN_HEIGHT as f32 - height as f32 * self.scale) * 0.5;
    }

    /// The screen rectangle (x0, y0, x1, y1) of a rectangle in virtual coordinates, or of the
    /// whole virtual frame.
    fn screen_rect(&self, rect: Option<RectI32>) -> (i32, i32, i32, i32) {
        let (vw, vh) = self.virtual_size;
        let r = rect.unwrap_or(RectI32 {
            x: 0,
            y: 0,
            width: vw as i32,
            height: vh as i32,
        });
        let map = |v: i32, off: f32| round(v as f32 * self.scale + off) as i32;
        (
            map(r.x, self.off_x).max(0),
            map(r.y, self.off_y).max(0),
            map(r.x + r.width, self.off_x).min(SCREEN_WIDTH as i32),
            map(r.y + r.height, self.off_y).min(SCREEN_HEIGHT as i32),
        )
    }

    fn clip(&self, scissor: Option<RectI32>) {
        let (x0, y0, x1, y1) = self.screen_rect(scissor);
        unsafe { rfvp_psv_gpu_set_clip(1, x0, y0, x1, y1) };
    }

    /// Size a texture of `width` x `height` is kept at.
    fn kept_size(&self, width: u32, height: u32) -> (u32, u32) {
        if self.scale >= 0.999 {
            return (width, height);
        }
        (
            (round(width as f32 * self.scale) as u32).max(1),
            (round(height as f32 * self.scale) as u32).max(1),
        )
    }
}

impl Default for PsvRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PsvRenderer {
    fn drop(&mut self) {
        for (_, texture) in core::mem::take(&mut self.textures) {
            unsafe { rfvp_psv_gpu_destroy_texture(texture.handle) };
        }
    }
}

/// Pixels of `format` as RGBA8 bytes.
fn to_rgba8(format: PixelFormat, src: &[u8], count: usize) -> RfvpResult<Vec<u8>> {
    let bpp = match format {
        PixelFormat::Rgba8 | PixelFormat::Bgra8 => 4,
        PixelFormat::Rgb8 => 3,
        PixelFormat::LumaA8 => 2,
        PixelFormat::Luma8 | PixelFormat::Alpha8 => 1,
    };
    if src.len() < count * bpp {
        return Err(RfvpError::InvalidArgument);
    }
    if format == PixelFormat::Rgba8 {
        return Ok(src[..count * 4].to_vec());
    }
    let mut out = Vec::with_capacity(count * 4);
    for p in src[..count * bpp].chunks_exact(bpp) {
        out.extend_from_slice(&match format {
            PixelFormat::Rgba8 => [p[0], p[1], p[2], p[3]],
            PixelFormat::Bgra8 => [p[2], p[1], p[0], p[3]],
            PixelFormat::Rgb8 => [p[0], p[1], p[2], 255],
            PixelFormat::LumaA8 => [p[0], p[0], p[0], p[1]],
            PixelFormat::Luma8 => [p[0], p[0], p[0], 255],
            PixelFormat::Alpha8 => [255, 255, 255, p[0]],
        });
    }
    Ok(out)
}

/// `f32::round` and `f32::ceil`, which `core` does not have.
fn round(v: f32) -> f32 {
    let t = v as i64 as f32;
    if v - t >= 0.5 {
        t + 1.0
    } else if t - v > 0.5 {
        t - 1.0
    } else {
        t
    }
}

fn ceil(v: f32) -> f32 {
    let t = v as i64 as f32;
    if v > t {
        t + 1.0
    } else {
        t
    }
}

fn to_u8(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32
}

/// A color as vita2d's RGBA8 (bytes r, g, b, a).
fn rgba8(c: ColorRgba) -> u32 {
    to_u8(c.r) | (to_u8(c.g) << 8) | (to_u8(c.b) << 16) | (to_u8(c.a) << 24)
}

impl RfvpRenderer for PsvRenderer {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        let (width, height) = self.kept_size(desc.width, desc.height);
        let count = desc.width as usize * desc.height as usize;
        let rgba = match pixels {
            Some(src) => {
                let full = to_rgba8(desc.format, src, count)?;
                if (width, height) == (desc.width, desc.height) {
                    full
                } else {
                    downscale_rgba8(&full, desc.width, desc.height, width, height)
                }
            }
            None => alloc::vec![0u8; width as usize * height as usize * 4],
        };
        let handle = unsafe { rfvp_psv_gpu_create_texture(width, height, rgba.as_ptr(), 1) };
        if handle.is_null() {
            return Err(RfvpError::OutOfMemory);
        }
        if let Some(old) = self.textures.insert(
            id.0,
            GpuTexture {
                handle,
                full_width: desc.width,
                full_height: desc.height,
                width,
                height,
            },
        ) {
            unsafe { rfvp_psv_gpu_destroy_texture(old.handle) };
        }
        Ok(())
    }

    fn update_texture(
        &mut self,
        id: TextureId,
        rect: TextureRect,
        pixels: &[u8],
    ) -> RfvpResult<()> {
        let texture = self.textures.get(&id.0).ok_or(RfvpError::NotFound)?;
        if rect.width == 0 || rect.height == 0 {
            return Ok(());
        }
        if rect.x + rect.width > texture.full_width || rect.y + rect.height > texture.full_height {
            return Err(RfvpError::InvalidArgument);
        }
        // The engine's pixels are RGBA8 here (text and movie frames).
        let count = rect.width as usize * rect.height as usize;
        let src = to_rgba8(PixelFormat::Rgba8, pixels, count)?;
        if texture.width == texture.full_width && texture.height == texture.full_height {
            unsafe {
                rfvp_psv_gpu_update_texture(texture.handle, rect.x, rect.y, rect.width, rect.height, src.as_ptr())
            };
            return Ok(());
        }
        // Kept at screen resolution: shrink the rectangle the same way.
        let sx = texture.width as f32 / texture.full_width as f32;
        let sy = texture.height as f32 / texture.full_height as f32;
        let x0 = (rect.x as f32 * sx) as u32;
        let y0 = (rect.y as f32 * sy) as u32;
        let x1 = (ceil((rect.x + rect.width) as f32 * sx) as u32).min(texture.width).max(x0 + 1);
        let y1 = (ceil((rect.y + rect.height) as f32 * sy) as u32).min(texture.height).max(y0 + 1);
        let small = downscale_rgba8(&src, rect.width, rect.height, x1 - x0, y1 - y0);
        unsafe { rfvp_psv_gpu_update_texture(texture.handle, x0, y0, x1 - x0, y1 - y0, small.as_ptr()) };
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(texture) = self.textures.remove(&id.0) {
            unsafe { rfvp_psv_gpu_destroy_texture(texture.handle) };
        }
    }

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        if width == 0 || height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        if (width, height) != self.virtual_size {
            self.set_virtual_size(width, height);
        }
        self.frame_done = false;
        unsafe { rfvp_psv_gpu_begin_frame() };
        if let Some(c) = clear {
            let (x0, y0, x1, y1) = self.screen_rect(None);
            let color = rgba8(ColorRgba { a: 1.0, ..c });
            unsafe {
                rfvp_psv_gpu_draw_rect(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32, color, 0)
            };
        }
        // Nothing is drawn over the letterbox bars.
        self.clip(None);
        Ok(())
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        // A texture that is not there yet is skipped rather than failing the whole frame.
        let Some(texture) = self.textures.get(&command.texture.0) else {
            return Ok(());
        };
        let mut corners = [0f32; 16];
        for (i, v) in command.vertices.iter().enumerate() {
            corners[i * 4] = v.position[0] * self.scale + self.off_x;
            corners[i * 4 + 1] = v.position[1] * self.scale + self.off_y;
            corners[i * 4 + 2] = v.tex_coord[0];
            corners[i * 4 + 3] = v.tex_coord[1];
        }
        // vita2d takes one tint per draw; sprites have one color, so the first vertex's is it.
        let tint = rgba8(command.vertices[0].color);
        if command.scissor.is_some() {
            self.clip(command.scissor);
        }
        unsafe {
            rfvp_psv_gpu_draw_sprite(
                texture.handle,
                corners.as_ptr(),
                tint,
                (command.blend == BlendMode::Add) as i32,
            )
        };
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
        unsafe {
            rfvp_psv_gpu_draw_rect(
                x0 as f32,
                y0 as f32,
                (x1 - x0) as f32,
                (y1 - y0) as f32,
                rgba8(command.color),
                (command.blend == BlendMode::Add) as i32,
            )
        };
        if command.scissor.is_some() {
            self.clip(None);
        }
        Ok(())
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        unsafe { rfvp_psv_gpu_end_frame() };
        self.frame_done = true;
        Ok(())
    }

    fn present(&mut self) -> RfvpResult<()> {
        unsafe { rfvp_psv_gpu_present() };
        Ok(())
    }

    /// The game frame read back from the display buffer just drawn. (Vita3K keeps GPU surfaces
    /// on the host, so there it reads back black.)
    fn capture_thumbnail(&mut self, width: u32, height: u32) -> Option<Vec<u8>> {
        if !self.frame_done || width == 0 || height == 0 {
            return None;
        }
        let fb = unsafe { rfvp_psv_gpu_frame_pixels() };
        if fb.is_null() {
            return None;
        }
        let (x0, y0, x1, y1) = self.screen_rect(None);
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut view = Vec::with_capacity(w * h * 4);
        for y in y0 as usize..y1 as usize {
            let row = unsafe {
                core::slice::from_raw_parts(fb.add((y * SCREEN_WIDTH as usize + x0 as usize) * 4), w * 4)
            };
            view.extend_from_slice(row);
        }
        Some(downscale_rgba8(&view, w as u32, h as u32, width, height))
    }
}
