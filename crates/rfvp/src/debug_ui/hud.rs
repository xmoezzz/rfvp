use std::collections::HashMap;
use std::sync::Arc;

use egui::{Align, Align2, Color32, FontId, Layout, TextStyle};

use crate::audio_player::bgm_player::BgmDebugSummary;
use crate::audio_player::se_player::SeDebugSummary;
use crate::debug_ui::log_ring::LogRing;
use crate::debug_ui::vm_snapshot::VmSnapshot;
use crate::rendering::gpu_prim::{DebugPrimTile, GpuPrimRenderer, PrimRenderStats};

#[derive(Debug, Clone, Copy, Default)]
pub struct HudInput {
    /// Pointer position in points (logical pixels). None means "pointer not in window".
    pub pointer_pos: Option<(f32, f32)>,
    pub pointer_down: bool,
    pub scroll_delta_y: f32,
}

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
    pub prim_tiles: Vec<DebugPrimTile>,
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

    // Debug: native textures for prim tile preview
    // graph_id -> (generation, egui texture id, texture size)
    prim_tile_textures: HashMap<u16, (u64, egui::TextureId, (u32, u32))>,
    prim_tile_px: f32,

    // Per-frame generation tracking (to explain "generation jumps" without external logs).
    prim_tile_prev_gen: HashMap<u16, u64>,

    last_pointer_pos: egui::Pos2,
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
            prim_tile_textures: HashMap::new(),
            prim_tile_px: 96.0,
            prim_tile_prev_gen: HashMap::new(),
            last_pointer_pos: egui::Pos2::ZERO,
        }
    }

    pub fn set_max_console_lines(&mut self, n: usize) {
        self.max_console_lines = n.max(50);
    }


    pub fn sync_prim_tile_textures(
        &mut self,
        device: &wgpu::Device,
        prim_renderer: &GpuPrimRenderer,
        tiles: &[DebugPrimTile],
    ) {
        if tiles.is_empty() {
            return;
        }

        // Tile size in points (HUD window logical units).
        let tile_px: f32 = std::env::var("RFVP_HUD_PRIM_TILE_PX")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(96.0)
            .clamp(32.0, 256.0);
        self.prim_tile_px = tile_px;

        // Collect unique graph ids used in the current frame.
        let mut graph_ids: Vec<u16> = Vec::new();
        graph_ids.reserve(tiles.len());
        for t in tiles {
            if !graph_ids.contains(&t.graph_id) {
                graph_ids.push(t.graph_id);
            }
        }

        for gid in graph_ids {
            let Some((gen, view, size)) = prim_renderer.debug_graph_native(gid) else {
                continue;
            };

            let needs_register = match self.prim_tile_textures.get(&gid) {
                Some((old_gen, _, _)) => *old_gen != gen,
                None => true,
            };

            if !needs_register {
                continue;
            }

            if let Some((_, old_id, _)) = self.prim_tile_textures.remove(&gid) {
                // Free the previous native texture binding in egui.
                self.renderer.free_texture(&old_id);
            }

            // Register a native wgpu texture view for egui.
            let tex_id = self
                .renderer
                .register_native_texture(device, view, wgpu::FilterMode::Linear);

            self.prim_tile_textures.insert(gid, (gen, tex_id, size));
        }
    }

    pub fn prepare_frame(
        &mut self,
        window_size: (u32, u32),
        pixels_per_point: f32,
        snap: &HudSnapshot,
        input: Option<HudInput>,
    ) {
        let (w, h) = window_size;
        self.screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [w.max(1), h.max(1)],
            pixels_per_point: pixels_per_point.max(0.5),
        };

        let mut raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(w as f32 / pixels_per_point, h as f32 / pixels_per_point),
            )),
            // This HUD is intentionally "display-first" to avoid winit input-version coupling.
            // It still renders useful diagnostics without consuming user input.
            ..Default::default()
        };

        // Optional input injection (for hover/click in debug HUD). Keep this gated so the HUD
        // remains safe to render even if event wiring breaks.
        if std::env::var("RFVP_HUD_INPUT").as_deref() == Ok("1") {
            if let Some(inp) = input {
                if let Some((x, y)) = inp.pointer_pos {
                    let pos = egui::Pos2::new(x, y);
                    self.last_pointer_pos = pos;
                    raw_input.events.push(egui::Event::PointerMoved(pos));
                } else {
                    raw_input.events.push(egui::Event::PointerGone);
                }
                raw_input.events.push(egui::Event::PointerButton {
                    pos: self.last_pointer_pos,
                    button: egui::PointerButton::Primary,
                    pressed: inp.pointer_down,
                    modifiers: egui::Modifiers::default(),
                });
                if inp.scroll_delta_y.abs() > 0.0 {
                    raw_input.events.push(egui::Event::Scroll(egui::Vec2::new(0.0, inp.scroll_delta_y)));
                }
            }
        }

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

    fn build_ui(&mut self, snap: &HudSnapshot) {
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
            if !snap.prim_tiles.is_empty() {
                ui.group(|ui| {
                    ui.label(format!("Prim Tiles (candidates): {}", snap.prim_tiles.len()));
                    ui.separator();

                    // Track generation deltas to explain "generation jumps".
                    let mut delta_map: HashMap<u16, u64> = HashMap::new();
                    let mut deltas: Vec<(u16, u64, u64, (u8, u8, u8))> = Vec::new();
                    for t in &snap.prim_tiles {
                        let prev = self.prim_tile_prev_gen.get(&t.graph_id).copied().unwrap_or(t.graph_gen);
                        let delta = t.graph_gen.saturating_sub(prev);
                        self.prim_tile_prev_gen.insert(t.graph_id, t.graph_gen);
                        delta_map.insert(t.graph_id, delta);
                        if delta > 0 {
                            deltas.push((t.graph_id, delta, t.graph_gen, (t.graph_r, t.graph_g, t.graph_b)));
                        }
                    }
                    deltas.sort_by(|a, b| b.1.cmp(&a.1));

                    if !deltas.is_empty() {
                        egui::CollapsingHeader::new("Generation deltas (since last HUD frame)")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                                for (gid, d, gen, (r, g, b)) in deltas.iter().take(20) {
                                    ui.label(format!("g{:04}: +{:6} (gen={}) rgb=({}, {}, {})", gid, d, gen, r, g, b));
                                }
                            });
                    }
                    egui::ScrollArea::horizontal()
                        .auto_shrink([false, true])
                        .max_height(self.prim_tile_px + 12.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for t in &snap.prim_tiles {
                                    let gen_delta = delta_map.get(&t.graph_id).copied().unwrap_or(0);
                                    let entry = self.prim_tile_textures.get(&t.graph_id);
                                    let (tex_id, tex_size, hud_gen) = match entry {
                                        Some((gen, id, sz)) => (Some(*id), Some(*sz), *gen),
                                        None => (None, None, 0),
                                    };

                                    let (rect, resp) = ui.allocate_exact_size(
                                        egui::vec2(self.prim_tile_px, self.prim_tile_px),
                                        egui::Sense::hover(),
                                    );

                                    if let Some(id) = tex_id {
                                        // Fit texture into tile while preserving aspect ratio ("contain").
                                        let mut img_rect = rect;
                                        if let Some((tw, th)) = tex_size {
                                            if tw > 0 && th > 0 {
                                                let ar_tex = tw as f32 / th as f32;
                                                let ar_tile = rect.width() / rect.height();
                                                if ar_tex > ar_tile {
                                                    // Wider: letterbox vertically.
                                                    let h = rect.width() / ar_tex;
                                                    let y0 = rect.center().y - h * 0.5;
                                                    img_rect = egui::Rect::from_min_size(
                                                        egui::pos2(rect.min.x, y0),
                                                        egui::vec2(rect.width(), h),
                                                    );
                                                } else {
                                                    // Taller: letterbox horizontally.
                                                    let w = rect.height() * ar_tex;
                                                    let x0 = rect.center().x - w * 0.5;
                                                    img_rect = egui::Rect::from_min_size(
                                                        egui::pos2(x0, rect.min.y),
                                                        egui::vec2(w, rect.height()),
                                                    );
                                                }
                                            }
                                        }

                                        ui.painter().image(
                                            id,
                                            img_rect,
                                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(20));
                                    }

                                    // Prim id overlay (green) at top-left.
                                    ui.painter().text(
                                        rect.min + egui::vec2(2.0, 2.0),
                                        Align2::LEFT_TOP,
                                        format!("{}", t.prim_id),
                                        FontId::monospace(16.0),
                                        Color32::GREEN,
                                    );

                                    // Graph id (small) below the prim id.
                                    ui.painter().text(
                                        rect.min + egui::vec2(2.0, 18.0),
                                        Align2::LEFT_TOP,
                                        format!("g{}", t.graph_id),
                                        FontId::monospace(12.0),
                                        Color32::from_gray(200),
                                    );

                                    if gen_delta > 0 {
                                        ui.painter().text(
                                            rect.max + egui::vec2(-2.0, -2.0),
                                            Align2::RIGHT_BOTTOM,
                                            format!("+{}", gen_delta),
                                            FontId::monospace(12.0),
                                            Color32::from_rgb(255, 220, 0),
                                        );
                                    }

                                    resp.on_hover_text(format!(
                                        "prim_id={} graph_id={} kind={:?}\ngraph: ready={} cpu={} gen={} size={}x{} rgb=({}, {}, {})\nhud: cached_gen={} (RFVP_HUD_PRIM_TILE_PX to resize)",
                                        t.prim_id,
                                        t.graph_id,
                                        t.kind,
                                        if t.graph_ready { 1 } else { 0 },
                                        if t.graph_has_cpu { 1 } else { 0 },
                                        t.graph_gen,
                                        t.graph_w,
                                        t.graph_h,
                                        t.graph_r,
                                        t.graph_g,
                                        t.graph_b,
                                        hud_gen,
                                    ));
                                }
                            });
                        });
                });

                ui.add_space(8.0);
            }

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
