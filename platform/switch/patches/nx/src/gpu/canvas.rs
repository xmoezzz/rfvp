use core::marker::PhantomData;
use core::ops::Mul;

use alloc::sync::Arc;
use bitfield_struct::bitfield;
//use num_traits::float::Float;
use sealed::CanvasColorFormat;

use crate::arm;
use crate::mem::alloc::Buffer;
use crate::result::Result;
use crate::sync::RwLock;

use crate::{ipc::sf::AppletResourceUserId, service::vi::LayerFlags};

use super::surface::{ScaleMode, Surface};
use super::{BlockLinearHeights, ColorFormat, Context, LayerZ, MultiFence, PixelFormat};

#[cfg(feature = "truetype")]
pub type Font<'a> = ab_glyph::FontRef<'a>;

#[derive(Debug, Clone, Copy, Default)]
pub enum AlphaBlend {
    None,
    #[default]
    Source,
    Destination,
}
/// RGBA (4444) Pixel/Color Representation
#[bitfield(u16, order = Lsb)]
pub struct RGBA4 {
    /// Alpha channel
    #[bits(4)]
    pub b: u8,
    /// Blue channel
    #[bits(4)]
    pub a: u8,
    /// Green channel
    #[bits(4)]
    pub r: u8,
    /// Red channel
    #[bits(4)]
    pub g: u8,
}

impl RGBA4 {
    /// Takes in standard 8-bit colors and scales them down to fit the range of 4-bit channels (via 4-bit right shift)
    #[inline]
    pub const fn new_scaled(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new()
            .with_r(r >> 4)
            .with_g(g >> 4)
            .with_b(b >> 4)
            .with_a(a >> 4)
    }

    #[inline]
    fn blend_channel(channel: u8, other: u8, alpha: u8) -> u8 {
        (channel * alpha + other * (15 - alpha)) >> 4
    }
}

impl sealed::CanvasColorFormat for RGBA4 {
    type RawType = u16;

    const COLOR_FORMAT: ColorFormat = ColorFormat::A4B4G4R4;
    const PIXEL_FORMAT: PixelFormat = PixelFormat::RGBA_4444;

    fn new() -> Self {
        Self::new()
    }

    /// Takes in standard 8-bit colors and scales them down to fit the range of 4-bit channels (via 4-bit right shift)
    #[inline]
    fn new_scaled(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new_scaled(r, g, b, a)
    }

    fn to_raw(self) -> Self::RawType {
        self.0.to_be()
    }

    fn from_raw(raw: Self::RawType) -> Self {
        Self::from_bits(raw)
    }

    fn blend_with(self, other: Self, blend_mode: AlphaBlend) -> Self {
        match blend_mode {
            AlphaBlend::None => self,
            AlphaBlend::Source => self
                .with_r(Self::blend_channel(self.r(), other.r(), self.a()))
                .with_g(Self::blend_channel(self.g(), other.g(), self.a()))
                .with_b(Self::blend_channel(self.b(), other.b(), self.a()))
                .with_a(other.a()),
            AlphaBlend::Destination => self
                .with_r(Self::blend_channel(self.r(), other.r(), self.a()))
                .with_g(Self::blend_channel(self.g(), other.g(), self.a()))
                .with_b(Self::blend_channel(self.b(), other.b(), self.a()))
                .with_a((self.a() + other.a()).min(0xF)),
        }
    }

    fn scale_alpha(self, alpha: f32) -> Self {
        let new_alpha = alpha * self.a() as f32;
        self.with_a(new_alpha as u8)
    }
}

/// RGBA (8888) Pixel/Color Representation
///
/// We are using the bitfields crate here just for consistency with RGBA4444.
/// The bit range checks should be optimized out in release mode.
// rfvp patch: laid out as `A8B8G8R8` (bytes r, g, b, a in memory), the format libnx pairs with
// RGBA_8888. Upstream used `R8G8B8A8`, which Ryujinx's compositor does not implement.
#[bitfield(u32, order = Lsb)]
pub struct RGBA8 {
    /// Red channel
    #[bits(8)]
    pub r: u8,
    /// Green channel
    #[bits(8)]
    pub g: u8,
    /// Blue channel
    #[bits(8)]
    pub b: u8,
    /// Alpha channel
    #[bits(8)]
    pub a: u8,
}

impl RGBA8 {
    pub const fn new_scaled(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new().with_r(r).with_g(g).with_b(b).with_a(a)
    }

    fn blend_channel(channel: u8, other: u8, alpha: u8) -> u8 {
        ((u16::from(channel).mul(alpha as u16)
            + u16::from(other).mul(u8::MAX as u16 - alpha as u16))
            / 0xff) as u8
    }
}

impl sealed::CanvasColorFormat for RGBA8 {
    type RawType = u32;
    const COLOR_FORMAT: ColorFormat = ColorFormat::A8B8G8R8;
    const PIXEL_FORMAT: PixelFormat = PixelFormat::RGBA_8888;

    fn new() -> Self {
        Self::new()
    }

    fn to_raw(self) -> Self::RawType {
        self.0
    }

    fn from_raw(raw: Self::RawType) -> Self {
        Self::from_bits(raw)
    }

    fn new_scaled(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new_scaled(r, g, b, a)
    }

    fn blend_with(self, other: Self, blend_mode: AlphaBlend) -> Self {
        match blend_mode {
            AlphaBlend::None => self,
            AlphaBlend::Source => self
                .with_r(Self::blend_channel(self.r(), other.r(), self.a()))
                .with_g(Self::blend_channel(self.g(), other.g(), self.a()))
                .with_b(Self::blend_channel(self.b(), other.b(), self.a()))
                .with_a(other.a()),
            AlphaBlend::Destination => self
                .with_r(Self::blend_channel(self.r(), other.r(), self.a()))
                .with_g(Self::blend_channel(self.g(), other.g(), self.a()))
                .with_b(Self::blend_channel(self.b(), other.b(), self.a()))
                .with_a(self.a().saturating_add(other.a())),
        }
    }

    fn scale_alpha(self, alpha: f32) -> Self {
        let new_alpha = alpha * self.a() as f32;
        self.with_a(new_alpha as u8)
    }
}

pub(crate) mod sealed {
    use super::{AlphaBlend, ColorFormat, PixelFormat};

    pub trait CanvasColorFormat: Copy + Default + 'static {
        /// The raw type that will be written into the backing buffer;
        type RawType: num_traits::PrimInt + 'static;

        /// The native color format used
        const COLOR_FORMAT: ColorFormat;
        /// The native pixel format
        const PIXEL_FORMAT: PixelFormat;
        /// The size of the raw pixel in bytes (must be the same size as the RawType)
        const BYTES_PER_PIXEL: u32 = Self::COLOR_FORMAT.bytes_per_pixel();

        /// Create a new default instance of the type
        fn new() -> Self;

        /// Create an instance of the type scaled from standard 888 color values
        fn new_scaled(r: u8, g: u8, b: u8, a: u8) -> Self;
        /// Transmute from the raw backing type
        fn from_raw(raw: Self::RawType) -> Self;
        /// Transmute to the raw backing type
        fn to_raw(self) -> Self::RawType;
        /// Alpha blend the pixels together
        fn blend_with(self, other: Self, blend: AlphaBlend) -> Self;
        /// Scale the color's alpha channel
        ///
        /// `alpha` must be between 0 and 1, or the implementation should panic.
        fn scale_alpha(self, alpha: f32) -> Self;
    }
}

pub struct CanvasManager<ColorFormat: sealed::CanvasColorFormat> {
    pub surface: super::surface::Surface,
    _color_fmt: PhantomData<ColorFormat>,
}

#[allow(clippy::too_many_arguments)]
impl<ColorFormat: sealed::CanvasColorFormat> CanvasManager<ColorFormat> {
    /// Computes the heap memory required to contain all the frame buffers for the provided parameters.
    pub const fn total_heap_required(
        width: u32,
        height: u32,
        block_height_config: BlockLinearHeights,
        buffer_count: u32,
    ) -> usize {
        let pitch = align_up!(width * ColorFormat::COLOR_FORMAT.bytes_per_pixel(), 64u32);
        let aligned_height = align_up!(height, block_height_config.block_height_bytes());
        let single_buffer_size: usize = align_up!(
            pitch as usize * aligned_height as usize,
            crate::mem::alloc::PAGE_ALIGNMENT
        );

        buffer_count as usize * single_buffer_size
    }
    /// Creates a new managed layer (application/applet window) that can be drawn on overlay elements.
    ///
    /// These layers exist on top of stray layers and application/applet UIs, and can be Z-order vertically layered over each other.
    /// The GPU provides the alpha blending for all layers based on the committed frame.
    pub fn new_managed(
        gpu_ctx: Arc<RwLock<Context>>,
        surface_name: super::surface::DisplayName,
        x: u32,
        y: u32,
        z: LayerZ,
        width: u32,
        height: u32,
        aruid: AppletResourceUserId,
        layer_flags: LayerFlags,
        buffer_count: u32,
        block_height: BlockLinearHeights,
        scaling: ScaleMode,
    ) -> Result<Self> {
        let raw_surface = Surface::new_managed(
            gpu_ctx,
            surface_name,
            aruid,
            layer_flags,
            x as f32,
            y as f32,
            z,
            width,
            height,
            buffer_count,
            block_height,
            ColorFormat::COLOR_FORMAT,
            ColorFormat::PIXEL_FORMAT,
            scaling,
        )?;
        Ok(Self {
            surface: raw_surface,
            _color_fmt: PhantomData,
        })
    }

    /// Creates a new stray layer (application/applet window) that can be drawn on for UI elements
    #[inline(always)]
    pub fn new_stray(
        gpu_ctx: Arc<RwLock<Context>>,
        surface_name: super::surface::DisplayName,
        buffer_count: u32,
        block_height: BlockLinearHeights,
    ) -> Result<Self> {
        let raw_surface = Surface::new_stray(
            gpu_ctx,
            surface_name,
            buffer_count,
            block_height,
            ColorFormat::COLOR_FORMAT,
            ColorFormat::PIXEL_FORMAT,
        )?;
        Ok(Self {
            surface: raw_surface,
            _color_fmt: PhantomData,
        })
    }

    /// Wait for a vsync event to ensure that the previously submitted frame has been fully rendered to the display.
    #[inline(always)]
    pub fn wait_vsync_event(&self, timeout: Option<i64>) -> Result<()> {
        self.surface.wait_vsync_event(timeout.unwrap_or(-1))
    }

    /// Check out a canvas/framebuffer to draw a new frame.
    ///
    /// This frame is buffered to a linear buffer before being swizzled
    ///  into the backing buffer during frame-commit. This provides better performance for line-based renderers
    /// as writes will be sequential in memory. There are cache misses during commit, but that is still better performance
    /// than mapped page churn for GPU-mapped memory.
    #[inline(always)]
    pub fn render<T>(
        &mut self,
        clear_color: Option<ColorFormat>,
        runner: impl Fn(&mut BufferedCanvas<'_, ColorFormat>) -> Result<T>,
    ) -> Result<T> {
        let (buffer, buffer_length, slot, _fence_present, fences) =
            self.surface.dequeue_buffer(false)?;

        let mut canvas = BufferedCanvas {
            slot,
            // This transmute is OK as we're only implementing this for types that are secretly just primitive ints
            linear_buf: Buffer::new(
                size_of::<u128>(), // we do this as the gob conversion is done in blocks of 8 bytes, casting each block to `*mut u128`
                buffer_length,
            )?,
            base_pointer: buffer as usize,
            fences,
            buffer_size: buffer_length,
            manager: self,
        };

        if let Some(background_color) = clear_color {
            canvas.clear(background_color);
        }
        runner(&mut canvas)
    }

    /// Renders a pre-prepared buffer of pixels (represented by their color values) directly to the screen.
    ///
    /// A maximum of `(width * height)` pixels are read from the input buffer line-by-line to the screen. Extra
    /// pixels are discarded, and no error is returned for partial refreshes.
    pub fn render_prepared_buffer(&mut self, pixels: &[ColorFormat]) -> Result<()> {
        let height = self.surface.height() as usize;
        let width = self.surface.width() as usize;

        let (buffer, buffer_length, slot, _fence_present, fences) =
            self.surface.dequeue_buffer(false)?;

        let mut canvas = UnbufferedCanvas {
            slot,
            base_pointer: buffer as usize,
            fences,
            buffer_size: buffer_length,
            manager: self,
        };

        for (index, pixel) in pixels.iter().cloned().take(height * width).enumerate() {
            canvas.draw_single(
                (index % width) as i32,
                (index / width) as i32,
                pixel,
                AlphaBlend::None,
            );
        }

        Ok(())
    }

    /// Check out a canvas/framebuffer to draw a new frame, without a linear buffer.
    ///
    /// This can be used in memory constrained environments such as sysmodules, but also provides slightly better performance
    /// for frames that draw in block rather than scan-lines (e.g. JPEG decoding).
    #[inline(always)]
    pub fn render_unbuffered<T>(
        &mut self,
        clear_color: Option<ColorFormat>,
        runner: impl Fn(&mut UnbufferedCanvas<'_, ColorFormat>) -> Result<T>,
    ) -> Result<T> {
        let (buffer, buffer_length, slot, _fence_present, fences) =
            self.surface.dequeue_buffer(false)?;

        let mut canvas = UnbufferedCanvas {
            slot,
            base_pointer: buffer as usize,
            fences,
            buffer_size: buffer_length,
            manager: self,
        };

        if let Some(background_color) = clear_color {
            canvas.clear(background_color);
        }

        runner(&mut canvas)
    }
}

#[allow(clippy::too_many_arguments)]
pub trait Canvas {
    type ColorFormat: sealed::CanvasColorFormat;

    /// Draw a single pixel to the canvas
    ///
    /// out-of-bounds co-ordinates should be ignored by the implementation.
    fn draw_single(&mut self, x: i32, y: i32, color: Self::ColorFormat, blend: AlphaBlend);
    fn height(&self) -> u32;
    fn width(&self) -> u32;

    fn clear(&mut self, color: Self::ColorFormat) {
        for y in 0..self.height() as i32 {
            for x in 0..self.width() as i32 {
                self.draw_single(x, y, color, AlphaBlend::None);
            }
        }
    }

    /// Draw a single-color rectangle to the canvas
    ///
    /// out-of-bounds co-ordinates should be ignored by the implementation.
    fn draw_rect(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: Self::ColorFormat,
        blend: AlphaBlend,
    ) {
        let s_width = self.width() as i32;
        let s_height = self.height() as i32;
        let x0 = x.clamp(0, s_width);
        let x1 = x.saturating_add_unsigned(width).clamp(0, s_width);
        let y0 = y.clamp(0, s_height);
        let y1 = y.saturating_add_unsigned(height).clamp(0, s_height);
        for y in y0..y1 {
            for x in x0..x1 {
                self.draw_single(x, y, color, blend);
            }
        }
    }

    /// Draw a straight line from the start co-ordinates to the end co-ordinates
    ///
    /// Start/end co-ordinates may be outside the screen, but out-of-bounds pixel co-ordinates should be ignored by the implementation.
    fn draw_line(
        &mut self,
        start: (i32, i32),
        end: (i32, i32),
        width: u32,
        color: Self::ColorFormat,
        blend: AlphaBlend,
    ) {
        for (x, y) in line_drawing::Bresenham::new(start, end) {
            // TODO - fix this stupid algorithm
            self.draw_circle_filled(x, y, width, color, blend)
        }
    }

    /// Draw an empty circle from the centre co-ordinates and radius.
    ///
    /// The shape does not need to be fully in-bounds, but out-of-bounds pixel co-ordinates should be ignored by the implementation.
    fn draw_circle(
        &mut self,
        x: i32,
        y: i32,
        r: u32,
        line_width: u32,
        color: Self::ColorFormat,
        blend: AlphaBlend,
    ) {
        for (x, y) in line_drawing::BresenhamCircle::new(x, y, r as i32) {
            // TODO - fix this stupid algorithm
            self.draw_circle_filled(x, y, line_width, color, blend);
        }
    }

    /// Draw an filled circle from the centre co-ordinates and radius.
    ///
    /// The shape does not need to be fully in-bounds, but out-of-bounds pixel co-ordinates should be ignored by the implementation.
    fn draw_circle_filled(
        &mut self,
        cx: i32,
        cy: i32,
        r: u32,
        color: Self::ColorFormat,
        blend: AlphaBlend,
    ) {
        for row in cy.saturating_sub(r as i32)..cy.saturating_add(r as i32) {
            if (0..self.height() as i32).contains(&row) {
                // Find the width of the row to draw using a rearranged pythagoras's theorem.
                // Uses i64s internally as it should all be in registers (no extra memory use)
                // and the squaring of an i32 should never overflow an i64
                let x_width = ((r as i64).pow(2) - ((row - cy) as i64).pow(2)).isqrt() as i32;
                for column in cx.saturating_sub(x_width)..cx.saturating_add(x_width) {
                    if (0..self.width() as i32).contains(&column) {
                        self.draw_single(column, row, color, blend);
                    }
                }
            }
        }
    }

    /// Draw text in a true-type font to the canvas.
    ///
    /// The text does not need to be fully in-bounds, but out-of-bounds pixel co-ordinates should be ignored by the implementation.
    #[cfg(feature = "truetype")]
    fn draw_font_text(
        &mut self,
        font: &Font,
        text: impl AsRef<str>,
        color: Self::ColorFormat,
        size: f32,
        x: i32,
        y: i32,
        blend: AlphaBlend,
    ) {
        use ab_glyph::{Font, Glyph, Point, ScaleFont};
        use alloc::vec::Vec;
        let text = text.as_ref();
        let position: Point = (x as f32, y as f32).into();
        let scale = font.pt_to_px_scale(size).unwrap();
        let font = font.as_scaled(scale);

        let v_advance = font.height() + font.line_gap();
        let mut caret = position + ab_glyph::point(0.0, font.ascent());
        let mut last_glyph: Option<Glyph> = None;
        let mut target: Vec<Glyph> = Vec::new();
        for c in text.chars() {
            if c.is_control() {
                if c == '\n' {
                    caret = ab_glyph::point(position.x, caret.y + v_advance);
                    last_glyph = None;
                }
                continue;
            }
            let mut glyph = font.scaled_glyph(c);
            if let Some(previous) = last_glyph.take() {
                caret.x += font.kern(previous.id, glyph.id);
            }
            glyph.position = caret;

            last_glyph = Some(glyph.clone());
            caret.x += font.h_advance(glyph.id);

            if !c.is_whitespace() && caret.x > position.x + self.width() as f32 {
                caret = ab_glyph::point(position.x, caret.y + v_advance);
                glyph.position = caret;
                last_glyph = None;
            }

            target.push(glyph);
        }

        for glyph in target {
            if let Some(outline) = font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                outline.draw(|d_x, d_y, c| {
                    if c > 0.2 {
                        // we don't want to cover subpixel issues, so we're just not going to color the pixel unless it's >20% covered.
                        let pix_color = color.scale_alpha(c);
                        self.draw_single(
                            bounds.min.x as i32 + d_x as i32,
                            bounds.min.y as i32 + d_y as i32,
                            pix_color,
                            blend,
                        );
                    }
                });
            }
        }
    }

    /// Draw text in a 8x8 monospace font to the canvas.
    ///
    /// The text does not need to be fully in-bounds, but out-of-bounds pixel co-ordinates should be ignored by the implementation.
    #[cfg(feature = "fonts")]
    #[inline(always)]
    fn draw_ascii_bitmap_text(
        &mut self,
        text: impl AsRef<str>,
        color: Self::ColorFormat,
        scale: u32,
        x: i32,
        y: i32,
        blend: AlphaBlend,
    ) {
        self.draw_bitmap_text(text, &font8x8::BASIC_FONTS, color, scale, x, y, blend);
    }

    /// Draw text to the canvas, with a unicode font from the [`font8x8`] crate.
    ///
    /// The text does not need to be fully in-bounds, but out-of-bounds pixel co-ordinates should be ignored by the implementation.
    #[cfg(feature = "fonts")]
    fn draw_bitmap_text(
        &mut self,
        text: impl AsRef<str>,
        character_set: &font8x8::unicode::BasicFonts,
        color: Self::ColorFormat,
        scale: u32,
        x: i32,
        y: i32,
        blend: AlphaBlend,
    ) {
        use font8x8::UnicodeFonts;
        let text = text.as_ref();
        let mut tmp_x = x;
        let mut tmp_y = y;
        for c in text.chars() {
            match c {
                '\r' => {
                    // carriage return moves the caret to the start of the line
                    tmp_x = x;
                }
                '\n' => {
                    // new line uses unix convention, advancing the line and moving the caret to the start of the line
                    tmp_y = tmp_y.saturating_add_unsigned(8 * scale);
                    tmp_x = x;
                }
                _ => {
                    if let Some(glyph) = character_set.get(c) {
                        let char_tmp_x = tmp_x;
                        let char_tmp_y = tmp_y;
                        for gx in glyph.into_iter() {
                            for bit in 0..8 {
                                match gx & (1 << bit) {
                                    0 => {}
                                    _ => {
                                        self.draw_rect(tmp_x, tmp_y, scale, scale, color, blend);
                                    }
                                }
                                tmp_x += scale as i32;
                            }
                            tmp_y += scale as i32;
                            tmp_x = char_tmp_x;
                        }
                        tmp_x = tmp_x.saturating_add_unsigned(8 * scale);
                        tmp_y = char_tmp_y;
                    }
                }
            }
        }
    }
}

pub struct BufferedCanvas<'fb, ColorFormat: sealed::CanvasColorFormat> {
    slot: i32,
    linear_buf: Buffer<u8>,
    base_pointer: usize,
    buffer_size: usize,
    fences: MultiFence,
    manager: &'fb mut CanvasManager<ColorFormat>,
}

impl<ColorFormat: sealed::CanvasColorFormat> BufferedCanvas<'_, ColorFormat> {
    /// Write from the pixel buffer to the backing framebuffer in a memory efficient way for GPU page boundaries.
    fn convert_buffers(&mut self) {
        let block_config: BlockLinearHeights = self.manager.surface.get_block_linear_config();
        let block_height_gobs = block_config.block_height();
        let block_height_px = block_config.block_height_bytes();

        let mut out_buf = self.base_pointer as *mut u8;
        let in_buf = self.linear_buf.ptr as *const u8;
        let pitch = self.manager.surface.pitch();
        let height = self.manager.surface.height();

        let width_blocks = pitch >> 6;
        let height_blocks =
            align_up!(self.height(), block_config.block_height()) / block_config.block_height();

        for block_y in 0..height_blocks {
            for block_x in 0..width_blocks {
                for gob_y in 0..block_height_gobs {
                    unsafe {
                        let x = block_x * 64;
                        let y = block_y * block_height_px + gob_y * 8;
                        if y < height {
                            let in_gob_buf = in_buf.add((y * pitch + x) as usize);
                            Self::convert_buffers_gob_impl(out_buf, in_gob_buf, pitch);
                        }
                        out_buf = out_buf.add(512);
                    }
                }
            }
        }
    }

    /// Converts a pointer to a linear block to Generic16Bx2 sector ordering.
    fn convert_buffers_gob_impl(out_gob_buf: *mut u8, in_gob_buf: *const u8, stride: u32) {
        unsafe {
            let mut out_gob_buf_128 = out_gob_buf as *mut u128;
            for i in 0..32 {
                let y = ((i >> 1) & 0x6) | (i & 0x1);
                let x = ((i << 3) & 0x10) | ((i << 1) & 0x20);
                let in_gob_buf_128 = in_gob_buf.offset((y * stride + x) as isize) as *const u128;
                core::ptr::copy(in_gob_buf_128, out_gob_buf_128, 1);
                out_gob_buf_128 = out_gob_buf_128.add(1);
            }
        }
    }
}

impl<ColorFormat: CanvasColorFormat> Canvas for BufferedCanvas<'_, ColorFormat> {
    type ColorFormat = ColorFormat;

    fn clear(&mut self, color: Self::ColorFormat) {
        let raw_color: <ColorFormat as CanvasColorFormat>::RawType = color.to_raw();
        let raw_buffer: &mut [<ColorFormat as CanvasColorFormat>::RawType] = unsafe {
            core::slice::from_raw_parts_mut(
                self.linear_buf.ptr as *mut <ColorFormat as CanvasColorFormat>::RawType,
                self.linear_buf.layout.size()
                    / <ColorFormat as CanvasColorFormat>::BYTES_PER_PIXEL as usize,
            )
        };

        for pixel_slot in raw_buffer.iter_mut() {
            *pixel_slot = raw_color;
        }
    }
    fn height(&self) -> u32 {
        self.manager.surface.height()
    }
    fn width(&self) -> u32 {
        self.manager.surface.width()
    }

    fn draw_single(&mut self, x: i32, y: i32, color: ColorFormat, blend: AlphaBlend) {
        if !(0..self.manager.surface.width() as i32).contains(&x)
            || !(0..self.manager.surface.height() as i32).contains(&y)
        {
            return;
        }
        let pixel_offset = (self.manager.surface.pitch() * y as u32
            + x as u32 * ColorFormat::COLOR_FORMAT.bytes_per_pixel())
            as usize;

        debug_assert!(
            pixel_offset < self.buffer_size,
            "in-bounds pixels should never lead to out-of bounds reads/writes"
        );

        let out_color = color.blend_with(
            ColorFormat::from_raw(unsafe {
                core::ptr::read(self.linear_buf.ptr.add(pixel_offset) as *mut ColorFormat::RawType)
            }),
            blend,
        );

        // SAFETY - we know get_unchecked access below is OK as we have checked the dimensions
        unsafe {
            core::ptr::write(
                self.linear_buf.ptr.add(pixel_offset) as *mut ColorFormat::RawType,
                out_color.to_raw(),
            )
        };
    }
}

impl<ColorFormat: CanvasColorFormat> Drop for BufferedCanvas<'_, ColorFormat> {
    fn drop(&mut self) {
        self.convert_buffers();
        arm::cache_flush(self.base_pointer as *mut u8, self.buffer_size);
        let _ = self.manager.surface.queue_buffer(self.slot, self.fences);
        let _ = self.manager.surface.wait_buffer_event(-1);
    }
}

/// A Canvas where draws are directly written to the backing framebuffer memory.
///
/// Users of this object are expected to write to the screen efficiently to manage memory controller pressure,
/// or to accept lower performance for lower memory usage.
pub struct UnbufferedCanvas<'fb, ColorFormat: sealed::CanvasColorFormat> {
    slot: i32,
    base_pointer: usize,
    buffer_size: usize,
    fences: MultiFence,
    manager: &'fb mut CanvasManager<ColorFormat>,
}

impl<ColorFormat: sealed::CanvasColorFormat> UnbufferedCanvas<'_, ColorFormat> {
    /// Gets a mutable reference to the raw framebuffer
    pub fn raw_buffer(&mut self) -> &mut [ColorFormat::RawType] {
        unsafe {
            core::slice::from_raw_parts_mut(
                core::ptr::with_exposed_provenance_mut(self.base_pointer),
                self.buffer_size / core::mem::size_of::<ColorFormat::RawType>(),
            )
        }
    }

    pub fn draw_single(&mut self, x: i32, y: i32, color: ColorFormat, blend: AlphaBlend) {
        if !(0..self.manager.surface.width() as i32).contains(&x)
            || !(0..self.manager.surface.height() as i32).contains(&y)
        {
            return;
        }

        let pixel_offset = self.xy_to_gob_pixel_offset(x as u32, y as u32);
        debug_assert!(
            pixel_offset * (ColorFormat::COLOR_FORMAT.bytes_per_pixel() as usize)
                < self.buffer_size,
            "pixel offset is outside the buffer"
        );

        let pixel_pointer =
            unsafe { (self.base_pointer as *mut ColorFormat::RawType).add(pixel_offset) };

        // SAFETY: These reads/writes as:
        // 1 - we allocated it when creating the surface so the base pointer is not null and pixel-aligned
        // 2 - we only allow 2-byte and 4-byte color formats, so we will never have a pixel that spans the
        // over the edge of a bit-swizzling boundary of the block linear format's GOBs.
        unsafe {
            let old_pixel = core::ptr::read(pixel_pointer);
            core::ptr::write(
                pixel_pointer,
                color
                    .blend_with(ColorFormat::from_raw(old_pixel), blend)
                    .to_raw(),
            );
        }
    }

    /// Calculate the actual offset of the pixel in the swizzled framebuffer from the x/y co-ordinates.
    fn xy_to_gob_pixel_offset(&self, x: u32, y: u32) -> usize {
        let block_config = self.manager.surface.get_block_linear_config();
        let block_height = block_config.block_height();

        let mut gob_offset =
            (y / (8 * block_height)) * 512 * block_height * (self.manager.surface.pitch() / 64)
                + (x * ColorFormat::COLOR_FORMAT.bytes_per_pixel() / 64) * 512 * block_height
                + (y % (8 * block_height) / 8) * 512;

        //  Figure 46 in the TRM is in pixel space, even though each GOB in a block is sequential in memory.
        // This means that each 64bytes of a row (y1 == y2) are separated by `64 x 8 x block_height` bytes,
        // so the `x` value  in figure 47 will be (x * bytes_per_pixel)%64, and `y` will be (y%8)
        let x = (x * ColorFormat::COLOR_FORMAT.bytes_per_pixel()) % 64;
        let y = y % 8;
        gob_offset += ((x % 64) / 32) * 256
            + ((y % 8) / 2) * 64
            + ((x % 32) / 16) * 32
            + (y % 2) * 16
            + (x % 16);

        (gob_offset / ColorFormat::COLOR_FORMAT.bytes_per_pixel()) as usize
    }
}

impl<ColorFormat: sealed::CanvasColorFormat> Canvas for UnbufferedCanvas<'_, ColorFormat> {
    type ColorFormat = ColorFormat;

    fn draw_single(&mut self, x: i32, y: i32, color: Self::ColorFormat, blend: AlphaBlend) {
        self.draw_single(x, y, color, blend);
    }

    fn clear(&mut self, color: Self::ColorFormat) {
        let raw_color: <ColorFormat as CanvasColorFormat>::RawType = color.to_raw();
        let raw_buffer: &mut [<ColorFormat as CanvasColorFormat>::RawType] = unsafe {
            core::slice::from_raw_parts_mut(
                self.base_pointer as *mut <ColorFormat as CanvasColorFormat>::RawType,
                (self.manager.surface.single_buffer_size()
                    / <ColorFormat as CanvasColorFormat>::BYTES_PER_PIXEL) as usize,
            )
        };

        let _: () = raw_buffer
            .iter_mut()
            .map(|pixel_slot| *pixel_slot = raw_color)
            .collect();
    }

    fn width(&self) -> u32 {
        self.manager.surface.width()
    }
    fn height(&self) -> u32 {
        self.manager.surface.height()
    }
}

impl<ColorFormat: CanvasColorFormat> Drop for UnbufferedCanvas<'_, ColorFormat> {
    fn drop(&mut self) {
        arm::cache_flush(self.base_pointer as *mut u8, self.buffer_size);
        let _ = self.manager.surface.queue_buffer(self.slot, self.fences);
        let _ = self.manager.surface.wait_buffer_event(-1);
    }
}
