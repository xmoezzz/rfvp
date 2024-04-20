use std::mem;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2};
use rfvp_core::time::Ticks;
use wgpu::include_wgsl;

use crate::{
    pipelines,
    vertices::{TextVertex, VertexSource},
    BindGroupLayouts, TextureBindGroup,
};

#[derive(Pod, Zeroable, Copy, Clone, Debug)]
#[repr(C)]
struct TextOutlineParams {
    pub transform: Mat4,
    pub time: Ticks,
    pub distance: Vec2,
}

pub struct TextOutlinePipeline(wgpu::RenderPipeline);

impl TextOutlinePipeline {
    pub fn new(
        device: &wgpu::Device,
        bind_group_layouts: &BindGroupLayouts,
        texture_format: wgpu::TextureFormat,
    ) -> Self {
        let shader_module = device.create_shader_module(include_wgsl!("text_outline.wgsl"));

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("TextOutlinePipeline Layout"),
            bind_group_layouts: &[&bind_group_layouts.texture],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                range: 0..mem::size_of::<TextOutlineParams>() as u32,
            }],
        });

        let desc = TextVertex::desc();

        Self(pipelines::make_pipeline(
            device,
            texture_format,
            shader_module,
            layout,
            desc,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            "TextOutlinePipeline",
        ))
    }

    pub fn draw<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        source: VertexSource<'a, TextVertex>,
        texture: &'a TextureBindGroup,
        transform: Mat4,
        time: Ticks,
        distance: Vec2,
    ) {
        render_pass.set_pipeline(&self.0);
        render_pass.set_bind_group(0, &texture.0, &[]);
        render_pass.set_push_constants(
            wgpu::ShaderStages::VERTEX_FRAGMENT,
            0,
            bytemuck::cast_slice(&[TextOutlineParams {
                transform,
                time,
                distance,
            }]),
        );
        source.draw(render_pass);
    }
}
