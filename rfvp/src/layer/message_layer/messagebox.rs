use std::sync::Arc;

use glam::{vec2, vec3, vec4, Mat4, Vec2};
use rfvp_core::vm::command::types::MessageboxType;
use rfvp_render::{
    vertices::PosColTexVertex, GpuCommonResources, LazyGpuTexture, PosVertexBuffer, Renderable,
    VertexBuffer,
};

use crate::{
    asset::texture_archive::TextureArchive,
    layer::message_layer::message::MessageMetrics,
    update::{Updatable, UpdateContext},
};

#[derive(TextureArchive)]
pub struct MessageboxTextures {
    #[txa(name = "keywait")]
    pub keywait: LazyGpuTexture,
    #[txa(name = "select")]
    pub select: LazyGpuTexture,
    #[txa(name = "select_cur")]
    pub select_cursor: LazyGpuTexture,

    #[txa(name = "msgwnd1")]
    pub message_window_1: LazyGpuTexture,
    #[txa(name = "msgwnd2")]
    pub message_window_2: LazyGpuTexture,
    #[txa(name = "msgwnd3")]
    pub message_window_3: LazyGpuTexture,
}

// struct VertexRanges {
//     header: std::ops::Range<u32>,
//     body: std::ops::Range<u32>,
// }

const MAX_VERTEX_COUNT: usize = 120;
const TEX_SIZE: Vec2 = vec2(1648.0, 288.0);

// https://stackoverflow.com/a/34324856
macro_rules! count {
    () => (0usize);
    ( $x:tt $($xs:tt)* ) => (1usize + count!($($xs)*));
}

macro_rules! make_vertices {
    ($r:expr, $([$x:expr, $y:expr, $x_tex:expr, $y_tex:expr]),*) => {
        $r.reserve(count!($($x)*));
        $(
            $r.push(PosColTexVertex {
                position: vec3($x, $y, 1.0),
                color: vec4(1.0, 1.0, 1.0, 0.85),
                texture_coordinate: vec2($x_tex / TEX_SIZE.x, $y_tex / TEX_SIZE.y),
            });
        )*
    };
}

fn build_message_header_buffer(character_name_width: f32) -> Vec<PosColTexVertex> {
    let mut result = Vec::new();

    if character_name_width == 0.0 {
        // Draw the header part without a character name box
        make_vertices!(
            result,
            [130.0, -32.0, 0.0, 144.0],
            [130.0, 80.0, 0.0, 256.0],
            [178.0, -32.0, 48.0, 144.0],
            [178.0, 80.0, 48.0, 256.0],
            [1742.0, -32.0, 64.0, 144.0],
            [1742.0, 80.0, 64.0, 256.0],
            [1790.0, -32.0, 112.0, 144.0],
            [1790.0, 80.0, 112.0, 256.0]
        );
    } else {
        // Draw the header part with a character name box
        make_vertices!(
            result,
            [130.0, -32.0, 0.0, 0.0],
            [130.0, 80.0, 0.0, 112.0],
            [178.0, -32.0, 48.0, 0.0],
            [178.0, 80.0, 48.0, 112.0],
            [178.0 + character_name_width, -32.0, 64.0, 0.0],
            [178.0 + character_name_width, 80.0, 64.0, 112.0],
            [290.0 + character_name_width, -32.0, 160.0, 0.0],
            [290.0 + character_name_width, 80.0, 160.0, 112.0],
            [1742.0, -32.0, 176.0, 0.0],
            [1742.0, 80.0, 176.0, 112.0],
            [1790.0, -32.0, 224.0, 0.0],
            [1790.0, 80.0, 224.0, 112.0]
        );
    }

    result
}

fn build_message_body_vertices(height: f32) -> Vec<PosColTexVertex> {
    let mut result = Vec::new();

    let mid = height + 32.0 - 256.0;
    let high = height + 32.0;

    make_vertices!(
        result,
        [130.0, 80.0, 240.0, 16.0],
        [130.0, mid, 240.0, 32.0],
        [178.0, 80.0, 288.0, 16.0],
        [178.0, mid, 288.0, 32.0],
        [446.0, 80.0, 304.0, 16.0],
        [446.0, mid, 304.0, 32.0],
        [1790.0, 80.0, 1648.0, 16.0],
        [1790.0, mid, 1648.0, 32.0],
        [1790.0, high, 1648.0, 288.0],
        [446.0, mid, 304.0, 32.0],
        [446.0, high, 304.0, 288.0],
        [178.0, mid, 288.0, 32.0],
        [178.0, high, 288.0, 288.0],
        [130.0, mid, 240.0, 32.0],
        [130.0, high, 240.0, 288.0]
    );

    result
}

fn unwrap_triangle_strip(strip: &[PosColTexVertex], output: &mut Vec<PosColTexVertex>) {
    assert!(strip.len() >= 3);
    output.reserve(strip.len() - 2);

    for window in strip.windows(3) {
        output.push(window[0]);
        output.push(window[1]);
        output.push(window[2]);
    }
}

fn build_vertex_buffer(character_name_width: f32, height: f32) -> Vec<PosColTexVertex> {
    let mut result = Vec::new();
    result.reserve(MAX_VERTEX_COUNT);

    // TODO: take opacity into account

    unwrap_triangle_strip(
        &build_message_header_buffer(character_name_width),
        &mut result,
    );
    // let header = 0..result.len() as u32;

    unwrap_triangle_strip(&build_message_body_vertices(height), &mut result);
    // let body = header.end..result.len() as u32;

    assert!(result.len() < MAX_VERTEX_COUNT);

    result
}

pub struct Messagebox {
    textures: Arc<MessageboxTextures>,
    tex_vertex_buffer: VertexBuffer<PosColTexVertex>,
    fill_vertex_buffer: PosVertexBuffer,
    messagebox_type: MessageboxType,
    visible: bool,
    metrics: MessageMetrics,
    dynamic_height: f32,
}

impl Messagebox {
    pub fn new(textures: Arc<MessageboxTextures>, resources: &GpuCommonResources) -> Self {
        Self {
            textures,
            // TODO: reduce the capacity of the vertex buffer
            tex_vertex_buffer: VertexBuffer::new_updatable(
                resources,
                MAX_VERTEX_COUNT as u32,
                Some("Messagebox VertexBuffer"),
            ),
            fill_vertex_buffer: PosVertexBuffer::new_fullscreen(resources),
            messagebox_type: MessageboxType::Neutral,
            visible: false,
            metrics: MessageMetrics {
                character_name_width: 0.0,
                height: 360.0, // Static height: maximum height the message will ever have
            },
            dynamic_height: 360.0, // Dynamic height: potentially changes as the player clicks through the message
        }
    }
}

impl Updatable for Messagebox {
    fn update(&mut self, _context: &UpdateContext) {}
}

impl Renderable for Messagebox {
    fn render<'enc>(
        &'enc self,
        resources: &'enc GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'enc>,
        transform: Mat4,
        projection: Mat4,
    ) {
        if !self.visible {
            return;
        }

        render_pass.push_debug_group("Messagebox");

        match self.messagebox_type {
            MessageboxType::Neutral | MessageboxType::WitchSpace | MessageboxType::Ushiromiya => {
                let total_transform = projection
                    * transform
                    * Mat4::from_translation(vec3(
                        -960.0,
                        -540.0 + (1080.0 - self.dynamic_height) - 32.0,
                        0.0,
                    ));

                // TODO: do not upload the vertices if they haven't changed
                let vertices =
                    build_vertex_buffer(self.metrics.character_name_width, self.dynamic_height);
                self.tex_vertex_buffer.write(&resources.queue, &vertices);

                let texture = match self.messagebox_type {
                    MessageboxType::Neutral => &self.textures.message_window_1,
                    MessageboxType::WitchSpace => &self.textures.message_window_2,
                    MessageboxType::Ushiromiya => &self.textures.message_window_3,
                    _ => unreachable!(),
                }
                .gpu_texture(resources);

                resources.draw_sprite(
                    render_pass,
                    self.tex_vertex_buffer.vertex_source(),
                    texture.bind_group(),
                    total_transform,
                );
            }
            MessageboxType::Transparent | MessageboxType::NoText => {
                // the messagebox is invisible, no need to render anything (I think)
            }
            MessageboxType::Novel => {
                resources.draw_fill(
                    render_pass,
                    self.fill_vertex_buffer.vertex_source(),
                    projection * transform,
                    vec4(0.0, 0.0, 0.0, 0.7),
                );
            }
        }

        render_pass.pop_debug_group();
    }

    fn resize(&mut self, _resources: &GpuCommonResources) {}
}

impl Messagebox {
    pub fn set_messagebox_type(&mut self, messagebox_type: MessageboxType) {
        self.messagebox_type = messagebox_type;
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn set_metrics(&mut self, metrics: MessageMetrics) {
        self.metrics = metrics;
    }
}
