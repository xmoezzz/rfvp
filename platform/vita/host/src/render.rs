use alloc::collections::BTreeMap;
use alloc::borrow::Cow;
use alloc::vec::Vec;
use core::ffi::c_void;

use crate::downscale::downscale_rgba8;
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
    fn rfvp_psv_gpu_update_texture_strided(texture: *mut c_void, x: u32, y: u32,
        w: u32, h: u32, rgba: *const u8, source_stride: u32) -> i32;
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
    format: PixelFormat,
    resample_opaque: Option<bool>,
}

/// Draws the game with the Vita's GPU (through vita2d, see rfvp_psv_gpu.c).
///
/// Large reductions use screen-sized textures. Small reductions use native pixels and GPU
/// filtering, avoiding expensive CPU resizes when expressions or text change.
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
fn to_rgba8(format: PixelFormat, src: &[u8], count: usize) -> RfvpResult<Cow<'_, [u8]>> {
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
        return Ok(Cow::Borrowed(&src[..count * 4]));
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
    Ok(Cow::Owned(out))
}

/// `f32::round` and `f32::ceil`, which `core` does not have.
/// A resize/upload worth one bounded diagnostic line. Native textures are not logged unless
/// their upload is this slow; a scaled texture is logged regardless so the policy path is visible.
const SLOW_TEXTURE_US: u64 = 10_000;

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

/// (bytes per pixel, alpha byte index) for source formats that have an alpha channel.
fn alpha_layout(format: PixelFormat) -> Option<(usize, usize)> {
    match format {
        PixelFormat::Rgba8 | PixelFormat::Bgra8 => Some((4, 3)),
        PixelFormat::LumaA8 => Some((2, 1)),
        PixelFormat::Alpha8 => Some((1, 0)),
        _ => None,
    }
}

impl RfvpRenderer for PsvRenderer {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        let _timing = rfvp::perf::scope(rfvp::perf::PSV_TEXTURE);
        if desc.width == 0 || desc.height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        let count = (desc.width as usize)
            .checked_mul(desc.height as usize)
            .ok_or(RfvpError::InvalidArgument)?;
        count.checked_mul(4).ok_or(RfvpError::InvalidArgument)?;
        // Only near the old 2 MiB cap: a transparent character sprite may keep native pixels
        // (the device's 1362x1813 sprite is 2.47M pixels). Opaque backgrounds stay scaled.
        let near_cap = self.scale >= 0.75
            && desc.width <= 2048
            && desc.height <= 2048
            && (count as u64) > crate::texture_policy::NATIVE_PIXEL_LIMIT
            && (count as u64) <= crate::texture_policy::NEAR_CAP_PIXEL_LIMIT;
        let transparent = if near_cap {
            pixels
                .and_then(|src| alpha_layout(desc.format).map(|(bpp, alpha)| {
                    crate::texture_policy::has_transparent_alpha(src, count, bpp, alpha)
                }))
                .unwrap_or(false)
        } else {
            false
        };
        let ((mut width, mut height), mut reason) =
            crate::texture_policy::policy_with_alpha(self.scale, desc.width, desc.height, transparent);
        let mut resized = (width, height) != (desc.width, desc.height);
        let mut resize_us = 0u64;
        let mut resample_opaque = None;
        let mut rgba = match pixels {
            Some(src) => {
                let full = to_rgba8(desc.format, src, count)?;
                if resized {
                    resample_opaque = Some(full.chunks_exact(4).all(|p| p[3] == 255));
                    let resize_start = rfvp::perf::start();
                    let small = {
                        let _resize = rfvp::perf::scope(rfvp::perf::PSV_RESIZE);
                        crate::downscale::downscale_rgba8_region_with_opacity(&full, desc.width, desc.height,
                            width, height, 0, 0, width, height, resample_opaque.unwrap())
                            .ok_or(RfvpError::InvalidArgument)?
                    };
                    resize_us = rfvp::perf::start().saturating_sub(resize_start);
                    Cow::Owned(small)
                } else {
                    full
                }
            }
            None => Cow::Owned(alloc::vec![0u8; width as usize * height as usize * 4]),
        };
        let upload_start = rfvp::perf::start();
        let mut handle = {
            let _upload = rfvp::perf::scope(rfvp::perf::PSV_GPU_UPLOAD);
            unsafe { rfvp_psv_gpu_create_texture(width, height, rgba.as_ptr(), 1) }
        };
        let mut upload_us = rfvp::perf::start().saturating_sub(upload_start);
        if handle.is_null() && (width, height) == (desc.width, desc.height) {
            let fallback = crate::texture_policy::scaled_size(self.scale, desc.width, desc.height);
            if fallback != (width, height) {
                reason = "native-alloc-fallback";
                resized = true;
                resample_opaque = Some(rgba.chunks_exact(4).all(|p| p[3] == 255));
                rfvp::perf::event(format_args!("native texture allocation failed id={} size={}x{}; retry {}x{}", id.0, width, height, fallback.0, fallback.1));
                let resize_start = rfvp::perf::start();
                {
                    let _resize = rfvp::perf::scope(rfvp::perf::PSV_RESIZE);
                    rgba = Cow::Owned(
                        crate::downscale::downscale_rgba8_region_with_opacity(&rgba, desc.width, desc.height,
                            fallback.0, fallback.1, 0, 0, fallback.0, fallback.1, resample_opaque.unwrap())
                            .ok_or(RfvpError::OutOfMemory)?,
                    );
                }
                resize_us += rfvp::perf::start().saturating_sub(resize_start);
                (width, height) = fallback;
                let upload_start = rfvp::perf::start();
                let _upload = rfvp::perf::scope(rfvp::perf::PSV_GPU_UPLOAD);
                handle = unsafe { rfvp_psv_gpu_create_texture(width, height, rgba.as_ptr(), 1) };
                upload_us += rfvp::perf::start().saturating_sub(upload_start);
            }
        }
        if handle.is_null() {
            return Err(RfvpError::OutOfMemory);
        }
        if rfvp::perf::enabled()
            && (resized
                || reason == "native-transparent-near-cap"
                || resize_us >= SLOW_TEXTURE_US
                || upload_us >= SLOW_TEXTURE_US)
        {
            rfvp::perf::event(format_args!(
                "texture create id={} fmt={:?} src={}x{} dst={}x{} scale={:.3} reason={} resize_us={} upload_us={}",
                id.0, desc.format, desc.width, desc.height, width, height, self.scale, reason, resize_us, upload_us
            ));
        }
        if let Some(old) = self.textures.insert(
            id.0,
            GpuTexture {
                handle,
                full_width: desc.width,
                full_height: desc.height,
                width,
                height,
                format: desc.format,
                resample_opaque,
            },
        ) {
            unsafe { rfvp_psv_gpu_destroy_texture(old.handle) };
        }
        Ok(())
    }

    fn update_texture_region_from_full(&mut self, id: TextureId, desc: TextureDesc,
        rect: TextureRect, pixels: &[u8]) -> RfvpResult<bool> {
        let _timing = rfvp::perf::scope(rfvp::perf::PSV_TEXTURE);
        let texture = match self.textures.get(&id.0) {
            Some(texture) if texture.full_width == desc.width && texture.full_height == desc.height
                && texture.format == PixelFormat::Rgba8 && desc.format == PixelFormat::Rgba8 => texture,
            _ => return Ok(false),
        };
        let len = (desc.width as usize).checked_mul(desc.height as usize)
            .and_then(|n| n.checked_mul(4)).ok_or(RfvpError::InvalidArgument)?;
        if pixels.len() < len { return Err(RfvpError::InvalidArgument); }
        if rect.width == 0 || rect.height == 0 { return Ok(true); }
        let right = rect.x.checked_add(rect.width).ok_or(RfvpError::InvalidArgument)?;
        let bottom = rect.y.checked_add(rect.height).ok_or(RfvpError::InvalidArgument)?;
        if right > desc.width || bottom > desc.height { return Err(RfvpError::InvalidArgument); }

        let mut resize_us = 0;
        let mut small = None;
        let (x, y, width, height, offset, stride) = if texture.width == desc.width && texture.height == desc.height {
            (rect.x, rect.y, rect.width, rect.height,
                (rect.y as usize * desc.width as usize + rect.x as usize) * 4, desc.width * 4)
        } else {
            // A global opaque/nonopaque branch transition can affect rounding outside
            // the patch. Recreate rather than leave differently filtered pixels behind.
            let opaque = pixels[..len].chunks_exact(4).all(|p| p[3] == 255);
            if texture.resample_opaque != Some(opaque) { return Ok(false); }
            if texture.width == 0 || texture.height == 0 || texture.width > desc.width || texture.height > desc.height {
                return Ok(false);
            }
            let rx = ((u64::from(desc.width)) << 16) / u64::from(texture.width);
            let ry = ((u64::from(desc.height)) << 16) / u64::from(texture.height);
            // Include a one-destination-pixel halo and sample from complete source rows.
            let x0 = (((u64::from(rect.x) << 16) / rx).saturating_sub(1) as u32).min(texture.width - 1);
            let y0 = (((u64::from(rect.y) << 16) / ry).saturating_sub(1) as u32).min(texture.height - 1);
            let x1 = ((((u64::from(right) << 16) + rx - 1) / rx + 1) as u32).min(texture.width);
            let y1 = ((((u64::from(bottom) << 16) + ry - 1) / ry + 1) as u32).min(texture.height);
            if x1 <= x0 || y1 <= y0 { return Ok(false); }
            let started = rfvp::perf::start();
            {
                let _resize = rfvp::perf::scope(rfvp::perf::PSV_RESIZE);
                small = Some(crate::downscale::downscale_rgba8_region_with_opacity(pixels, desc.width, desc.height,
                    texture.width, texture.height, x0, y0, x1 - x0, y1 - y0, opaque)
                    .ok_or(RfvpError::InvalidArgument)?);
            }
            resize_us = rfvp::perf::start().saturating_sub(started);
            (x0, y0, x1 - x0, y1 - y0, 0, (x1 - x0) * 4)
        };
        let data = small.as_deref().unwrap_or(&pixels[offset..]);
        let upload_started = rfvp::perf::start();
        let result = {
            let _upload = rfvp::perf::scope(rfvp::perf::PSV_GPU_UPLOAD);
            unsafe { rfvp_psv_gpu_update_texture_strided(texture.handle, x, y, width, height, data.as_ptr(), stride) }
        };
        if result != 0 { return Err(RfvpError::Backend); }
        let upload_us = rfvp::perf::start().saturating_sub(upload_started);
        rfvp::perf::event(format_args!(
            "texture partial id={} src={}x{} dirty={}x{}+{},{} dst={}x{}+{},{} bytes={} resize_us={} upload_us={}",
            id.0, desc.width, desc.height, rect.width, rect.height, rect.x, rect.y,
            width, height, x, y, width as u64 * height as u64 * 4, resize_us, upload_us));
        Ok(true)
    }

    fn update_texture(
        &mut self,
        id: TextureId,
        rect: TextureRect,
        pixels: &[u8],
    ) -> RfvpResult<()> {
        let _timing = rfvp::perf::scope(rfvp::perf::PSV_TEXTURE);
        let texture = self.textures.get(&id.0).ok_or(RfvpError::NotFound)?;
        if rect.width == 0 || rect.height == 0 {
            return Ok(());
        }
        let right = rect.x.checked_add(rect.width).ok_or(RfvpError::InvalidArgument)?;
        let bottom = rect.y.checked_add(rect.height).ok_or(RfvpError::InvalidArgument)?;
        if right > texture.full_width || bottom > texture.full_height {
            return Err(RfvpError::InvalidArgument);
        }
        // The engine's pixels are RGBA8 here (text and movie frames).
        let count = (rect.width as usize)
            .checked_mul(rect.height as usize)
            .ok_or(RfvpError::InvalidArgument)?;
        count.checked_mul(4).ok_or(RfvpError::InvalidArgument)?;
        let src = to_rgba8(PixelFormat::Rgba8, pixels, count)?;
        if texture.width == texture.full_width && texture.height == texture.full_height {
            let _upload = rfvp::perf::scope(rfvp::perf::PSV_GPU_UPLOAD);
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
        let resize_start = rfvp::perf::start();
        let small = {
            let _resize = rfvp::perf::scope(rfvp::perf::PSV_RESIZE);
            downscale_rgba8(&src, rect.width, rect.height, x1 - x0, y1 - y0)
                .ok_or(RfvpError::InvalidArgument)?
        };
        let resize_us = rfvp::perf::start().saturating_sub(resize_start);
        if rfvp::perf::enabled() && resize_us >= SLOW_TEXTURE_US {
            rfvp::perf::event(format_args!(
                "texture update id={} rect={}x{}+{},{} dst={}x{} reason=screen-size resize_us={}",
                id.0, rect.width, rect.height, rect.x, rect.y, x1 - x0, y1 - y0, resize_us
            ));
        }
        let _upload = rfvp::perf::scope(rfvp::perf::PSV_GPU_UPLOAD);
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
        let _timing = rfvp::perf::scope(rfvp::perf::PSV_THUMBNAIL);
        if !self.frame_done || width == 0 || height == 0 {
            return None;
        }
        let fb = unsafe { rfvp_psv_gpu_frame_pixels() };
        if fb.is_null() {
            return None;
        }
        let (x0, y0, x1, y1) = self.screen_rect(None);
        // The C bridge exposes exactly 960x544 packed RGBA8 pixels after GPU completion.
        let frame = unsafe {
            core::slice::from_raw_parts(fb, SCREEN_WIDTH as usize * SCREEN_HEIGHT as usize * 4)
        };
        crate::thumbnail::capture_rgba8(frame, SCREEN_WIDTH as usize, SCREEN_HEIGHT as usize,
            (x0 as usize, y0 as usize, x1 as usize, y1 as usize), width, height)
    }
}
