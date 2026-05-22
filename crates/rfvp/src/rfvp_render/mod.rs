pub mod pipelines;
pub mod render_target;
pub mod texture;
pub mod vertex_buffer;
pub mod vertices;

pub use pipelines::Pipelines;
pub use render_target::RenderTarget;
pub use texture::{BindGroupLayouts, GpuTexture, TextureBindGroup};
pub use vertex_buffer::VertexBuffer;
pub use vertices::*;

use std::sync::{Arc, RwLock};

pub struct GpuCommonResources {
    pub device: wgpu::Device,
    pub queue: Arc<wgpu::Queue>,
    pub render_buffer_size: RwLock<(u32, u32)>,
    pub bind_group_layouts: BindGroupLayouts,
    pub pipelines: Pipelines,
}
