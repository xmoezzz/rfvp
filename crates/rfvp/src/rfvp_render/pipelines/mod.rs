#[cfg(not(target_arch = "wasm32"))]
pub mod sprite;
#[cfg(target_arch = "wasm32")]
#[path = "sprite_wasm.rs"]
pub mod sprite;

#[cfg(not(target_arch = "wasm32"))]
pub mod fill;
#[cfg(target_arch = "wasm32")]
#[path = "fill_wasm.rs"]
pub mod fill;

use std::sync::Arc;

use super::BindGroupLayouts;

pub struct Pipelines {
    pub sprite: sprite::SpritePipeline,
    pub sprite_screen: sprite::SpritePipeline,
    pub fill: fill::FillPipeline,
}

impl Pipelines {
    pub fn new(
        device: &wgpu::Device,
        queue: &Arc<wgpu::Queue>,
        layouts: &BindGroupLayouts,
        swapchain_format: wgpu::TextureFormat,
    ) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let sprite = sprite::SpritePipeline::new(device, layouts, crate::rfvp_render::RenderTarget::FORMAT);
        #[cfg(target_arch = "wasm32")]
        let sprite = sprite::SpritePipeline::new(device, queue, layouts, crate::rfvp_render::RenderTarget::FORMAT);

        #[cfg(not(target_arch = "wasm32"))]
        let sprite_screen = sprite::SpritePipeline::new(device, layouts, swapchain_format);
        #[cfg(target_arch = "wasm32")]
        let sprite_screen = sprite::SpritePipeline::new(device, queue, layouts, swapchain_format);

        #[cfg(not(target_arch = "wasm32"))]
        let fill = fill::FillPipeline::new(device, crate::rfvp_render::RenderTarget::FORMAT);
        #[cfg(target_arch = "wasm32")]
        let fill = fill::FillPipeline::new(device, queue, crate::rfvp_render::RenderTarget::FORMAT);
        Self { sprite, sprite_screen, fill }
    }
}
