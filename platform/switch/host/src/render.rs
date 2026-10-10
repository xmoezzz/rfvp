use rfvp::host_api::{
    ColorRgba, DrawSolidCommand, DrawSpriteCommand, RfvpRenderer, RfvpResult, SoftRaster,
    TextureDesc, TextureId, TextureRect,
};

const DISPLAY_WIDTH: u32 = 1280;
const DISPLAY_HEIGHT: u32 = 720;

/// Draws the game with the shared software rasterizer straight into a display-sized frame (the
/// virtual frame is scaled and letterboxed as it is drawn), then copies that into the
/// framebuffer libnx lends for the frame (see `rfvp_switch_frame`).
pub struct HorizonRenderer {
    raster: SoftRaster,
    virtual_size: (u32, u32),
    frame_index: u64,
    /// The framebuffer of the frame being drawn, and its row length in pixels.
    target: *mut u32,
    target_stride: usize,
}

impl HorizonRenderer {
    pub fn new() -> Self {
        Self {
            raster: SoftRaster::new(DISPLAY_WIDTH, DISPLAY_HEIGHT),
            virtual_size: (0, 0),
            frame_index: 0,
            target: core::ptr::null_mut(),
            target_stride: 0,
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
    /// The last composed screen as a 32-bit BMP, for the debug report.
    pub fn screen_bmp(&self) -> alloc::vec::Vec<u8> {
        let (width, height) = (DISPLAY_WIDTH as usize, DISPLAY_HEIGHT as usize);
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

    pub fn screen_to_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        self.raster.output_to_virtual(x, y)
    }

    /// Where `present` copies the frame to; null outside a frame.
    pub fn set_target(&mut self, target: *mut u32, stride: usize) {
        self.target = target;
        self.target_stride = stride;
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
        if self.target.is_null() {
            return Ok(());
        }
        let width = DISPLAY_WIDTH as usize;
        // The rasterizer's pixels are `r, g, b, a` bytes, as RGBA_8888 wants them.
        for (y, row) in self.raster.frame().chunks_exact(width).enumerate() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    row.as_ptr(),
                    self.target.add(y * self.target_stride),
                    width,
                );
            }
        }
        Ok(())
    }
}
