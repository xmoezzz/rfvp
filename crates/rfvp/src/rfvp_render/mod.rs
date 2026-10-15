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

/// GPU memory held by textures and render targets, for `RFVP_MEM_REPORT` (see `App`).
pub mod mem_stats {
    use std::sync::atomic::{AtomicU64, Ordering};

    pub static TEXTURE_BYTES: AtomicU64 = AtomicU64::new(0);
    pub static TEXTURES: AtomicU64 = AtomicU64::new(0);
    pub static TARGET_BYTES: AtomicU64 = AtomicU64::new(0);
    pub static TARGETS: AtomicU64 = AtomicU64::new(0);
    /// Totals since start: textures made, and bytes written into existing ones.
    pub static TEXTURES_MADE: AtomicU64 = AtomicU64::new(0);
    pub static TEXTURE_WRITES: AtomicU64 = AtomicU64::new(0);

    pub fn add(bytes: &AtomicU64, count: &AtomicU64, size: u64) {
        bytes.fetch_add(size, Ordering::Relaxed);
        count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn sub(bytes: &AtomicU64, count: &AtomicU64, size: u64) {
        bytes.fetch_sub(size, Ordering::Relaxed);
        count.fetch_sub(1, Ordering::Relaxed);
    }
}
