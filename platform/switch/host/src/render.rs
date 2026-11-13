//! RFVP Switch software renderer with host-selected output dimensions.
//! Output dimensions come from the libnx host, including dock/undock changes.
//! Framebuffers are borrowed for a checked row copy, never retained as pointers.
#![forbid(unsafe_code)]

use rfvp::host_api::{
    ColorRgba, DrawSolidCommand, DrawSpriteCommand, RfvpError, RfvpRenderer, RfvpResult, SoftRaster,
    TextureDesc, TextureId, TextureRect,
};

/// Output memory admission limit, independent of handheld/docked policy.
pub const MAX_OUTPUT_PIXELS: u64 = 8_388_608;

/// Draws the game with the shared software rasterizer straight into a display-sized frame (the
/// virtual frame is scaled and letterboxed as it is drawn). The libnx
/// output owner borrows its libnx framebuffer to `copy_to_framebuffer` after a
/// completed frame; this renderer never owns a native window or device.
pub struct HorizonRenderer {
    raster: SoftRaster,
    virtual_size: (u32, u32),
    frame_index: u64,
    frame_ready: bool,
    frame_open: bool,
}

impl HorizonRenderer {
    /// The console host supplies its actual output dimensions. Dock status is
    /// not a resolution: host display configuration chooses the surface size.
    pub fn new(width: u32, height: u32) -> RfvpResult<Self> {
        Self::validate_output(width, height)?;
        let mut raster = SoftRaster::new(1, 1);
        raster.resize_output(width, height)?;
        Ok(Self {
            raster,
            virtual_size: (0, 0),
            frame_index: 0,
            frame_ready: false,
            frame_open: false,
        })
    }

    fn validate_output(width: u32, height: u32) -> RfvpResult<()> {
        if width == 0 || height == 0 {
            return Err(RfvpError::InvalidArgument);
        }
        if u64::from(width) * u64::from(height) > MAX_OUTPUT_PIXELS {
            return Err(RfvpError::CapacityExceeded);
        }
        Ok(())
    }

    pub fn output_size(&self) -> (u32, u32) {
        self.raster.output_size()
    }

    /// Apply a host surface change between frames, including dock/undock.
    /// Native textures, canvas size and VM state remain intact. The host must
    /// retire the old presentation before resizing; the next frame redraws.
    pub fn resize_output(&mut self, width: u32, height: u32) -> RfvpResult<()> {
        if self.frame_open {
            return Err(RfvpError::Backend);
        }
        Self::validate_output(width, height)?;
        if (width, height) != self.output_size() {
            self.raster.resize_output(width, height)?;
            self.frame_ready = false;
        }
        Ok(())
    }

    /// Size of the game frame in virtual (script) coordinates.
    pub const fn frame_width(&self) -> u32 {
        self.virtual_size.0
    }

    pub const fn frame_height(&self) -> u32 {
        self.virtual_size.1
    }

    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// The last composed screen as a 32-bit BMP, for an explicit host capture.
    pub fn screen_bmp(&self) -> alloc::vec::Vec<u8> {
        let (width, height) = self.output_size();
        let (width, height) = (width as usize, height as usize);
        let image_size = width * height * 4;
        let mut bmp = alloc::vec::Vec::with_capacity(54 + image_size);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&((54 + image_size) as u32).to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes());
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(width as i32).to_le_bytes());
        // Negative height: rows top to bottom.
        bmp.extend_from_slice(&(-(height as i32)).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&32u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 24]);
        for pixel in self.raster.frame() {
            let [r, g, b, a] = pixel.to_ne_bytes();
            bmp.extend_from_slice(&[b, g, r, a]);
        }
        bmp
    }

    /// Map screen pixels through the same letterbox transform used for drawing.
    pub fn screen_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        self.raster.output_to_virtual(x, y)
    }

    /// Copy one completed image to the host's linear RGBA_8888 framebuffer.
    /// `stride_bytes` is the byte pitch returned by libnx `framebufferBegin`.
    /// Row padding remains untouched; the host owns `framebufferEnd`, vblank,
    /// buffer reuse and suspend/shutdown. No target pointer survives this call.
    pub fn copy_to_framebuffer(&self, target: &mut [u32], stride_bytes: usize) -> RfvpResult<()> {
        if !self.frame_ready {
            return Err(RfvpError::Backend);
        }
        let pixel_bytes = core::mem::size_of::<u32>();
        let (width, height) = self.output_size();
        let (width, height) = (width as usize, height as usize);
        if stride_bytes % pixel_bytes != 0 {
            return Err(RfvpError::InvalidArgument);
        }
        let stride = stride_bytes / pixel_bytes;
        if stride < width {
            return Err(RfvpError::InvalidArgument);
        }
        let required = (height - 1).checked_mul(stride)
            .and_then(|n| n.checked_add(width))
            .ok_or(RfvpError::InvalidArgument)?;
        if target.len() < required {
            return Err(RfvpError::InvalidArgument);
        }
        for (y, row) in self.raster.frame().chunks_exact(width).enumerate() {
            target[y * stride..y * stride + width].copy_from_slice(row);
        }
        Ok(())
    }

    /// The complete screen image, tightly packed RGBA_8888 words. This borrow
    /// must end before the next renderer mutation; it is output state, not a save.
    pub fn frame(&self) -> Option<&[u32]> {
        self.frame_ready.then(|| self.raster.frame())
    }
}

impl RfvpRenderer for HorizonRenderer {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        self.raster.create_texture(id, desc, pixels)
    }

    fn update_texture(
        &mut self,
        id: TextureId,
        rect: TextureRect,
        pixels: &[u8],
    ) -> RfvpResult<()> {
        self.raster.update_texture(id, rect, pixels)
    }

    fn destroy_texture(&mut self, id: TextureId) {
        self.raster.destroy_texture(id);
    }

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        if self.frame_open {
            return Err(RfvpError::Backend);
        }
        self.frame_ready = false;
        self.raster.begin_frame(width, height, clear)?;
        self.virtual_size = (width, height);
        self.frame_open = true;
        Ok(())
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        self.raster.draw_sprite(command)
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        self.raster.draw_solid(command)
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        if !self.frame_open {
            return Err(RfvpError::Backend);
        }
        self.raster.end_frame()?;
        self.frame_open = false;
        self.frame_index = self.frame_index.wrapping_add(1);
        self.frame_ready = true;
        Ok(())
    }

    fn capture_thumbnail(&mut self, width: u32, height: u32) -> Option<alloc::vec::Vec<u8>> {
        Some(self.raster.thumbnail(width, height))
    }

    fn present(&mut self) -> RfvpResult<()> {
        // Native composition completes here; device presentation belongs to the
        // libnx runtime, which calls copy_to_framebuffer before release.
        if !self.frame_ready {
            return Err(RfvpError::Backend);
        }
        Ok(())
    }
}
