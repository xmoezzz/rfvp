use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::ffi::c_void;

use rfvp::host_api::{
    BlendMode, ColorRgba, DrawSolidCommand, DrawSpriteCommand, PixelFormat, RectI32, RfvpError,
    RfvpRenderer, RfvpResult, TextureDesc, TextureId, TextureRect,
};

extern "C" {
    // main.c
    fn rfvp_wiiu_gpu_screen_size(width: *mut u32, height: *mut u32);
    fn rfvp_wiiu_gpu_create_texture(width: u32, height: u32, rgba: *const u8) -> *mut c_void;
    fn rfvp_wiiu_gpu_update_texture(texture: *mut c_void, x: u32, y: u32, w: u32, h: u32, rgba: *const u8);
    fn rfvp_wiiu_gpu_destroy_texture(texture: *mut c_void);
    fn rfvp_wiiu_gpu_begin_frame(width: f32, height: f32);
    fn rfvp_wiiu_gpu_draw(texture: *mut c_void, corners: *const f32, rgba: u32, additive: i32, clip: *const f32);
    fn rfvp_wiiu_gpu_end_frame();
}

struct GpuTexture {
    handle: *mut c_void,
    width: u32,
    height: u32,
}

/// The GPU's largest texture side.
const GPU_MAX_TEXTURE: u32 = 8192;

/// Draws the game with the Wii U's GPU (GX2, see main.c). The draws are given in the game's
/// virtual coordinates; main.c plays them on the TV and on the GamePad, letterboxed. Textures
/// stay at the game's size.
pub struct WiiuRenderer {
    textures: BTreeMap<u32, GpuTexture>,
    screen: (u32, u32),
    virtual_size: (u32, u32),
}

impl WiiuRenderer {
    pub fn new() -> Self {
        let (mut width, mut height) = (1280, 720);
        unsafe { rfvp_wiiu_gpu_screen_size(&mut width, &mut height) };
        Self {
            textures: BTreeMap::new(),
            screen: (width, height),
            virtual_size: (0, 0),
        }
    }

    /// Map a point on the TV picture (1280x720) to the game's virtual coordinates.
    pub fn screen_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        let (vw, vh) = (self.virtual_size.0.max(1) as f32, self.virtual_size.1.max(1) as f32);
        let (sw, sh) = (self.screen.0 as f32, self.screen.1 as f32);
        let scale = (sw / vw).min(sh / vh);
        let (ox, oy) = ((sw - vw * scale) * 0.5, (sh - vh * scale) * 0.5);
        ((x - ox) / scale, (y - oy) / scale)
    }

    fn clip_rect(rect: Option<RectI32>) -> Option<[f32; 4]> {
        rect.map(|r| {
            [
                r.x as f32,
                r.y as f32,
                (r.x + r.width) as f32,
                (r.y + r.height) as f32,
            ]
        })
    }
}

impl Default for WiiuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WiiuRenderer {
    fn drop(&mut self) {
        for (_, texture) in core::mem::take(&mut self.textures) {
            unsafe { rfvp_wiiu_gpu_destroy_texture(texture.handle) };
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
        // Only the PlayStation's builds (feature `psx`) have indexed formats.
        #[allow(unreachable_patterns)]
        _ => return Err(RfvpError::Unsupported),
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
            _ => [255, 255, 255, p[0]],
        });
    }
    Ok(out)
}

fn to_u8(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32
}

/// A color as RGBA8 (bytes r, g, b, a from the low byte up).
fn rgba8(c: ColorRgba) -> u32 {
    to_u8(c.r) | (to_u8(c.g) << 8) | (to_u8(c.b) << 16) | (to_u8(c.a) << 24)
}

impl RfvpRenderer for WiiuRenderer {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        if desc.width > GPU_MAX_TEXTURE || desc.height > GPU_MAX_TEXTURE {
            return Err(RfvpError::CapacityExceeded);
        }
        let count = desc.width as usize * desc.height as usize;
        let rgba = match pixels {
            Some(src) => Some(to_rgba8(desc.format, src, count)?),
            None => None,
        };
        let ptr = rgba.as_ref().map_or(core::ptr::null(), |p| p.as_ptr());
        let handle = unsafe { rfvp_wiiu_gpu_create_texture(desc.width, desc.height, ptr) };
        if handle.is_null() {
            return Err(RfvpError::OutOfMemory);
        }
        let texture = GpuTexture { handle, width: desc.width, height: desc.height };
        if let Some(old) = self.textures.insert(id.0, texture) {
            unsafe { rfvp_wiiu_gpu_destroy_texture(old.handle) };
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
        if rect.x + rect.width > texture.width || rect.y + rect.height > texture.height {
            return Err(RfvpError::InvalidArgument);
        }
        // The engine's pixels are RGBA8 here (text and movie frames).
        let count = rect.width as usize * rect.height as usize;
        if pixels.len() < count * 4 {
            return Err(RfvpError::InvalidArgument);
        }
        unsafe {
            rfvp_wiiu_gpu_update_texture(texture.handle, rect.x, rect.y, rect.width, rect.height, pixels.as_ptr())
        };
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(texture) = self.textures.remove(&id.0) {
            unsafe { rfvp_wiiu_gpu_destroy_texture(texture.handle) };
        }
    }

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        if width == 0 || height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        self.virtual_size = (width, height);
        unsafe { rfvp_wiiu_gpu_begin_frame(width as f32, height as f32) };
        if let Some(c) = clear {
            let (w, h) = (width as f32, height as f32);
            let corners = [0.0, 0.0, 0.0, 0.0, 0.0, h, 0.0, 1.0, w, 0.0, 1.0, 0.0, w, h, 1.0, 1.0];
            let color = rgba8(ColorRgba { a: 1.0, ..c });
            unsafe { rfvp_wiiu_gpu_draw(core::ptr::null_mut(), corners.as_ptr(), color, 0, core::ptr::null()) };
        }
        Ok(())
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        // A texture that is not there yet is skipped rather than failing the whole frame.
        let Some(texture) = self.textures.get(&command.texture.0) else {
            return Ok(());
        };
        // Virtual x, y and normalized u, v per corner, in triangle-strip order.
        let mut corners = [0f32; 16];
        for (i, v) in command.vertices.iter().enumerate() {
            corners[i * 4] = v.position[0];
            corners[i * 4 + 1] = v.position[1];
            corners[i * 4 + 2] = v.tex_coord[0];
            corners[i * 4 + 3] = v.tex_coord[1];
        }
        // Sprites have one color, so the first vertex's is it.
        let tint = rgba8(command.vertices[0].color);
        let clip = Self::clip_rect(command.scissor);
        unsafe {
            rfvp_wiiu_gpu_draw(
                texture.handle,
                corners.as_ptr(),
                tint,
                (command.blend == BlendMode::Add) as i32,
                clip.as_ref().map_or(core::ptr::null(), |c| c.as_ptr()),
            )
        };
        Ok(())
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        let r = command.rect;
        if r.width <= 0 || r.height <= 0 {
            return Ok(());
        }
        let (x0, y0) = (r.x as f32, r.y as f32);
        let (x1, y1) = ((r.x + r.width) as f32, (r.y + r.height) as f32);
        let corners = [x0, y0, 0.0, 0.0, x0, y1, 0.0, 1.0, x1, y0, 1.0, 0.0, x1, y1, 1.0, 1.0];
        let clip = Self::clip_rect(command.scissor);
        unsafe {
            rfvp_wiiu_gpu_draw(
                core::ptr::null_mut(),
                corners.as_ptr(),
                rgba8(command.color),
                (command.blend == BlendMode::Add) as i32,
                clip.as_ref().map_or(core::ptr::null(), |c| c.as_ptr()),
            )
        };
        Ok(())
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        unsafe { rfvp_wiiu_gpu_end_frame() };
        Ok(())
    }

    /// main.c shows the frame after `rfvp_wiiu_frame` returns.
    fn present(&mut self) -> RfvpResult<()> {
        Ok(())
    }
}
