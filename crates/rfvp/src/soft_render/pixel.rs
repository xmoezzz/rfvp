/// Pixel storage format for software framebuffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// Four bytes per pixel in red, green, blue, alpha order.
    Rgba8,
    /// Four bytes per pixel in blue, green, red, alpha order.
    Bgra8,
}

impl PixelFormat {
    /// Number of bytes used by one pixel in this format.
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8 | Self::Bgra8 => 4,
        }
    }
}
