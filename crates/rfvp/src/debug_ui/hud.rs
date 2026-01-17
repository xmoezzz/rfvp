use std::sync::Arc;

use egui::{Align, Layout, TextStyle};

use crate::audio_player::bgm_player::BgmDebugSummary;
use crate::audio_player::se_player::SeDebugSummary;
use crate::debug_ui::log_ring::LogRing;
use crate::debug_ui::vm_snapshot::VmSnapshot;
use crate::rendering::gpu_prim::PrimRenderStats;

#[derive(Debug, Clone, Default)]
pub struct HudSnapshot {
    pub frame_no: u64,
    pub dt_ms: f32,
    pub render: PrimRenderStats,
    pub se: SeDebugSummary,
    pub bgm: BgmDebugSummary,
    pub vm: VmSnapshot,
}

pub struct DebugHud {
    ring: Arc<LogRing>,
    egui_ctx: egui::Context,
    renderer: egui_wgpu::Renderer,

    // Prepared each frame
    paint_jobs: Vec<egui::ClippedPrimitive>,
    textures_delta: egui::TexturesDelta,
    screen_desc: egui_wgpu::ScreenDescriptor,

    max_console_lines: usize,
}

impl DebugHud {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, ring: Arc<LogRing>) -> Self {
        let egui_ctx = egui::Context::default();
        let renderer = egui_wgpu::Renderer::new(device, surface_format, None, 1);

        Self {
            ring,
            egui_ctx,
            renderer,
            paint_jobs: Vec::new(),
            textures_delta: egui::TexturesDelta::default(),
            screen_desc: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [1, 1],
                pixels_per_point: 1.0,
            },
            max_console_lines: 700,
        }
    }

    pub fn set_max_console_lines(&mut self, n: usize) {
        self.max_console_lines = n.max(50);
    }

    pub fn prepare_frame(
        &mut self,
        window_size: (u32, u32),
        pixels_per_point: f32,
        snap: &HudSnapshot,
    ) {
        let (w, h) = window_size;
        self.screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [w.max(1), h.max(1)],
            pixels_per_point: pixels_per_point.max(0.5),
        };

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(w as f32 / pixels_per_point, h as f32 / pixels_per_point),
            )),
            // This HUD is intentionally "display-first" to avoid winit input-version coupling.
            // It still renders useful diagnostics without consuming user input.
            ..Default::default()
        };

        self.egui_ctx.begin_frame(raw_input);

        self.build_ui(snap);

        let full_output = self.egui_ctx.end_frame();
        self.textures_delta = full_output.textures_delta;
        self.paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        for (id, delta) in &self.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }

        self.renderer.update_buffers(device, queue, encoder, &self.paint_jobs, &self.screen_desc);

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("debug_hud"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            self.renderer.render(&mut rpass, &self.paint_jobs, &self.screen_desc);
        }

        for id in &self.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }

    fn build_ui(&self, snap: &HudSnapshot) {
        let console_lines = self.ring.snapshot_tail(self.max_console_lines);

        egui::TopBottomPanel::top("hud_top").show(&self.egui_ctx, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.heading("rfvp debug");
                ui.separator();
                ui.label(format!(
                    "frame={}  dt={:.2}ms",
                    snap.frame_no, snap.dt_ms
                ));
                ui.separator();
                ui.label(format!(
                    "render: quads={} verts={} draws={} textures={}",
                    snap.render.quad_count,
                    snap.render.vertex_count,
                    snap.render.draw_calls,
                    snap.render.cached_graphs
                ));
            });
        });

        egui::SidePanel::left("hud_console")
            .resizable(true)
            .default_width(560.0)
            .show(&self.egui_ctx, |ui| {
                ui.label("Console (log)");
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                        for line in console_lines {
                            ui.label(line);
                        }
                    });
            });

        egui::CentralPanel::default().show(&self.egui_ctx, |ui| {
            ui.columns(2, |cols| {
                // Audio
                cols[0].group(|ui| {
                    ui.label("Audio");
                    ui.separator();
                    ui.label(format!(
                        "SE: loaded_datas={} playing_slots={} (slots={})",
                        snap.se.loaded_datas, snap.se.playing.len(), snap.se.max_slots
                    ));
                    if !snap.se.playing.is_empty() {
                        ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                        for s in snap.se.playing.iter().take(24) {
                            ui.label(format!(
                                "slot {:02}  vol={:.2}  muted={}  loaded={}  kind={}",
                                s.slot,
                                s.volume,
                                if s.muted { 1 } else { 0 },
                                if s.data_loaded { 1 } else { 0 },
                                s.kind.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())
                            ));
                        }
                    }
                    ui.separator();
                    ui.label(format!(
                        "BGM: loaded_datas={} playing_slots={} (slots={})",
                        snap.bgm.loaded_datas, snap.bgm.playing.len(), snap.bgm.max_slots
                    ));
                    if !snap.bgm.playing.is_empty() {
                        ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                        for b in snap.bgm.playing.iter().take(8) {
                            ui.label(format!(
                                "slot {:02}  vol={:.2}  muted={}  loaded={}  kind={}",
                                b.slot,
                                b.volume,
                                if b.muted { 1 } else { 0 },
                                if b.data_loaded { 1 } else { 0 },
                                b.kind.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())
                            ));
                        }
                    }
                });

                // VM
                cols[1].group(|ui| {
                    ui.label("VM / Coroutine");
                    ui.separator();
                    let (run, wait, sleep, dissolve) = snap.vm.summarize_counts();
                    ui.label(format!(
                        "current={}  running={}  wait={}  sleep={}  dissolve_wait={}  tick={} ",
                        snap.vm.current_id, run, wait, sleep, dissolve, snap.vm.tick_seq
                    ));
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(420.0)
                        .show(ui, |ui| {
                            ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                            egui::Grid::new("vm_grid")
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label("id");
                                    ui.label("status");
                                    ui.label("wait_ms");
                                    ui.end_row();

                                    for e in &snap.vm.entries {
                                        let status = format_thread_state(e.status_bits);
                                        ui.label(format!("{:02}", e.id));
                                        ui.label(status);
                                        ui.label(format!("{}", e.wait_ms));
                                        ui.end_row();
                                    }
                                });
                        });
                });
            });
        });
    }
}

fn format_thread_state(bits: u32) -> String {
    // ThreadState bit layout:
    //   RUN=1, WAIT=2, SLEEP=4, DISSOLVE=16
    let run = (bits & 1) != 0;
    let wait = (bits & 2) != 0;
    let sleep = (bits & 4) != 0;
    let dissolve = (bits & 16) != 0;
    format!(
        "{}{}{}{}",
        if run { "RUN " } else { "    " },
        if wait { "WAI " } else { "    " },
        if sleep { "SLP " } else { "    " },
        if dissolve { "DIS" } else { "   " },
    )
}
