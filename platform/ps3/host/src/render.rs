use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::ffi::c_void;

use rfvp::host_api::soft_raster::downscale_rgba8;
use rfvp::host_api::{
    BlendMode, ColorRgba, DrawSolidCommand, DrawSpriteCommand, PixelFormat, RectI32, RfvpError,
    RfvpRenderer, RfvpResult, TextureDesc, TextureId, TextureRect,
};

extern "C" {
    // main.c
    fn rfvp_ps3_gpu_screen_size(width: *mut u32, height: *mut u32);
    fn rfvp_ps3_gpu_create_texture(width: u32, height: u32, rgba: *const u8) -> *mut c_void;
    fn rfvp_ps3_gpu_update_texture(texture: *mut c_void, x: u32, y: u32, w: u32, h: u32, rgba: *const u8);
    fn rfvp_ps3_gpu_destroy_texture(texture: *mut c_void);
    fn rfvp_ps3_gpu_begin_frame();
    fn rfvp_ps3_gpu_set_clip(x0: i32, y0: i32, x1: i32, y1: i32);
    fn rfvp_ps3_gpu_draw_sprite(texture: *mut c_void, corners: *const f32, rgba: u32, additive: i32);
    fn rfvp_ps3_gpu_draw_rect(x0: i32, y0: i32, x1: i32, y1: i32, rgba: u32, additive: i32);
    fn rfvp_ps3_gpu_end_frame();
}

struct GpuTexture {
    handle: *mut c_void,
    /// Size of the texture as the engine made it.
    full_width: u32,
    full_height: u32,
    /// Size kept on the GPU: the same, unless a side is over the RSX's limit.
    width: u32,
    height: u32,
}

/// The RSX's largest texture side.
const GPU_MAX_TEXTURE: u32 = 4096;

/// Draws the game with the RSX (see main.c), at the game's full resolution: the PS3 has the
/// memory for its textures as they are.
pub struct Ps3Renderer {
    textures: BTreeMap<u32, GpuTexture>,
    screen: (u32, u32),
    virtual_size: (u32, u32),
    scale_x: f32,
    scale_y: f32,
    off_x: f32,
    off_y: f32,
}

impl Ps3Renderer {
    pub fn new() -> Self {
        let (mut width, mut height) = (1280, 720);
        unsafe { rfvp_ps3_gpu_screen_size(&mut width, &mut height) };
        Self {
            textures: BTreeMap::new(),
            screen: (width, height),
            virtual_size: (0, 0),
            scale_x: 1.0,
            scale_y: 1.0,
            off_x: 0.0,
            off_y: 0.0,
        }
    }

    /// Map a point on the screen to the game's virtual coordinates.
    pub fn screen_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.off_x) / self.scale_x, (y - self.off_y) / self.scale_y)
    }

    /// The screen size: pointer positions arrive in these pixels.
    pub const fn screen_size(&self) -> (u32, u32) {
        self.screen
    }

    fn set_virtual_size(&mut self, width: u32, height: u32) {
        self.virtual_size = (width, height);
        let (sw, sh) = (self.screen.0 as f32, self.screen.1 as f32);
        // Square pixels: scale to fit, letterboxed.
        let scale = (sw / width as f32).min(sh / height as f32);
        self.scale_x = scale;
        self.scale_y = scale;
        self.off_x = (sw - width as f32 * scale) * 0.5;
        self.off_y = (sh - height as f32 * scale) * 0.5;
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
        let map = |v: i32, scale: f32, off: f32| round(v as f32 * scale + off) as i32;
        (
            map(r.x, self.scale_x, self.off_x).max(0),
            map(r.y, self.scale_y, self.off_y).max(0),
            map(r.x + r.width, self.scale_x, self.off_x).min(self.screen.0 as i32),
            map(r.y + r.height, self.scale_y, self.off_y).min(self.screen.1 as i32),
        )
    }

    fn clip(&self, scissor: Option<RectI32>) {
        let (x0, y0, x1, y1) = self.screen_rect(scissor);
        unsafe { rfvp_ps3_gpu_set_clip(x0, y0, x1, y1) };
    }

    /// Size a texture of `width` x `height` is kept at: as it is, unless a side is over the
    /// RSX's limit.
    fn kept_size(&self, width: u32, height: u32) -> (u32, u32) {
        let longest = width.max(height);
        if longest <= GPU_MAX_TEXTURE {
            return (width, height);
        }
        let scale = GPU_MAX_TEXTURE as f32 / longest as f32;
        (
            (round(width as f32 * scale) as u32).clamp(1, GPU_MAX_TEXTURE),
            (round(height as f32 * scale) as u32).clamp(1, GPU_MAX_TEXTURE),
        )
    }
}

impl Default for Ps3Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Ps3Renderer {
    fn drop(&mut self) {
        for (_, texture) in core::mem::take(&mut self.textures) {
            unsafe { rfvp_ps3_gpu_destroy_texture(texture.handle) };
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

/// A color as RGBA8 (bytes r, g, b, a).
fn rgba8(c: ColorRgba) -> u32 {
    to_u8(c.r) | (to_u8(c.g) << 8) | (to_u8(c.b) << 16) | (to_u8(c.a) << 24)
}

impl RfvpRenderer for Ps3Renderer {
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
        let handle = unsafe { rfvp_ps3_gpu_create_texture(width, height, rgba.as_ptr()) };
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
            unsafe { rfvp_ps3_gpu_destroy_texture(old.handle) };
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
                rfvp_ps3_gpu_update_texture(texture.handle, rect.x, rect.y, rect.width, rect.height, src.as_ptr())
            };
            return Ok(());
        }
        // Kept smaller: shrink the rectangle the same way.
        let sx = texture.width as f32 / texture.full_width as f32;
        let sy = texture.height as f32 / texture.full_height as f32;
        let x0 = (rect.x as f32 * sx) as u32;
        let y0 = (rect.y as f32 * sy) as u32;
        let x1 = (ceil((rect.x + rect.width) as f32 * sx) as u32).min(texture.width).max(x0 + 1);
        let y1 = (ceil((rect.y + rect.height) as f32 * sy) as u32).min(texture.height).max(y0 + 1);
        let small = downscale_rgba8(&src, rect.width, rect.height, x1 - x0, y1 - y0);
        unsafe { rfvp_ps3_gpu_update_texture(texture.handle, x0, y0, x1 - x0, y1 - y0, small.as_ptr()) };
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(texture) = self.textures.remove(&id.0) {
            unsafe { rfvp_ps3_gpu_destroy_texture(texture.handle) };
        }
    }

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        if width == 0 || height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        if (width, height) != self.virtual_size {
            self.set_virtual_size(width, height);
        }
        unsafe { rfvp_ps3_gpu_begin_frame() };
        // Nothing is drawn over the letterbox bars.
        self.clip(None);
        if let Some(c) = clear {
            let (x0, y0, x1, y1) = self.screen_rect(None);
            let color = rgba8(ColorRgba { a: 1.0, ..c });
            unsafe { rfvp_ps3_gpu_draw_rect(x0, y0, x1, y1, color, 0) };
        }
        Ok(())
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        // A texture that is not there yet is skipped rather than failing the whole frame.
        let Some(texture) = self.textures.get(&command.texture.0) else {
            return Ok(());
        };
        // Screen x, y and texel u, v per corner, in triangle-strip order.
        let mut corners = [0f32; 16];
        for (i, v) in command.vertices.iter().enumerate() {
            corners[i * 4] = v.position[0] * self.scale_x + self.off_x;
            corners[i * 4 + 1] = v.position[1] * self.scale_y + self.off_y;
            corners[i * 4 + 2] = v.tex_coord[0] * texture.width as f32;
            corners[i * 4 + 3] = v.tex_coord[1] * texture.height as f32;
        }
        // Sprites have one color, so the first vertex's is it.
        let tint = rgba8(command.vertices[0].color);
        if command.scissor.is_some() {
            self.clip(command.scissor);
        }
        unsafe {
            rfvp_ps3_gpu_draw_sprite(
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
            rfvp_ps3_gpu_draw_rect(
                x0,
                y0,
                x1,
                y1,
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
        unsafe { rfvp_ps3_gpu_end_frame() };
        Ok(())
    }

    /// main.c shows the frame after `rfvp_ps3_frame` returns.
    fn present(&mut self) -> RfvpResult<()> {
        Ok(())
    }
}
