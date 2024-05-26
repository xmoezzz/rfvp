use std::cell::RefCell;

use bevy_utils::HashMap;
use egui::{
    style::WidgetVisuals, ClippedPrimitive, CollapsingHeader, Color32, Context, FontFamily, FontId,
    InnerResponse, Rounding, Stroke, TextureId, Ui,
};
use egui_wgpu::{ScreenDescriptor, Renderer};
use glam::vec2;
use rfvp_render::GpuCommonResources;

use crate::utils::time::Time;

pub struct OverlayManager {
    show_overlays_window: bool,
    renderer: Renderer,
    context: Context,
    primitives: Vec<ClippedPrimitive>,
    free_textures: Vec<TextureId>,
    storage: OverlayStateStorage,
}

impl OverlayManager {
    pub fn new(resources: &GpuCommonResources, texture_format: wgpu::TextureFormat) -> Self {
        let renderer = Renderer::new(&resources.device, texture_format, None, 1);
        let context = Context::default();

        let alpha = 128;

        let from_gray = |l: u8| Color32::from_rgba_unmultiplied(l, l, l, alpha);

        context.set_style(egui::Style {
            override_font_id: Some(FontId {
                size: 14.0,
                // TODO: use Fira Code
                family: FontFamily::Monospace,
            }),
            visuals: egui::Visuals {
                widgets: egui::style::Widgets {
                    noninteractive: WidgetVisuals {
                        bg_fill: from_gray(248),
                        weak_bg_fill: from_gray(248),
                        bg_stroke: Stroke::new(1.0, from_gray(190)), // separators, indentation lines
                        fg_stroke: Stroke::new(1.0, Color32::from_gray(180)), // normal text color
                        rounding: Rounding::same(2.0),
                        expansion: 0.0,
                    },
                    inactive: WidgetVisuals {
                        bg_fill: from_gray(230), // button background
                        weak_bg_fill: from_gray(230),
                        bg_stroke: Default::default(),
                        fg_stroke: Stroke::new(1.0, Color32::from_gray(180)), // button text
                        rounding: Rounding::same(2.0),
                        expansion: 0.0,
                    },
                    hovered: WidgetVisuals {
                        bg_fill: from_gray(220),
                        weak_bg_fill: from_gray(220),
                        bg_stroke: Stroke::new(1.0, from_gray(105)), // e.g. hover over window edge or button
                        fg_stroke: Stroke::new(1.5, Color32::BLACK),
                        rounding: Rounding::same(3.0),
                        expansion: 1.0,
                    },
                    active: WidgetVisuals {
                        bg_fill: from_gray(165),
                        weak_bg_fill: from_gray(165),
                        bg_stroke: Stroke::new(1.0, from_gray(255)),
                        fg_stroke: Stroke::new(2.0, Color32::BLACK),
                        rounding: Rounding::same(2.0),
                        expansion: 1.0,
                    },
                    open: WidgetVisuals {
                        bg_fill: from_gray(220),
                        weak_bg_fill: from_gray(220),
                        bg_stroke: Stroke::new(1.0, from_gray(160)),
                        fg_stroke: Stroke::new(1.0, Color32::BLACK),
                        rounding: Rounding::same(2.0),
                        expansion: 0.0,
                    },
                },
                window_fill: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                panel_fill: from_gray(248),
                ..egui::Visuals::light()
            },
            ..Default::default()
        });

        Self {
            show_overlays_window: false,
            renderer,
            context,
            primitives: Vec::new(),
            free_textures: Vec::new(),
            storage: OverlayStateStorage::new(),
        }
    }

    fn screen_descriptor(&self) -> ScreenDescriptor {
        let ctx = &self.context;

        let pixels_per_point = ctx.pixels_per_point();
        let size = ctx.input(|i| i.screen_rect.size());
        let size = vec2(size.x, size.y); // convert from egui Vec2 to glam Vec2
        assert_ne!(
            size,
            vec2(10000.0, 10000.0),
            "Screen size is not set, was the update method called?"
        );
        ScreenDescriptor {
            size_in_pixels: [size.x as u32, size.y as u32],
            pixels_per_point,
        }
    }

    pub fn start_update(
        &mut self,
        _time: &Time,
        _window_size: (u32, u32),
    ) {
        let _ctx = &self.context;

        // self.action_state.update(raw_input_state);

        // if self
        //     .action_state
        //     .is_just_pressed(OverlayManagerAction::ToggleOverlayManager)
        // {
        //     self.show_overlays_window = !self.show_overlays_window;
        // }

        for id in self.free_textures.drain(..) {
            self.renderer.free_texture(&id);
        }

        let raw_input = egui::RawInput::default();
        self.context.begin_frame(raw_input);
    }

    /// Visit overlays and show them
    /// This method should be called exactly once after `start_update` and before `end_update`
    pub fn visit_overlays(&mut self, visit_fn: impl FnOnce(&mut OverlayCollector)) {
        let ctx = &self.context;

        egui::Area::new(egui::Id::new("top-left")).show(ctx, |top_left| {
            let mut visit_fn = Some(visit_fn);

            let window_shown = if self.show_overlays_window {
                let result = egui::Window::new("Overlays")
                    .resizable(false)
                    .show(ctx, |ui| {
                        let mut collector = OverlayCollector {
                            ctx,
                            top_left,
                            ui: Some(ui),
                            storage: &mut self.storage,
                        };

                        visit_fn.take().unwrap()(&mut collector);
                    });
                !matches!(result, Some(InnerResponse { inner: None, .. }))
            } else {
                false
            };
            if !window_shown {
                // if the window is closed, we still want to visit the visitable, just without a ui
                let mut collector = OverlayCollector {
                    ctx,
                    top_left,
                    ui: None,
                    storage: &mut self.storage,
                };

                visit_fn.take().unwrap()(&mut collector);
            }
        });
    }

    pub fn finish_update(
        &mut self,
        resources: &GpuCommonResources,
    ) {
        let ctx = &self.context;
        let full_output = self.context.end_frame();

        // TODO: handle platform outputs or smth
        
        self.primitives = ctx.tessellate(full_output.shapes, 1.0);

        // update the textures as requested
        for (id, tex) in full_output.textures_delta.set {
            self.renderer
                .update_texture(&resources.device, &resources.queue, id, &tex);
        }

        let mut encoder =
            resources
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Egui Encoder"),
                });

        let user_cmd_bufs = self.renderer.update_buffers(
            &resources.device,
            &resources.queue,
            &mut encoder,
            &self.primitives,
            &self.screen_descriptor(),
        );

        resources.queue.submit(
            user_cmd_bufs
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
    }

    pub fn render<'a>(
        &'a self,
        _resources: &GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        render_pass.push_debug_group("Overlay");
        self.renderer
            .render(render_pass, &self.primitives, &self.screen_descriptor());
        render_pass.pop_debug_group();
    }
}

struct OverlayStateStorage {
    overlays: HashMap<String, bool>,
    subgroups: HashMap<String, RefCell<OverlayStateStorage>>,
}

// TODO: we should learn to persist the overlay states
impl OverlayStateStorage {
    pub fn new() -> Self {
        Self {
            overlays: HashMap::new(),
            subgroups: HashMap::new(),
        }
    }
}

pub struct OverlayCollector<'a, 'top_left, 'ctx> {
    ctx: &'ctx Context,
    top_left: &'top_left mut Ui,
    ui: Option<&'a mut Ui>,
    storage: &'a mut OverlayStateStorage,
}

impl<'a, 'top_left, 'ctx> OverlayCollector<'a, 'top_left, 'ctx> {
    pub fn overlay(&mut self, name: &str, content: impl FnOnce(&Context, &mut Ui), default: bool) {
        let state = self
            .storage
            .overlays
            .entry(name.to_string())
            .or_insert(default);
        if let Some(ui) = self.ui.as_mut() {
            ui.checkbox(state, name);
        }
        if *state {
            content(self.ctx, self.top_left);
        }
    }

    pub fn subgroup(
        &mut self,
        name: &str,
        content: impl FnOnce(&mut OverlayCollector),
        default_open: bool,
    ) {
        let state = self
            .storage
            .subgroups
            .entry(name.to_string())
            .or_insert_with(|| RefCell::new(OverlayStateStorage::new()));

        if let Some(ui) = self.ui.as_mut() {
            CollapsingHeader::new(name)
                .default_open(default_open)
                .show(ui, |ui| {
                    content(&mut OverlayCollector {
                        ctx: self.ctx,
                        top_left: self.top_left,
                        ui: Some(ui),
                        storage: &mut state.borrow_mut(),
                    });
                });
        } else {
            content(&mut OverlayCollector {
                ctx: self.ctx,
                top_left: self.top_left,
                ui: None,
                storage: &mut state.borrow_mut(),
            });
        }
    }
}

pub trait OverlayVisitable {
    fn visit_overlay(&self, collector: &mut OverlayCollector);
}
