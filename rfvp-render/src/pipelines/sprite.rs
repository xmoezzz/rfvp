use std::mem;

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::include_wgsl;

use crate::{
    pipelines,
    vertices::{PosColTexVertex, VertexSource},
    BindGroupLayouts, TextureBindGroup,
};

#[derive(Pod, Zeroable, Copy, Clone, Debug)]
#[repr(C)]
struct SpriteParams {
    pub transform: Mat4,
}

pub struct SpritePipeline(wgpu::RenderPipeline);

impl SpritePipeline {
    pub fn new(
        device: &wgpu::Device,
        bind_group_layouts: &BindGroupLayouts,
        texture_format: wgpu::TextureFormat,
    ) -> Self {
        let shader_module = device.create_shader_module(include_wgsl!("sprite.wgsl"));

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SpritePipeline Layout"),
            bind_group_layouts: &[&bind_group_layouts.texture],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                range: 0..(mem::size_of::<SpriteParams>() as u32),
            }],
        });

        Self(pipelines::make_pipeline(
            device,
            texture_format,
            shader_module,
            layout,
            PosColTexVertex::desc(),
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            "SpritePipeline",
        ))
    }

    pub fn draw<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        source: VertexSource<'a, PosColTexVertex>,
        texture: &'a TextureBindGroup,
        transform: Mat4,
    ) {
        render_pass.set_pipeline(&self.0);
        render_pass.set_bind_group(0, &texture.0, &[]);
        render_pass.set_push_constants(
            wgpu::ShaderStages::VERTEX_FRAGMENT,
            0,
            bytemuck::cast_slice(&[SpriteParams { transform }]),
        );
        source.draw(render_pass);
    }
}
