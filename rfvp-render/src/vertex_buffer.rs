use std::sync::atomic::{AtomicU32, Ordering};

use glam::{vec2, vec3, vec4, Vec4};
use wgpu::util::DeviceExt;

use crate::{
    vertices::{PosColTexVertex, PosVertex, TextVertex, VertexSource},
    GpuCommonResources, VIRTUAL_HEIGHT, VIRTUAL_WIDTH,
};

pub trait Vertex: bytemuck::Pod + bytemuck::Zeroable {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a>;
}

// TODO: derive this
impl Vertex for PosColTexVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        PosColTexVertex::desc()
    }
}
impl Vertex for PosVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        PosVertex::desc()
    }
}
impl Vertex for TextVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        TextVertex::desc()
    }
}

pub struct VertexBuffer<T: Vertex> {
    buffer: wgpu::Buffer,
    num_vertices: AtomicU32,
    capacity_vertices: u32,
    phantom: std::marker::PhantomData<T>,
}

impl<T: Vertex> VertexBuffer<T> {
    pub fn new(resources: &GpuCommonResources, vertices: &[T], label: Option<&str>) -> Self {
        let buffer = resources
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label,
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let num_vertices = vertices.len() as u32;
        Self {
            buffer,
            num_vertices: num_vertices.into(),
            capacity_vertices: num_vertices,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn new_updatable(
        resources: &GpuCommonResources,
        capacity_vertices: u32,
        label: Option<&str>,
    ) -> Self {
        let buffer = resources.device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size: (capacity_vertices * std::mem::size_of::<T>() as u32) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            num_vertices: 0.into(),
            capacity_vertices,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn write(&self, queue: &wgpu::Queue, vertices: &[T]) {
        assert!(vertices.len() as u32 <= self.capacity_vertices);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(vertices));
        self.num_vertices
            .store(vertices.len() as u32, Ordering::SeqCst);
    }

    pub fn vertex_source(&self) -> VertexSource<T> {
        VertexSource::VertexBuffer {
            vertex_buffer: &self.buffer,
            vertices: 0..self.num_vertices.load(Ordering::SeqCst),
            instances: 0..1,
            phantom: std::marker::PhantomData,
        }
    }

    // pub fn vertex_source_slice(&self, range: std::ops::Range<u32>) -> VertexSource<T> {
    //     assert!(range.end <= self.num_vertices.load(Ordering::SeqCst));
    //
    //     VertexSource::VertexBuffer {
    //         vertex_buffer: &self.buffer,
    //         vertices: range,
    //         instances: 0..1,
    //         phantom: std::marker::PhantomData,
    //     }
    // }
}

pub struct IndexBuffer {
    buffer: wgpu::Buffer,
    num_indices: u32,
}

impl IndexBuffer {
    pub fn new(resources: &GpuCommonResources, indices: &[u16], label: Option<&str>) -> Self {
        let buffer = resources
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label,
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let num_indices = indices.len() as u32;
        Self {
            buffer,
            num_indices,
        }
    }

    #[allow(unused)]
    pub fn num_indices(&self) -> u32 {
        self.num_indices
    }
}

pub struct SpriteVertexBuffer {
    vertex_buffer: VertexBuffer<PosColTexVertex>,
    index_buffer: IndexBuffer,
}

impl SpriteVertexBuffer {
    pub fn new(
        resources: &GpuCommonResources,
        (l, t, r, b): (f32, f32, f32, f32),
        color: Vec4,
    ) -> Self {
        let vertices = [
            // 0
            PosColTexVertex {
                position: vec3(l, b, 0.0),
                color,
                texture_coordinate: vec2(0.0, 1.0),
            },
            // 1
            PosColTexVertex {
                position: vec3(l, t, 0.0),
                color,
                texture_coordinate: vec2(0.0, 0.0),
            },
            // 2
            PosColTexVertex {
                position: vec3(r, b, 0.0),
                color,
                texture_coordinate: vec2(1.0, 1.0),
            },
            // 3
            PosColTexVertex {
                position: vec3(r, t, 0.0),
                color,
                texture_coordinate: vec2(1.0, 0.0),
            },
        ];

        let indices = [0, 1, 2, 2, 1, 3];

        Self {
            vertex_buffer: VertexBuffer::new(
                resources,
                &vertices,
                Some(&format!("SpriteVertexBuffer({}, {}, {}, {})", l, t, r, b)),
            ),
            index_buffer: IndexBuffer::new(
                resources,
                &indices,
                Some("SpriteVertexBuffer.index_buffer"),
            ),
        }
    }

    pub fn new_fullscreen(resources: &GpuCommonResources) -> Self {
        let w = VIRTUAL_WIDTH / 2.0;
        let h = VIRTUAL_HEIGHT / 2.0;

        Self::new(resources, (-w, -h, w, h), vec4(1.0, 1.0, 1.0, 1.0))
    }

    pub fn vertex_source(&self) -> VertexSource<PosColTexVertex> {
        self.vertex_buffer
            .vertex_source()
            .with_index_buffer(&self.index_buffer.buffer, 0..self.index_buffer.num_indices)
    }
}

pub struct PosVertexBuffer {
    vertex_buffer: VertexBuffer<PosVertex>,
    index_buffer: IndexBuffer,
}

impl PosVertexBuffer {
    pub fn new(resources: &GpuCommonResources, (l, t, r, b): (f32, f32, f32, f32)) -> Self {
        let vertices = [
            // 0
            PosVertex {
                position: vec3(l, b, 0.0),
            },
            // 1
            PosVertex {
                position: vec3(l, t, 0.0),
            },
            // 2
            PosVertex {
                position: vec3(r, b, 0.0),
            },
            // 3
            PosVertex {
                position: vec3(r, t, 0.0),
            },
        ];

        let indices = [0, 1, 2, 2, 1, 3];

        Self {
            vertex_buffer: VertexBuffer::new(
                resources,
                &vertices,
                Some(&format!("PosVertexBuffer({}, {}, {}, {})", l, t, r, b)),
            ),
            index_buffer: IndexBuffer::new(
                resources,
                &indices,
                Some("PosVertexBuffer.index_buffer"),
            ),
        }
    }

    #[allow(unused)]
    pub fn new_fullscreen(resources: &GpuCommonResources) -> Self {
        let w = VIRTUAL_WIDTH / 2.0;
        let h = VIRTUAL_HEIGHT / 2.0;

        Self::new(resources, (-w, -h, w, h))
    }

    pub fn vertex_source(&self) -> VertexSource<PosVertex> {
        self.vertex_buffer
            .vertex_source()
            .with_index_buffer(&self.index_buffer.buffer, 0..self.index_buffer.num_indices)
    }
}
