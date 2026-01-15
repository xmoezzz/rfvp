use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3, Vec4};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PosVertex {
    pub position: Vec3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PosColTexVertex {
    pub position: Vec3,
    pub color: Vec4,
    pub texture_coordinate: Vec2,
}

pub enum VertexSource<'a> {
    VertexBuffer {
        vertex_buffer: &'a wgpu::Buffer,
        vertices: Range<u32>,
        instances: Range<u32>,
    },
    VertexIndexBuffer {
        vertex_buffer: &'a wgpu::Buffer,
        index_buffer: &'a wgpu::Buffer,
        indices: Range<u32>,
        instances: Range<u32>,
    },
}
