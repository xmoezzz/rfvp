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
    pub textures: Vec<String>,
    pub text_slots: Vec<String>,
    pub text_lines: Vec<String>,
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
        egui::TopBottomPanel::top("hud_top").show(&self.egui_ctx, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.heading("rfvp debug");
                ui.separator();
                let fps = if snap.dt_ms > 0.0 { 1000.0 / snap.dt_ms } else { 0.0 };
                ui.label(format!("frame={}  dt={:.2}ms  fps={:.1}", snap.frame_no, snap.dt_ms, fps));
                ui.separator();
                ui.label(format!("render: quads={} verts={} draws={} textures={}",
                    snap.render.quad_count,
                    snap.render.vertex_count,
                    snap.render.draw_calls,
                    snap.render.cached_graphs
                ));
                ui.separator();
                ui.label(format!("loaded textures={}", snap.textures.len()));
            });
        });

        egui::CentralPanel::default().show(&self.egui_ctx, |ui| {
            ui.columns(2, |cols| {
                cols[0].group(|ui| {
                    ui.label("Textures");
                    ui.separator();
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                            for t in &snap.textures {
                                ui.label(t);
                            }
                        });
                });

                

                cols[0].add_space(8.0);

                cols[0].group(|ui| {
                    ui.label("Text Slots");
                    ui.separator();
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                            for line in &snap.text_slots {
                                ui.label(line);
                            }
                        });
                });

cols[1].group(|ui| {
                    ui.label("Audio");
                    ui.separator();
                    ui.label(format!("BGM: loaded_datas={} playing_slots={} (slots={})",
                        snap.bgm.loaded_datas, snap.bgm.playing_slots, snap.bgm.max_slots
                    ));
                    egui::ScrollArea::both().max_height(220.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                            for b in &snap.bgm.slots {
                                let name = b.name.as_deref().unwrap_or("<unnamed>");
                                ui.label(format!("BGM[{slot:02}] {play} {load} vol={vol:.2} muted={muted} kind={kind} name={name}",
                                    slot=b.slot,
                                    play=if b.playing {"play"} else {"stop"},
                                    load=if b.data_loaded {"loaded"} else {"empty"},
                                    vol=b.volume,
                                    muted=if b.muted {1} else {0},
                                    kind=b.kind.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
                                    name=name
                                ));
                            }
                        });

                    ui.separator();
                    ui.label(format!("SE: loaded_datas={} playing_slots={} (slots={})",
                        snap.se.loaded_datas, snap.se.playing_slots, snap.se.max_slots
                    ));
                    egui::ScrollArea::both().max_height(280.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                            for s in &snap.se.slots {
                                let name = s.name.as_deref().unwrap_or("<unnamed>");
                                ui.label(format!("SE[{slot:03}] {play} {load} vol={vol:.2} muted={muted} kind={kind} name={name}",
                                    slot=s.slot,
                                    play=if s.playing {"play"} else {"stop"},
                                    load=if s.data_loaded {"loaded"} else {"empty"},
                                    vol=s.volume,
                                    muted=if s.muted {1} else {0},
                                    kind=s.kind.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
                                    name=name
                                ));
                            }
                        });
                });

                cols[1].add_space(8.0);

                cols[1].group(|ui| {
                    ui.label("VM / Coroutine");
                    ui.separator();
                    let (run, wait, sleep, dissolve) = snap.vm.summarize_counts();
                    ui.label(format!("current={}  running={}  wait={}  sleep={}  dissolve_wait={}  tick={} ",
                        snap.vm.current_id, run, wait, sleep, dissolve, snap.vm.tick_seq
                    ));
                    ui.separator();
                    egui::ScrollArea::both()
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
