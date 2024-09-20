use std::sync::Arc;

use glam::{vec2, Mat4, Vec2};
use rfvp_core::{
    format::font::GlyphTrait,
    layout::{
        Action, ActionType, Block, BlockExitCondition, LayoutedChar, LayoutedMessage, LayoutingMode,
    },
    time::Ticks,
    vm::command::types::MessageTextLayout,
};
use rfvp_render::{vertices::TextVertex, GpuCommonResources, Renderable, VertexBuffer};
use tracing::warn;

use crate::{
    layer::message_layer::font_atlas::FontAtlas,
    render::dynamic_atlas::AtlasImage,
    update::{Updatable, UpdateContext},
};

/// Calculated global metrics for a message. Used to adjust the sizes of individual parts of
/// the message box, such that it fits the character name and the entire height of the message
#[derive(Copy, Clone)]
pub struct MessageMetrics {
    pub character_name_width: f32,
    pub height: f32,
}

pub struct Message {
    time: Ticks,
    font_atlas: Arc<FontAtlas>,
    used_codepoints: Vec<u16>,
    actions: Vec<Action>,
    blocks: Vec<Block>,
    vertex_buffer: VertexBuffer<TextVertex>,
    sent_signals: u32,
    received_signals: u32,
    completed_blocks: u32,
    metrics: MessageMetrics,
}

pub enum MessageStatus {
    Printing,
    ClickWaiting,
    SignalWaiting,
    Complete,
}

impl Message {
    pub fn new(
        context: &UpdateContext,
        font_atlas: Arc<FontAtlas>,
        base_position: Vec2,
        show_character_name: bool,
        message: &str,
    ) -> Self {
        // let mut font_atlas_guard = font_atlas.lock().unwrap();

        let layout_params = rfvp_core::layout::LayoutParams {
            font: font_atlas.get_font(),
            layout_width: 1500.0,
            character_name_layout_width: 384.0,
            base_font_height: 50.0,
            furigana_font_height: 20.0,
            font_horizontal_base_scale: 0.9697,
            text_layout: MessageTextLayout::Left,
            default_state: Default::default(),
            has_character_name: true,
            mode: LayoutingMode::MessageText,
        };

        let LayoutedMessage {
            mut character_name_chars,
            chars,
            mut actions,
            mut blocks,
        } = rfvp_core::layout::layout_text(layout_params, message);

        if !show_character_name {
            character_name_chars = None;
        }

        // reverse the blocks & actions so that we can easily pop them off the end in order
        blocks.reverse();
        actions.reverse();

        // Determine position and width of the character name part, if present
        let (character_name_start_x, character_name_actual_width) = match character_name_chars {
            Some(ref character_name_chars) => {
                let start_x = character_name_chars
                    .first()
                    .map(|c| c.position.x)
                    .unwrap_or(0.0_f32);
                let end_x = character_name_chars
                    .last()
                    .map(|c| c.position.x + c.size.advance_width)
                    .unwrap_or(0.0_f32);
                (start_x, end_x - start_x)
            }
            None => (0.0_f32, 0.0_f32),
        };

        let metrics = MessageMetrics {
            character_name_width: if character_name_actual_width > 0.0 {
                // The character name part is always at least 400 pixels wide
                character_name_actual_width.max(400.0)
            } else {
                0.0
            },

            // TODO: calculate message height
            height: 360.0,
        };

        let character_name_x_offset =
            (metrics.character_name_width - character_name_actual_width) / 2.0;

        // perform layout post-processing on the character name; chain to form an iterator
        // over all chars
        let all_chars_iter = character_name_chars
            .into_iter()
            .flatten()
            .map(|c| {
                LayoutedChar {
                    // Here we subtract the start_x so the character name is always layouted identically no matter
                    // where the character name "line" was initially layouted to, to avoid problems with center-/
                    // right-aligned lines
                    position: c.position
                        - vec2(
                            character_name_start_x - character_name_x_offset + 20.0,
                            16.0,
                        ),
                    ..c
                }
            })
            .chain(chars);

        let mut used_codepoints = Vec::new();
        let mut vertices = Vec::new();
        for char in all_chars_iter {
            // TODO: support for BOLD font
            let glyph_info = font_atlas
                .get_font()
                .get_glyph_for_character(char.codepoint)
                .get_info();

            let atlas_size = font_atlas.texture_size();
            let atlas_size = vec2(atlas_size.0 as f32, atlas_size.1 as f32);

            let AtlasImage {
                position: tex_position,
                size: _, // the atlas size is not to be trusted, as it can be larger than the actual texture (even larger than the power of 2 padded texture...)
            } = font_atlas.get_glyph(context.gpu_resources, char.codepoint);
            // save the codepoint to free it from the atlas later
            used_codepoints.push(char.codepoint);

            // just use the actual size of the glyph
            let tex_size = glyph_info.actual_size();
            let tex_size = vec2(tex_size.0 as f32, tex_size.1 as f32);

            // scale texture coordinates to the size of the texture
            let tex_position = tex_position / atlas_size;
            let tex_size = tex_size / atlas_size;

            let position = base_position
                + char.position
                + vec2(
                    glyph_info.bearing_x as f32 * char.size.horizontal_scale,
                    -glyph_info.bearing_y as f32 * char.size.scale,
                );
            let size = char.size.size();

            let time = char.time;
            let fade = char.fade;
            let color = char.color;

            // TODO: do the fade calculation here

            // helper macro to reduce vertex creation boilerplate
            macro_rules! v {
                (($x:expr, $y:expr), ($tex_x:expr, $tex_y:expr)) => {
                    TextVertex {
                        position: position + vec2($x, $y) * size,
                        tex_position: tex_position + vec2($tex_x, $tex_y) * tex_size,
                        color,
                        time,
                        fade,
                    }
                };
            }

            vertices.extend([
                // Top left triangle
                v!((0.0, 0.0), (0.0, 0.0)),
                v!((1.0, 0.0), (1.0, 0.0)),
                v!((0.0, 1.0), (0.0, 1.0)),
                // Bottom right triangle
                v!((1.0, 1.0), (1.0, 1.0)),
                v!((0.0, 1.0), (0.0, 1.0)),
                v!((1.0, 0.0), (1.0, 0.0)),
            ]);
        }

        let vertex_buffer = VertexBuffer::new(
            context.gpu_resources,
            &vertices,
            Some("Message VertexBuffer"),
        );

        Self {
            time: Ticks::ZERO,
            font_atlas,
            used_codepoints,
            actions,
            blocks,
            vertex_buffer,
            sent_signals: 0,
            received_signals: 0,
            completed_blocks: 0,
            metrics,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn status(&self) -> MessageStatus {
        match self.current_block() {
            None => MessageStatus::Complete,
            Some(block) => {
                if block.completed(self.time) {
                    match block.exit_condition {
                        BlockExitCondition::ClickWait => MessageStatus::ClickWaiting,
                        BlockExitCondition::Signal(_) => MessageStatus::SignalWaiting,
                        BlockExitCondition::None => unreachable!(
                            "If Block has None as exit condition it should be immediately removed"
                        ),
                    }
                } else {
                    MessageStatus::Printing
                }
            }
        }
    }

    fn current_block(&self) -> Option<&Block> {
        self.blocks.last()
    }

    fn next_block(&mut self) {
        // let old_block =
        self.blocks
            .pop()
            .expect("Message::next_block called when no blocks remain");
        self.completed_blocks += 1;

        // let overshoot_time = self.time - old_block.end_time;
        if let Some(block) = self.current_block() {
            self.time = block.start_time;
            self.execute_actions();
            // self.time += overshoot_time;
        }
    }

    pub fn advance(&mut self) {
        if let Some(block) = self.current_block() {
            if block.completed(self.time)
                && matches!(block.exit_condition, BlockExitCondition::ClickWait)
            {
                self.next_block();
            } else {
                // skip to the end of the current block
                // NOTE: we may want to have a separate control for that
                self.time = block.end_time;
            }
        }
    }

    pub fn fast_forward(&mut self) {
        while let Some(block) = self.current_block() {
            if block.completed(self.time) {
                if matches!(block.exit_condition, BlockExitCondition::ClickWait) {
                    self.next_block();
                } else {
                    break;
                }
            } else {
                // skip to the end of the current block
                // NOTE: we may want to have a separate control for that
                self.time = block.end_time;
            }
        }
    }

    pub fn signal(&mut self) {
        self.received_signals += 1;
    }

    fn execute_actions(&mut self) {
        while let Some(action) = self.actions.last() {
            if action.time > self.time {
                break;
            }
            let action = self.actions.pop().unwrap();
            match action.action_type {
                ActionType::SetLipSync(state) => warn!("Ignoring SetLipSync action: {:?}", state),
                ActionType::VoiceVolume(volume) => {
                    warn!("Ignoring voice volume change: {}", volume)
                }
                ActionType::Voice(filename) => warn!("Ignoring voice action: {}", filename),
                ActionType::SignalSection => self.sent_signals += 1,
            }
        }
    }

    pub fn completed_blocks(&self) -> u32 {
        self.completed_blocks
    }

    pub fn sent_signals(&self) -> u32 {
        self.sent_signals
    }

    pub fn received_signals(&self) -> u32 {
        self.received_signals
    }

    pub fn time(&self) -> Ticks {
        self.time
    }

    pub fn metrics(&self) -> MessageMetrics {
        self.metrics
    }
}

impl Updatable for Message {
    fn update(&mut self, context: &UpdateContext) {
        if let Some(block) = self.current_block() {
            if !block.completed(self.time) {
                self.time += context.time_delta_ticks();
            } else {
                match block.exit_condition {
                    BlockExitCondition::None => self.next_block(),
                    BlockExitCondition::Signal(s) if self.received_signals > s => self.next_block(),
                    _ => {}
                }
            }
            self.execute_actions();
        }
    }
}

impl Renderable for Message {
    fn render<'enc>(
        &'enc self,
        resources: &'enc GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'enc>,
        transform: Mat4,
        projection: Mat4,
    ) {
        const OUTLINE_DISTANCE: f32 = 3.5;

        let total_transform = projection * transform;

        let atlas_size = self.font_atlas.texture_size();
        let scaled_distance = OUTLINE_DISTANCE / vec2(atlas_size.0 as f32, atlas_size.1 as f32);

        render_pass.push_debug_group("Message");
        resources.draw_text_outline(
            render_pass,
            self.vertex_buffer.vertex_source(),
            self.font_atlas.texture_bind_group(),
            total_transform,
            self.time,
            scaled_distance,
        );

        resources.draw_text(
            render_pass,
            self.vertex_buffer.vertex_source(),
            self.font_atlas.texture_bind_group(),
            total_transform,
            self.time,
        );
        render_pass.pop_debug_group();
    }

    fn resize(&mut self, _resources: &GpuCommonResources) {}
}

impl Drop for Message {
    fn drop(&mut self) {
        for &codepoint in self.used_codepoints.iter() {
            self.font_atlas.free_glyph(codepoint);
        }
    }
}
