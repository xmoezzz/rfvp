use rfvp::host_api::{
    ColorRgba, DrawSolidCommand, DrawSpriteCommand, RfvpRenderer, RfvpResult, SoftRaster,
    TextureDesc, TextureId, TextureRect,
};

use crate::status::status_to_result;

/// The Vita screen.
const SCREEN_WIDTH: u32 = 960;
const SCREEN_HEIGHT: u32 = 544;

extern "C" {
    /// Hand a finished RGBA8 frame to the platform's present callback.
    fn rfvp_psv_c_present_rgba(rgba8: *const u8, width: u32, height: u32, stride_bytes: u32)
        -> i32;
}

/// Draws the game with the shared software rasterizer straight into a screen-sized framebuffer
/// (the virtual frame is scaled and letterboxed as it is drawn) and gives that to the platform
/// to put on the display.
pub struct PsvRenderer {
    raster: SoftRaster,
}

impl PsvRenderer {
    pub fn new() -> Self {
        Self {
            raster: SoftRaster::new(SCREEN_WIDTH, SCREEN_HEIGHT),
        }
    }

    /// Map a point on the screen to the game's virtual coordinates.
    pub fn screen_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        self.raster.output_to_virtual(x, y)
    }
}

impl Default for PsvRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpRenderer for PsvRenderer {
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
        self.raster.begin_frame(width, height, clear)
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        self.raster.draw_sprite(command)
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        self.raster.draw_solid(command)
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        self.raster.end_frame()
    }

    fn capture_thumbnail(&mut self, width: u32, height: u32) -> Option<alloc::vec::Vec<u8>> {
        Some(self.raster.thumbnail(width, height))
    }

    fn present(&mut self) -> RfvpResult<()> {
        let (width, height) = self.raster.output_size();
        let frame = self.raster.frame();
        // The rasterizer's pixels are `r, g, b, a` bytes, which is what the platform expects.
        let status = unsafe {
            rfvp_psv_c_present_rgba(frame.as_ptr().cast::<u8>(), width, height, width * 4)
        };
        status_to_result(status)
    }
}
