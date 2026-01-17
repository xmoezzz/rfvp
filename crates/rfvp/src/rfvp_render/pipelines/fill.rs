use glam::{Mat4, Vec4};

use crate::rfvp_render::vertices::{PosVertex, VertexSource};

pub struct FillPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl FillPipeline {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rfvp_render.fill_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fill.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rfvp_render.fill_pipeline_layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                range: 0..80,
            }],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PosVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rfvp_render.fill_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
        });

        Self { pipeline }
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        src: VertexSource<'a>,
        transform: Mat4,
        color: Vec4,
    ) {
        pass.set_pipeline(&self.pipeline);

        let mut pc = [0u8; 80];
        let m = transform.to_cols_array();
        pc[0..64].copy_from_slice(bytemuck::bytes_of(&m));
        let c = color.to_array();
        pc[64..80].copy_from_slice(bytemuck::bytes_of(&c));
        pass.set_push_constants(wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT, 0, &pc);

        match src {
            VertexSource::VertexBuffer { vertex_buffer, vertices, instances } => {
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(vertices, instances);
            }
            VertexSource::VertexIndexBuffer { vertex_buffer, index_buffer, indices, instances } => {
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(indices, 0, instances);
            }
        }
    }
}
