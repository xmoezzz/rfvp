use std::sync::RwLock;

use glam::{Mat4, Vec2, Vec4};
use rfvp_core::time::Ticks;

use crate::{
    pipelines::Pipelines,
    vertices::{PosColTexVertex, PosVertex, TextVertex, VertexSource},
    BindGroupLayouts, SubmittingEncoder, TextureBindGroup, YuvTextureBindGroup,
};

pub struct GpuCommonResources {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// please don't write to this, only the main window struct should write here
    /// TODO: make this private or smth
    pub render_buffer_size: RwLock<(u32, u32)>,
    pub pipelines: Pipelines,
    pub bind_group_layouts: BindGroupLayouts,
}

impl GpuCommonResources {
    pub fn start_encoder(&self) -> SubmittingEncoder {
        SubmittingEncoder {
            encoder: Some(
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("SubmittingEncoder"),
                    }),
            ),
            queue: &self.queue,
        }
    }

    pub fn draw_sprite<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        source: VertexSource<'a, PosColTexVertex>,
        texture: &'a TextureBindGroup,
        transform: Mat4,
    ) {
        self.pipelines
            .sprite
            .draw(render_pass, source, texture, transform);
    }

    pub fn draw_yuv_sprite<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        source: VertexSource<'a, PosColTexVertex>,
        texture: &'a YuvTextureBindGroup,
        transform: Mat4,
    ) {
        self.pipelines
            .yuv_sprite
            .draw(render_pass, source, texture, transform);
    }

    #[allow(unused)]
    pub fn draw_fill<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        source: VertexSource<'a, PosVertex>,
        transform: Mat4,
        color: Vec4,
    ) {
        self.pipelines
            .fill
            .draw(render_pass, source, transform, color);
    }

    pub fn draw_text<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        source: VertexSource<'a, TextVertex>,
        texture: &'a TextureBindGroup,
        transform: Mat4,
        time: Ticks,
    ) {
        self.pipelines
            .text
            .draw(render_pass, source, texture, transform, time);
    }

    pub fn draw_text_outline<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        source: VertexSource<'a, TextVertex>,
        texture: &'a TextureBindGroup,
        transform: Mat4,
        time: Ticks,
        distance: Vec2,
    ) {
        self.pipelines
            .text_outline
            .draw(render_pass, source, texture, transform, time, distance);
    }

    pub fn current_render_buffer_size(&self) -> (u32, u32) {
        *self.render_buffer_size.read().unwrap()
    }
}
