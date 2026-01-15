use std::{marker::PhantomData, ops::Range};

use bytemuck::Pod;

use super::vertices::VertexSource;
use super::GpuCommonResources;

pub struct VertexBuffer<T> {
    buffer: wgpu::Buffer,
    capacity: u32,
    _marker: PhantomData<T>,
}

impl<T: Pod> VertexBuffer<T> {
    pub fn new_updatable(resources: &GpuCommonResources, capacity: u32, label: Option<&str>) -> Self {
        let size_bytes = (capacity as usize).saturating_mul(std::mem::size_of::<T>()) as u64;
        let buffer = resources.device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size: size_bytes.max(4),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer, capacity, _marker: PhantomData }
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn write(&self, queue: &wgpu::Queue, data: &[T]) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
    }

    pub fn vertex_source_slice<'a>(&'a self, vertices: Range<u32>) -> VertexSource<'a> {
        VertexSource::VertexBuffer {
            vertex_buffer: &self.buffer,
            vertices,
            instances: 0..1,
        }
    }
}
