//! Platform-independent CPU/software rendering foundations.

mod framebuffer;
mod pixel;
mod renderer;

pub use framebuffer::{SoftFramebuffer, SoftRenderError};
pub use pixel::PixelFormat;
pub use renderer::{SoftRenderer, SoftRendererStats};

/// Construct the software renderer explicitly.
///
/// This does not participate in default renderer selection and does not touch
/// the existing GPU renderer.
pub fn create_soft_renderer(
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<SoftRenderer, SoftRenderError> {
    SoftRenderer::new(width, height, format)
}
