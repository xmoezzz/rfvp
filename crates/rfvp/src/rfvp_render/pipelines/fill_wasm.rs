use std::cell::Cell;
use std::sync::Arc;
use glam::{Mat4, Vec4};

use crate::rfvp_render::vertices::{PosVertex, VertexSource};

const PC_SIZE: u64 = 80;
const PC_STRIDE: u64 = 256;
const PC_RING_SLOTS: u64 = 4096;
const PC_RING_SIZE: u64 = PC_STRIDE * PC_RING_SLOTS;

pub struct FillPipeline {
    pipeline: wgpu::RenderPipeline,
    queue: Arc<wgpu::Queue>,
    pc_buffer: wgpu::Buffer,
    pc_bind_group: wgpu::BindGroup,
    next_pc_slot: Cell<u32>,
}

impl FillPipeline {
    pub fn new(device: &wgpu::Device, queue: &Arc<wgpu::Queue>, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rfvp_render.fill_shader.wasm"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fill_wasm.wgsl").into()),
        });

        let pc_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rfvp_render.fill_pc_bind_group_layout.wasm"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: std::num::NonZeroU64::new(80),
                },
                count: None,
            }],
        });

        let pc_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rfvp_render.fill_pc_uniform.wasm"),
            size: PC_RING_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let pc_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rfvp_render.fill_pc_bind_group.wasm"),
            layout: &pc_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &pc_buffer,
                    offset: 0,
                    size: std::num::NonZeroU64::new(80),
                }),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rfvp_render.fill_pipeline_layout.wasm"),
            bind_group_layouts: &[&pc_bind_group_layout],
            push_constant_ranges: &[],
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
            label: Some("rfvp_render.fill_pipeline.wasm"),
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

        Self {
            pipeline,
            queue: queue.clone(),
            pc_buffer,
            pc_bind_group,
            next_pc_slot: Cell::new(0),
        }
    }

    fn next_pc_offset(&self) -> u32 {
        let slot = self.next_pc_slot.get();
        self.next_pc_slot.set((slot + 1) % (PC_RING_SLOTS as u32));
        (slot as u64 * PC_STRIDE) as u32
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
        let pc_offset = self.next_pc_offset();
        self.queue.write_buffer(&self.pc_buffer, pc_offset as u64, &pc);
        pass.set_bind_group(0, &self.pc_bind_group, &[pc_offset]);

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
