use std::ops::Range;

use glam::Mat4;

use crate::rfvp_render::vertices::{PosColTexVertex, VertexSource};
use crate::rfvp_render::{BindGroupLayouts, TextureBindGroup};

/// Additive blending as used by the original engine for blend=1 prims:
/// SRCBLEND = SRCALPHA, DESTBLEND = ONE.
fn additive_blend_state() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        // Additive layers must not change the destination coverage.
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

pub struct SpritePipeline {
    pipeline: wgpu::RenderPipeline,
    pipeline_additive: wgpu::RenderPipeline,
}

impl SpritePipeline {
    pub fn new(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rfvp_render.sprite_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rfvp_render.sprite_pipeline_layout"),
            bind_group_layouts: &[&layouts.texture],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..64,
            }],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PosColTexVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 28,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        };

        let make_pipeline = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("rfvp_render.sprite_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[vertex_layout.clone()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            })
        };
        let pipeline = make_pipeline(wgpu::BlendState::ALPHA_BLENDING);
        let pipeline_additive = make_pipeline(additive_blend_state());

        Self {
            pipeline,
            pipeline_additive,
        }
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        src: VertexSource<'a>,
        tex: &'a TextureBindGroup,
        transform: Mat4,
    ) {
        self.draw_blended(pass, src, tex, transform, false);
    }

    /// Draw with either normal alpha blending or additive blending (`PrimSetBlend(id, 1)`).
    pub fn draw_blended<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        src: VertexSource<'a>,
        tex: &'a TextureBindGroup,
        transform: Mat4,
        additive: bool,
    ) {
        pass.set_pipeline(if additive {
            &self.pipeline_additive
        } else {
            &self.pipeline
        });
        pass.set_bind_group(0, tex.raw(), &[]);
        let m = transform.to_cols_array();
        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&m));

        match src {
            VertexSource::VertexBuffer {
                vertex_buffer,
                vertices,
                instances,
            } => {
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(vertices, instances);
            }
            VertexSource::VertexIndexBuffer {
                vertex_buffer,
                index_buffer,
                indices,
                instances,
            } => {
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(indices, 0, instances);
            }
        }
    }
}
