use std::error::Error;
use std::fmt;

use super::PixelFormat;

/// Errors returned by software framebuffer operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoftRenderError {
    /// The requested dimensions cannot be represented by the backing buffer.
    DimensionsTooLarge { width: u32, height: u32 },
}

impl fmt::Display for SoftRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionsTooLarge { width, height } => {
                write!(
                    f,
                    "software framebuffer dimensions are too large: {width}x{height}"
                )
            }
        }
    }
}

impl Error for SoftRenderError {}

/// Owned platform-independent software framebuffer.
///
/// The buffer is tightly packed by default. `stride` is measured in bytes and
/// represents the number of bytes between adjacent rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftFramebuffer {
    width: u32,
    height: u32,
    stride: usize,
    format: PixelFormat,
    pixels: Vec<u8>,
}

impl SoftFramebuffer {
    /// Create a new cleared framebuffer.
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Result<Self, SoftRenderError> {
        let (stride, len) = layout(width, height, format)?;

        Ok(Self {
            width,
            height,
            stride,
            format,
            pixels: vec![0; len],
        })
    }

    /// Resize the framebuffer and clear the backing buffer to transparent black.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), SoftRenderError> {
        let (stride, len) = layout(width, height, self.format)?;

        self.width = width;
        self.height = height;
        self.stride = stride;
        self.pixels.resize(len, 0);
        self.pixels.fill(0);

        Ok(())
    }

    /// Width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Bytes between the start of adjacent rows.
    pub const fn stride(&self) -> usize {
        self.stride
    }

    /// Pixel format used by the backing buffer.
    pub const fn format(&self) -> PixelFormat {
        self.format
    }

    /// Immutable view of the framebuffer bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Mutable view of the framebuffer bytes.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Fill the framebuffer with a single RGBA color.
    pub fn clear_rgba(&mut self, r: u8, g: u8, b: u8, a: u8) {
        let pixel = match self.format {
            PixelFormat::Rgba8 => [r, g, b, a],
            PixelFormat::Bgra8 => [b, g, r, a],
        };

        for chunk in self.pixels.chunks_exact_mut(self.format.bytes_per_pixel()) {
            chunk.copy_from_slice(&pixel);
        }
    }
}

fn layout(width: u32, height: u32, format: PixelFormat) -> Result<(usize, usize), SoftRenderError> {
    let bytes_per_pixel = format.bytes_per_pixel();
    let width = width as usize;
    let height = height as usize;
    let stride = width
        .checked_mul(bytes_per_pixel)
        .ok_or(SoftRenderError::DimensionsTooLarge {
            width: width as u32,
            height: height as u32,
        })?;
    let len = stride
        .checked_mul(height)
        .ok_or(SoftRenderError::DimensionsTooLarge {
            width: width as u32,
            height: height as u32,
        })?;

    Ok((stride, len))
}
