use std::ops::Range;

use wgpu::{util::BufferInitDescriptor, PrimitiveTopology};

use crate::{
    subsystem::components::material::Material,
    rendering::{gl_representations::TexturedGlVertex, Renderable2D},
};

const INDICES: &[u16] = &[0, 1, 3, 3, 1, 2];

/// Renderable Sprite.
#[derive(Debug)]
pub struct Sprite {
    /// Desired tile to render for this material.
    tile_number: usize,
    /// Current computed content for vertex
    contents: Option<[TexturedGlVertex; 4]>,
    /// Flag to keep track of changed tile number
    dirty: bool,
}

impl Sprite {
    /// Creates a new sprite that will use the `tile_number` from the tileset associated in the same
    /// entity
    pub fn new(tile_number: usize) -> Self {
        Self { tile_number, contents: None, dirty: false }
    }

    /// Modify the current sprite tile number
    pub fn set_tile_nb(&mut self, new_tile_nb: usize) {
        self.tile_number = new_tile_nb;
        self.dirty = true;
    }

    pub fn get_tile_nb(&self) -> usize {
        self.tile_number
    }

    pub(crate) fn compute_content(&self, material: Option<&Material>) -> [TexturedGlVertex; 4] {
        if (self.dirty || self.contents.is_none()) && material.is_some() {
        }
        self.contents.as_ref().expect("A computed content is missing in Sprite component").clone()
    }

    pub(crate) fn indices() -> Vec<u16> {
        INDICES.to_vec()
    }

    pub(crate) fn set_content(&mut self, content: [TexturedGlVertex; 4]) {
        self.contents = Some(content);
    }
}

impl Renderable2D for Sprite {
    fn vertex_buffer_descriptor(&mut self, material: Option<&Material>) -> BufferInitDescriptor {
        let content = self.compute_content(material);
        self.contents = Some(content);
        BufferInitDescriptor {
            label: Some("Sprite Vertex Buffer"),
            contents: bytemuck::cast_slice(
                self.contents.as_ref().expect("A computed content is missing in Sprite component"),
            ),
            usage: wgpu::BufferUsages::VERTEX,
        }
    }

    fn indexes_buffer_descriptor(&self) -> BufferInitDescriptor {
        BufferInitDescriptor {
            label: Some("Sprite Index Buffer"),
            contents: bytemuck::cast_slice(&INDICES),
            usage: wgpu::BufferUsages::INDEX,
        }
    }

    fn range(&self) -> Range<u32> {
        0..INDICES.len() as u32
    }

    fn topology() -> PrimitiveTopology {
        wgpu::PrimitiveTopology::TriangleList
    }

    fn dirty(&self) -> bool {
        self.dirty
    }

    fn set_dirty(&mut self, is_dirty: bool) {
        self.dirty = is_dirty;
    }
}
