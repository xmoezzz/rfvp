use std::{
    marker::PhantomData,
    sync::{Arc, Mutex, RwLock},
};

use slotmap::SlotMap;
use wgpu::{util::DeviceExt as _, BufferAddress, BufferSize};

use crate::Vertex;

type BufferKey = slotmap::DefaultKey;

// Used to simplify lifetimes
// maybe use a generational arena or smth
// the engine only drops graphics resources after the frame is done, we should do the same

struct BuffersScheduledForDeletion {
    buffers: Vec<BufferKey>,
}

impl BuffersScheduledForDeletion {
    fn insert(&mut self, buffer: BufferKey) {
        self.buffers.push(buffer);
    }

    fn take(&mut self) -> Vec<BufferKey> {
        std::mem::take(&mut self.buffers)
    }
}

struct BufferManager {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    buffers: RwLock<SlotMap<BufferKey, wgpu::Buffer>>,
    deletion: Arc<Mutex<BuffersScheduledForDeletion>>,
}

enum BufferUsage {
    /// COPY_DST | INDEX | VERTEX | UNIFORM
    DynamicBuffer,
}

impl From<BufferUsage> for wgpu::BufferUsages {
    fn from(value: BufferUsage) -> Self {
        match value {
            BufferUsage::DynamicBuffer => {
                wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::INDEX
                    | wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::UNIFORM
            }
        }
    }
}

impl BufferManager {
    pub fn allocate_raw(
        &self,
        size: u64,
        usage: BufferUsage,
        label: Option<&str>,
    ) -> OwnedBuffer<RawMarker> {
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size,
            usage: usage.into(),
            mapped_at_creation: false,
        });

        let key = self.buffers.write().unwrap().insert(buffer);

        OwnedBuffer {
            ownership: Owned(self.deletion.clone()),
            key,
            offset: 0,
            size,
            phantom: PhantomData,
        }
    }

    pub fn allocate_raw_with_contents(
        &self,
        usage: BufferUsage,
        contents: &[u8],
        label: Option<&str>,
    ) -> OwnedBuffer<RawMarker> {
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label,
                usage: usage.into(),
                contents,
            });

        let size = buffer.size();

        let key = self.buffers.write().unwrap().insert(buffer);

        OwnedBuffer {
            ownership: Owned(self.deletion.clone()),
            key,
            offset: 0,
            size,
            phantom: PhantomData,
        }
    }

    pub fn apply_pending_deletions(&self) {
        let mut buffers = self.buffers.write().unwrap();
        let mut deletion = self.deletion.lock().unwrap();
        for key in deletion.take() {
            buffers.remove(key);
        }
    }
}

trait BufferType {
    fn is_valid_offset(_offset: BufferAddress) -> bool {
        true
    }

    fn is_valid_size(_size: BufferAddress) -> bool {
        true
    }
}

pub struct RawMarker;
pub struct VertexMarker<T: Vertex>(PhantomData<T>);
pub struct IndexMarker;

impl BufferType for RawMarker {}
impl<T: Vertex> BufferType for VertexMarker<T> {}
impl BufferType for IndexMarker {}

trait BufferOwnership {}

pub struct Owned(Arc<Mutex<BuffersScheduledForDeletion>>);
pub struct Borrowed;

impl BufferOwnership for Owned {}
impl BufferOwnership for Borrowed {}

struct Buffer<O: BufferOwnership, T: BufferType> {
    ownership: O,
    key: BufferKey,
    offset: BufferAddress,
    size: BufferAddress,
    phantom: PhantomData<T>,
}

type OwnedBuffer<T> = Buffer<Owned, T>;
type BufferSliceReference<T> = Buffer<Borrowed, T>;

pub type VertexBufferSliceReference<T> = Buffer<Borrowed, VertexMarker<T>>;

// TODO: fix the "drop cannot be specialized"
// impl<T: BufferType> Drop for OwnedBuffer<T> {
//     // dropping a buffer actually defers the deletion to the end of the frame
//     fn drop(&mut self) {
//         self.ownership.0.lock().unwrap().insert(self.key);
//     }
// }

impl<O: BufferOwnership> Buffer<O, RawMarker> {
    pub fn downcast<T: BufferType>(self) -> Buffer<O, T> {
        assert!(T::is_valid_offset(self.offset));
        assert!(T::is_valid_size(self.size));
        Buffer {
            ownership: self.ownership,
            key: self.key,
            offset: self.offset,
            size: self.size,
            phantom: Default::default(),
        }
    }
}

/// Dynamically allocates space in a gpu buffer, mostly used for submitting uniform data
pub struct DynamicBuffer {
    buffer_manager: Arc<BufferManager>,
    queue: Arc<wgpu::Queue>,
    chunk_size: BufferSize,
    position: BufferAddress,
    buffer: OwnedBuffer<RawMarker>,
}

impl DynamicBuffer {
    pub fn new(
        buffer_manager: Arc<BufferManager>,
        queue: Arc<wgpu::Queue>,
        chunk_size: BufferSize,
    ) -> Self {
        let buffer = buffer_manager.allocate_raw(
            chunk_size.get(),
            BufferUsage::DynamicBuffer,
            Some("DynamicBuffer"),
        );

        Self {
            buffer_manager,
            queue,
            chunk_size,
            position: 0,
            buffer,
        }
    }

    /// Create a dynamic buffer region with the specified contents. The buffer region will live until the end of the frame, after which no guarantees are made about the contents.
    pub fn get_with_data(&mut self, data: &[u8]) -> BufferSliceReference<RawMarker> {
        // let buffers = self.buffer_manager

        let offset = self.position;
        let size = data.len() as BufferAddress;

        // self.queue.write_buffer();

        let new_offset = wgpu::util::align_to(offset + size, wgpu::COPY_BUFFER_ALIGNMENT);

        todo!()

        // wgpu::COPY_BUFFER_ALIGNMENT

        // self.position += size;
        //
        // BufferSliceReference {
        //     ownership: Borrowed,
        //     key: buffer.key,
        //     offset,
        //     size,
        //     phantom: PhantomData,
        // }
    }
}
