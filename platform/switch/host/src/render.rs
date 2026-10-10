use alloc::sync::Arc;

use nx::gpu;
use nx::gpu::canvas::{AlphaBlend, Canvas, CanvasManager, RGBA8};
use nx::sync::RwLock;
use rfvp::host_api::{
    ColorRgba, DrawSolidCommand, DrawSpriteCommand, RfvpError, RfvpRenderer, RfvpResult,
    SoftRaster, TextureDesc, TextureId, TextureRect,
};

const DISPLAY_WIDTH: u32 = gpu::SCREEN_WIDTH;
const DISPLAY_HEIGHT: u32 = gpu::SCREEN_HEIGHT;
const BUFFER_COUNT: u32 = 3;

/// Draws the game with the shared software rasterizer straight into a display-sized
/// framebuffer (the virtual frame is scaled and letterboxed as it is drawn), then copies that to
/// the screen once per frame.
pub struct HorizonRenderer {
    raster: SoftRaster,
    virtual_size: (u32, u32),
    frame_index: u64,
    canvas: Option<CanvasManager<RGBA8>>,
}

impl HorizonRenderer {
    pub fn new() -> Self {
        Self {
            raster: SoftRaster::new(DISPLAY_WIDTH, DISPLAY_HEIGHT),
            virtual_size: (0, 0),
            frame_index: 0,
            canvas: None,
        }
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

    /// Map a point on the screen to the game's virtual coordinates.
    pub fn screen_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        self.raster.output_to_virtual(x, y)
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
        self.virtual_size = (width, height);
        self.raster.begin_frame(width, height, clear)
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        self.raster.draw_sprite(command)
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        self.raster.draw_solid(command)
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        self.frame_index = self.frame_index.wrapping_add(1);
        self.raster.end_frame()
    }

    fn capture_thumbnail(&mut self, width: u32, height: u32) -> Option<alloc::vec::Vec<u8>> {
        Some(self.raster.thumbnail(width, height))
    }

    fn present(&mut self) -> RfvpResult<()> {
        self.ensure_canvas()?;
        let width = DISPLAY_WIDTH as usize;
        let frame = self.raster.frame();
        let canvas = self.canvas.as_mut().ok_or(RfvpError::Backend)?;
        // The canvas buffers draws in a linear buffer and swizzles them to the screen when the
        // closure returns, so writing in row order is cheap.
        canvas
            .render(None, |canvas| {
                for (index, pixel) in frame.iter().enumerate() {
                    // The rasterizer's pixels are `r, g, b, a` bytes; the canvas wants the same
                    // bytes with `a` as the lowest field, which is a byte swap.
                    canvas.draw_single(
                        (index % width) as i32,
                        (index / width) as i32,
                        RGBA8::from_bits(pixel.swap_bytes()),
                        AlphaBlend::None,
                    );
                }
                Ok(())
            })
            .map_err(|_| RfvpError::Backend)?;
        canvas
            .wait_vsync_event(None)
            .map_err(|_| RfvpError::Backend)
    }
}
