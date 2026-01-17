pub mod vertices;
pub mod vertex_buffer;
pub mod texture;
pub mod render_target;
pub mod pipelines;

pub use vertices::*;
pub use vertex_buffer::VertexBuffer;
pub use texture::{BindGroupLayouts, GpuTexture, TextureBindGroup};
pub use render_target::RenderTarget;
pub use pipelines::Pipelines;

use std::sync::RwLock;

pub struct GpuCommonResources {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub render_buffer_size: RwLock<(u32, u32)>,
    pub bind_group_layouts: BindGroupLayouts,
    pub pipelines: Pipelines,
}
