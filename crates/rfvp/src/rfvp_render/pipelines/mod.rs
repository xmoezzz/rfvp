pub mod sprite;
pub mod fill;

use super::BindGroupLayouts;

pub struct Pipelines {
    pub sprite: sprite::SpritePipeline,
    pub sprite_screen: sprite::SpritePipeline,
    pub fill: fill::FillPipeline,
}

impl Pipelines {
    pub fn new(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        swapchain_format: wgpu::TextureFormat,
    ) -> Self {
        let sprite = sprite::SpritePipeline::new(device, layouts, crate::rfvp_render::RenderTarget::FORMAT);
        let sprite_screen = sprite::SpritePipeline::new(device, layouts, swapchain_format);
        let fill = fill::FillPipeline::new(device, crate::rfvp_render::RenderTarget::FORMAT);
        Self { sprite, sprite_screen, fill }
    }
}
