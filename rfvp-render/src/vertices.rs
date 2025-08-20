use std::ops::Range;

use glam::{Vec2, Vec3, Vec4};
use rfvp_core::time::Ticks;

#[derive(Copy, Clone, Debug, wrld::Desc, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct PosColTexVertex {
    #[f32x3(0)]
    pub position: Vec3,
    #[f32x4(1)]
    pub color: Vec4,
    #[f32x2(2)]
    pub texture_coordinate: Vec2,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, wrld::Desc, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PosVertex {
    #[f32x3(0)]
    pub position: Vec3,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, wrld::Desc, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextVertex {
    #[f32x2(0)]
    pub position: Vec2,
    #[f32x2(1)]
    pub tex_position: Vec2,
    #[f32x3(2)]
    pub color: Vec3,
    #[f32(3)] // TODO(ticks): don't forget to change into u32
    pub time: Ticks,
    #[f32(4)]
    pub fade: f32,
}

pub enum VertexSource<'a, T> {
    VertexBuffer {
        vertex_buffer: &'a wgpu::Buffer, // TODO: support multiple vertex buffers
        vertices: Range<u32>,
        instances: Range<u32>,
        phantom: std::marker::PhantomData<T>,
    },
    VertexIndexBuffer {
        vertex_buffer: &'a wgpu::Buffer,
        index_buffer: &'a wgpu::Buffer,
        indices: Range<u32>,
        instances: Range<u32>,
    },
}

impl<'a, T> VertexSource<'a, T> {
    #[allow(unused)]
    pub fn vertex_count(&self) -> u32 {
        match self {
            VertexSource::VertexBuffer { vertices, .. } => vertices.end - vertices.start,
            VertexSource::VertexIndexBuffer { indices, .. } => indices.end - indices.start,
        }
    }

    pub fn vertex_buffer(&self) -> &'a wgpu::Buffer {
        match self {
            VertexSource::VertexBuffer { vertex_buffer, .. } => vertex_buffer,
            VertexSource::VertexIndexBuffer { vertex_buffer, .. } => vertex_buffer,
        }
    }

    pub fn instances(&self) -> Range<u32> {
        match self {
            VertexSource::VertexBuffer { instances, .. } => instances.clone(),
            VertexSource::VertexIndexBuffer { instances, .. } => instances.clone(),
        }
    }

    pub fn with_index_buffer(self, index_buffer: &'a wgpu::Buffer, indices: Range<u32>) -> Self {
        VertexSource::VertexIndexBuffer {
            vertex_buffer: self.vertex_buffer(),
            index_buffer,
            indices,
            instances: self.instances(),
        }
    }
}

impl<'a, T> VertexSource<'a, T> {
    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'a>) {
        match self {
            VertexSource::VertexBuffer {
                vertex_buffer,
                vertices,
                instances,
                phantom: _,
            } => {
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.draw(vertices.clone(), instances.clone());
            }
            VertexSource::VertexIndexBuffer {
                vertex_buffer,
                index_buffer,
                indices,
                instances,
            } => {
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(indices.clone(), 0, instances.clone());
            }
        }
    }
}
