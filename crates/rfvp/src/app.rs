use anyhow::Result;
use std::{
    collections::HashMap, fs::File, path::{Path, PathBuf}, slice::Windows, sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard}, time::Instant
};
use glam::{mat4, vec3, vec4, Mat4};
use image::{imageops::FilterType, RgbaImage};
use wgpu::util::DeviceExt;
use regex::Regex;
use crate::{
    script::{
        global::GLOBAL,
        parser::{Nls, Parser}, Variant,
    },
    subsystem::resources::thread_manager::ThreadManager,
    utils::ani::{self, icondir_to_custom_cursor, CursorBundle},
};

use winit::{dpi::{PhysicalPosition, PhysicalSize, Size}, window::CustomCursor};
use winit::{
    keyboard::{KeyCode, PhysicalKey},
    event::{Event, WindowEvent, MouseButton, MouseScrollDelta},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use crate::rendering::render_tree::RenderTree;
use crate::vm_worker::VmWorker;

use crate::subsystem::scene::{Scene, SceneAction, SceneMachine};
use crate::subsystem::scheduler::Scheduler;
use crate::subsystem::world::GameData;
use crate::{config::app_config::AppConfig, subsystem::event_handler::update_input_events};
use crate::rfvp_render::{BindGroupLayouts, GpuCommonResources, Pipelines, RenderTarget};
use crate::rfvp_render::vertices::{PosVertex, VertexSource};


use crate::rendering::gpu_prim::GpuPrimRenderer;
use crate::debug_ui::{self, hud::{DebugHud, HudInput, HudSnapshot}};
use crate::debug_ui::log_ring::{self, LogRing};
use crate::subsystem::resources::motion_manager::DissolveType;

// ----------------------------
// GameData lock helpers
// ----------------------------
#[inline]
fn gd_read<'a>(gd: &'a Arc<RwLock<GameData>>) -> RwLockReadGuard<'a, GameData> {
    match gd.read() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[inline]
fn gd_write<'a>(gd: &'a Arc<RwLock<GameData>>) -> RwLockWriteGuard<'a, GameData> {
    match gd.write() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}


#[derive(Clone, Copy, Debug)]
struct WindowedRestoreState {
    size: PhysicalSize<u32>,
    pos: Option<PhysicalPosition<i32>>,
}

pub struct App {
    config: AppConfig,
    game_data: Arc<RwLock<GameData>>,
    title: String,
    vm_worker: VmWorker,
    pending_vm_frame_ms: u64,
    pending_vm_frame_ms_valid: bool,
    scheduler: Scheduler,
    layer_machine: SceneMachine,
    window: Option<Arc<Window>>,

    // WindowMode support
    windowed_restore: Option<WindowedRestoreState>,
    last_fullscreen_flag: i32,

    render_target: RenderTarget,
    resources: Arc<GpuCommonResources>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    prim_renderer: GpuPrimRenderer,
    virtual_size: (u32, u32),
    render_tree: RenderTree,
    dissolve_vertex_buffer: wgpu::Buffer,
    dissolve_index_buffer: wgpu::Buffer,
    dissolve_num_indices: u32,


    // ----------------------------
    // Debug HUD (FVP_TEST=1)
    // ----------------------------
    debug_hud: Option<DebugHud>,
    hud_window: Option<Arc<Window>>,
    hud_surface: Option<wgpu::Surface<'static>>,
    hud_surface_config: Option<wgpu::SurfaceConfiguration>,
    hud_visible: bool,
    debug_ring: Arc<LogRing>,
    debug_frame_no: u64,
    last_dt_ms: f32,
    hud_cursor_pos: Option<(f64, f64)>,
    hud_pointer_down: bool,
    hud_scroll_delta_y: f32,
    // Tracks dissolve completion on the main thread so we can wake contexts
    // waiting on DISSOLVE_WAIT immediately via an EngineEvent.
    last_dissolve_type: DissolveType,
    last_dissolve2_transitioning: bool,
}

impl App {
    #[allow(dead_code)]
    pub fn app() -> AppBuilder {
        let app_config = AppConfig::default();
        App::app_with_config(app_config)
    }

    pub fn app_with_config(app_config: AppConfig) -> AppBuilder {
        crate::utils::logger::Logger::init_logging(app_config.logger_config.clone());
        log::info!(
            "Starting the app, with the following configuration \n {:?}",
            app_config
        );
        AppBuilder::new(app_config)
    }

    fn setup(&mut self) {
        self.initialize_internal_resources();
        {
            let mut gd = gd_write(&self.game_data);
            self.layer_machine.apply_scene_action(SceneAction::Start, &mut gd);
            if gd.has_cursor(1) {
                gd.switch_cursor(1);
            }
        }
    }

    fn initialize_internal_resources(&mut self) {
        let mut gd = gd_write(&self.game_data);

        let window = self.window.as_ref().expect("No window found during setup");
        gd
            .set_window(crate::subsystem::resources::window::Window::new(
                (window.inner_size().width, window.inner_size().height),
                window.scale_factor(),
            ));
    }
    fn window(&self) -> &Arc<Window> {
        self.window.as_ref().expect("No window found")
    }

    fn debug_title(&mut self, x: i32, y: i32) {
        let title = self.title.clone();
        let (x, y, down, up) = {
            let gd = gd_read(&self.game_data);
            (
                gd.inputs_manager.get_cursor_x(),
                gd.inputs_manager.get_cursor_y(),
                self.debug_keydown(),
                self.debug_keyup(),
            )
        };
        if let Ok(test) = std::env::var("DEBUG") {
            if test == *"1" {
                let title = format!("{} | {},{} | down {}, up {} | ", title, x, y, down, up);
                self.window.as_mut().unwrap().set_title(&title);
            }
        }
    }

    fn debug_keydown(&self) -> String {
        let gd = gd_read(&self.game_data);
        gd.inputs_manager.get_input_down().to_string()
    }

    fn debug_keyup(&self) -> String {
        let gd = gd_read(&self.game_data);
        gd.inputs_manager.get_input_up().to_string()
    }

    fn run(mut self, event_loop: EventLoop<()>) {
        let _result = event_loop.run(move |event, loopd| {
            loopd.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == self.window.as_mut().unwrap().id() => {
                    let is_input_event = matches!(
                        event,
                        WindowEvent::KeyboardInput { .. }
                            | WindowEvent::MouseInput { .. }
                            | WindowEvent::MouseWheel { .. }
                            | WindowEvent::CursorMoved { .. }
                            | WindowEvent::CursorEntered { .. }
                            | WindowEvent::CursorLeft { .. }
                    );
                    match event {
                        WindowEvent::CloseRequested => {
                            // ExitMode(2) can disable immediate close; in that case the engine
                            // marks "close pending" and lets the script decide when to exit.
                            let mut gd = gd_write(&self.game_data);
                            if gd.get_close_immediate() {
                                loopd.exit();
                            } else {
                                gd.set_close_pending(true);
                            }
                        }
                        WindowEvent::Focused(_focused) => {
                            // Do not introduce WindowMode side effects on focus changes.
                            // Input focus transitions are handled in update_input_events().
                        }
                        WindowEvent::Resized(physical_size) => {
                            gd_write(&self.game_data).window_mut().set_dimensions(physical_size.width, physical_size.height);

                            // Update swapchain configuration.
                            self.surface_config.width = physical_size.width.max(1);
                            self.surface_config.height = physical_size.height.max(1);
                            self.surface.configure(&self.resources.device, &self.surface_config);
                        }
                        WindowEvent::ScaleFactorChanged {  .. } => {
                            // self.renderer.as_mut().unwrap().resize(
                            //     self.window.as_ref().expect("Missing window").inner_size(),
                            //     *scale_factor,
                            // );
                        }
                        WindowEvent::RedrawRequested => {
                            // Drive the simulation from redraws so we do not busy-spin.
                            let (frame_ms, notify_dissolve_done) = self.next_frame();

                            // Wake dissolve waiters before advancing the VM for this frame.
                            if notify_dissolve_done {
                                self.vm_worker.send_dissolve_done_sync();
                            }

                            // Run the script VM before rendering so scene changes become visible immediately.
                            //
                            // Important: the VM enforces a per-context opcode budget (see VmRunner). Hitting that
                            // budget introduces an artificial yield point which can expose transient scene states
                            // (e.g., prim init defaults to draw=1, later hidden by script). The original engine is
                            // effectively single-threaded here and would not present in the middle of such a burst.
                            //
                            // To better match the original presentation semantics, keep pumping the VM in the same
                            // redraw until it reaches a script-requested yield (WAIT/SLEEP/NEXT/etc.), or until a
                            // small drain limit is reached.
                            let max_drain_ticks: usize = std::env::var("RFVP_VM_DRAIN_TICKS")
                                .ok()
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(16);

                            let mut drain_ticks: usize = 0;
                            let mut rep = self.vm_worker.send_frame_ms_sync(frame_ms);
                            drain_ticks += 1;

                            while rep.forced_yield && drain_ticks < max_drain_ticks {
                                // Subsequent drains in the same redraw are zero-delta: timers already advanced.
                                rep = self.vm_worker.send_frame_ms_sync(0);
                                drain_ticks += 1;
                            }

                            // Apply WindowMode/Cursor requests that may have been issued during the VM tick.
                            self.apply_window_mode_requests();
                            self.update_cursor();

                            {
                                let mut gd = gd_write(&self.game_data);
                                self.layer_machine.apply_scene_action(SceneAction::EndFrame, &mut gd);
                            }
                            if let Err(e) = self.render_frame() {
                                log::error!("render_frame: {e:?}");
                            }

                            // Clear per-frame transient input signals only after the VM had a
                            // chance to observe them (InputGetDown/InputGetUp/InputGetRepeat/Wheel).
                            gd_write(&self.game_data).inputs_manager.frame_reset();
                        }
                        WindowEvent::KeyboardInput { event, .. } => {
                            if event.state == winit::event::ElementState::Pressed && !event.repeat {
                                match event.physical_key {
                                    PhysicalKey::Code(KeyCode::F2) => {
                                        self.toggle_hud_window();
                                    }
                                    PhysicalKey::Code(KeyCode::F11) => {
                                        // Fallback toggler (useful when a title does not expose a UI affordance
                                        // to return from fullscreen).
                                        let mut gd = gd_write(&self.game_data);
                                        let cur = gd.get_render_flag();
                                        let last = if self.last_fullscreen_flag == 2 || self.last_fullscreen_flag == 3 {
                                            self.last_fullscreen_flag
                                        } else {
                                            3
                                        };
                                        let next = if cur == 2 || cur == 3 { 0 } else { last };
                                        gd.set_render_flag(next);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    _ => {}
                    }
                    {
                        let mut gd = gd_write(&self.game_data);
                        update_input_events(
                            event,
                            &mut gd,
                            (self.surface_config.width, self.surface_config.height),
                            self.virtual_size,
                        );

                        // The script VM can block waiting for user input (e.g., in-game click-to-advance).
                        // In that case, waking the VM on input events is not sufficient: it must also be
                        // able to observe the edge-triggered state (Down/Up) immediately, without waiting
                        // for the next frame boundary.
                        if is_input_event {
                            gd.inputs_manager.refresh_input();
                        }
                    }
                    // Wake the VM immediately on user input so scripts that poll
                    // InputGetEvent/InputGetDown respond without waiting for the next frame.
                    if is_input_event {
                        self.vm_worker.send_input_signal();
                    }
                }
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if self
                    .hud_window
                    .as_ref()
                    .map(|w| w.id())
                    == Some(window_id) => {
                    match event {
                        WindowEvent::CloseRequested => {
                            self.set_hud_visible(false);
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            self.hud_cursor_pos = Some((position.x, position.y));
                        }
                        WindowEvent::CursorLeft { .. } => {
                            self.hud_cursor_pos = None;
                        }
                        WindowEvent::MouseInput { state, button, .. } => {
                            if *button == MouseButton::Left {
                                self.hud_pointer_down = state.is_pressed();
                            }
                        }
                        WindowEvent::MouseWheel { delta, .. } => {
                            // Store a per-frame scroll delta (in points). Roughly match egui's
                            // usual "line" scroll scale.
                            match delta {
                                MouseScrollDelta::LineDelta(_, y) => {
                                    self.hud_scroll_delta_y += *y * 24.0;
                                }
                                MouseScrollDelta::PixelDelta(pos) => {
                                    let ppp = self
                                        .hud_window
                                        .as_ref()
                                        .map(|w| w.scale_factor() as f32)
                                        .unwrap_or(1.0);
                                    self.hud_scroll_delta_y += (pos.y as f32) / ppp.max(0.5);
                                }
                            }
                        }
                        WindowEvent::Resized(physical_size) => {
                            if let (Some(surf), Some(cfg)) = (
                                self.hud_surface.as_ref(),
                                self.hud_surface_config.as_mut(),
                            ) {
                                cfg.width = physical_size.width.max(1);
                                cfg.height = physical_size.height.max(1);
                                surf.configure(&self.resources.device, cfg);
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            if self.hud_visible {
                                if let Err(e) = self.render_hud_frame() {
                                    log::error!("render_hud_frame: {e:?}");
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => {
                    // ExitMode(3): after the main script context exits, terminate the host loop.
                    let (should_exit, main_exited) = {
                        let gd = gd_read(&self.game_data);
                        (gd.get_game_should_exit(), gd.get_main_thread_exited())
                    };
                    if should_exit && main_exited {
                        loopd.exit();
                        return;
                    }

                    // Schedule the next redraw. This keeps the event loop responsive while
                    // avoiding a hard-coded FPS cap.
                    self.window.as_mut().unwrap().request_redraw();

                    if self.hud_visible {
                        if let Some(w) = self.hud_window.as_ref() {
                            w.request_redraw();
                        }
                    }
                }
                _ => (),
            }
        });
    }

    fn next_frame(&mut self) -> (u64, bool) {
        let mut notify_dissolve_done = false;
        let frame_ms: u64;

        {
            // Take the write lock once and never re-lock inside this scope.
            // IMPORTANT: avoid borrowing fields via the RwLockWriteGuard multiple times; always
            // project through a single &mut GameData binding.
            let mut gd_guard = gd_write(&self.game_data);
            let gd = &mut *gd_guard;

            let frame_duration = gd.time_mut_ref().frame();
            frame_ms = frame_duration.as_millis() as u64;

            let prev_dissolve = self.last_dissolve_type;
            let prev_dissolve2 = self.last_dissolve2_transitioning;

            // Movie update must run even when the VM/scheduler is halted for modal playback.
            let mut video_tick_failed = false;
            {
                let (video_manager, motion_manager) =
                    (&mut gd.video_manager, &mut gd.motion_manager);
                if let Err(e) = video_manager.tick(motion_manager) {
                    log::error!("VideoPlayerManager::tick failed: {:?}", e);
                    video_tick_failed = true;
                }
            }
            if video_tick_failed {
                {
                    let (video_manager, motion_manager) =
                        (&mut gd.video_manager, &mut gd.motion_manager);
                    video_manager.stop(motion_manager);
                }
                gd.set_halt(false);
            }

            let modal_movie = gd.video_manager.is_modal_active();

            if !modal_movie {
                self.layer_machine
                    .apply_scene_action(SceneAction::Update, gd);
                self.scheduler.execute(gd);
                self.layer_machine
                    .apply_scene_action(SceneAction::LateUpdate, gd);
            }

            // If a dissolve finished on this frame, wake contexts waiting on DISSOLVE_WAIT
            // on the VM thread. We only emit the event on the transition to None/Static.
            let cur_dissolve = gd.motion_manager.get_dissolve_type();
            if (prev_dissolve != DissolveType::None && prev_dissolve != DissolveType::Static)
                && (cur_dissolve == DissolveType::None || cur_dissolve == DissolveType::Static)
            {
                notify_dissolve_done = true;
            }

            // Dissolve2 completion should also wake contexts waiting on DISSOLVE_WAIT.
            let cur_dissolve2 = gd.motion_manager.is_dissolve2_transitioning();
            if prev_dissolve2 && !cur_dissolve2 {
                notify_dissolve_done = true;
            }
            self.last_dissolve2_transitioning = cur_dissolve2;
            self.last_dissolve_type = cur_dissolve;

            // Input state must be updated every frame (including during modal playback/halt),
            // otherwise InputGetDown/InputGetUp will remain stale while InputGetEvent continues
            // to receive queued events.
            gd.inputs_manager.refresh_input();
            gd.set_current_thread(0);

            if gd.get_halt() {
                // Preserve halt while a modal Movie is active.
                if !gd.video_manager.is_modal_active() {
                    gd.set_halt(false);
                }
            }

        }

        self.last_dt_ms = frame_ms as f32;
        self.debug_frame_no = self.debug_frame_no.wrapping_add(1);

        (frame_ms, notify_dissolve_done)
    }

    fn apply_window_mode_requests(&mut self) {
        use winit::window::Fullscreen;

        let requested = {
            let mut gd = gd_write(&self.game_data);
            gd.take_pending_render_flag()
        };
        let Some(flag) = requested else { return; };

        let w = self.window.as_ref().expect("A window is mandatory to run this game !");

        match flag {
            0 => {
                // Windowed
                w.set_fullscreen(None);
                w.set_decorations(true);
                w.set_maximized(false);

                if let Some(st) = self.windowed_restore.take() {
                    w.request_inner_size(st.size);
                    if let Some(pos) = st.pos {
                        let _ = w.set_outer_position(pos);
                    }
                }
            }
            1 => {
                // "Full window" (maximize), distinct from fullscreen.
                w.set_fullscreen(None);
                w.set_decorations(true);
                w.set_maximized(true);
            }
            2 | 3 => {
                // Fullscreen.
                self.last_fullscreen_flag = flag;

                // Capture current window bounds so we can restore them when returning to windowed.
                if self.windowed_restore.is_none() {
                    let size = w.inner_size();
                    let pos = w.outer_position().ok();
                    self.windowed_restore = Some(WindowedRestoreState { size, pos });
                }

                // Best-effort borderless fullscreen on the current monitor.
                let monitor = w.current_monitor();
                w.set_fullscreen(Some(Fullscreen::Borderless(monitor)));
            }
            _ => {
                // Unknown flags are ignored at the backend layer.
            }
        }
    }


    fn update_cursor(&mut self) {
        let cursor_frame = {
            let mut gd = gd_write(&self.game_data);
            gd.update_cursor()
        };
        let w = self.window.as_mut().expect("A window is mandatory to run this game !");
        if let Some(frame) = cursor_frame {
            w.set_cursor(frame);
        }
        {
            let mut gd = gd_write(&self.game_data);
            let mut window = gd.window_mut();
            window.reset_future_settings()
        }
    }

    fn render_frame(&mut self) -> anyhow::Result<()> {
        let dissolve_color: Option<glam::Vec4>;
        let dissolve2_color: Option<glam::Vec4>;
        {
            let gd = gd_read(&self.game_data);

            // Build primitive draw list and upload any modified GraphBuffs to the GPU.
            self.prim_renderer.rebuild(&self.resources, &gd.motion_manager);

            let frame_no = self.debug_frame_no;
            if crate::trace::should_dump_prim_tree(frame_no) {
                let tree = gd.motion_manager.debug_dump_prim_tree(
                    crate::trace::prim_tree_max_nodes(),
                    crate::trace::prim_tree_max_depth(),
                );
                crate::trace::dump(crate::trace::TraceKind::PrimTree, &format!("prim_tree frame={}", frame_no), &tree);
            }
            if crate::trace::should_dump_motion(frame_no) {
                let ms = gd.motion_manager.debug_dump_motion_state(crate::trace::motion_max());
                crate::trace::dump(crate::trace::TraceKind::Motion, &format!("motion frame={}", frame_no), &ms);
            }


            let dissolve_type = gd.motion_manager.get_dissolve_type();
            dissolve_color = if dissolve_type != DissolveType::None {
                let alpha = gd.motion_manager.get_dissolve_alpha();
                    crate::trace::motion(format_args!("Global dissolve alpha: {}", alpha));
                if alpha > 0.0 {
                    let cid = gd.motion_manager.get_dissolve_color_id() as u8;
                    let c = gd.motion_manager.color_manager.get_entry(cid);
                    Some(vec4(
                        c.get_r() as f32 / 255.0,
                        c.get_g() as f32 / 255.0,
                        c.get_b() as f32 / 255.0,
                        (c.get_a() as f32 / 255.0) * alpha,
                    ))
                } else {
                    None
                }
            } else {
                None
            };

            // Dissolve2 is a pure full-screen color fade used by engine-internal flows
            // (save/load/transition), rendered between root=0 and the overlay/custom root.
            let alpha2 = gd.motion_manager.get_dissolve2_alpha();
            dissolve2_color = if alpha2 > 0.0 {
                let cid = gd.motion_manager.get_dissolve2_color_id() as u8;
                let c = gd.motion_manager.color_manager.get_entry(cid);
                Some(vec4(
                    c.get_r() as f32 / 255.0,
                    c.get_g() as f32 / 255.0,
                    c.get_b() as f32 / 255.0,
                    (c.get_a() as f32 / 255.0) * alpha2,
                ))
            } else {
                None
            };
        }


        // Save thumbnail capture request (resolved after the virtual pass).
        let save_capture = {
            let gd = gd_read(&self.game_data);
            gd.save_manager.pending_save_capture()
        };

        let mut encoder = self
            .resources
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rfvp frame encoder"),
            });

        // Pass 1: render primitives into the virtual render target.
        {
            let mut pass = self
                .render_target
                .begin_srgb_render_pass(&mut encoder, Some("rfvp virtual pass"));

            let proj = self.render_target.projection_matrix();

            // Match original engine draw order:
            //   1) root=0 prim tree
            //   2) dissolve (mask/color)
            //   3) dissolve2 (full-screen color fade)
            //   4) overlay/custom root prim tree
            self.prim_renderer.draw_virtual_root0(&mut pass, &self.resources.pipelines.sprite, proj);

            let mk_fill_src = || VertexSource::VertexIndexBuffer {
                vertex_buffer: &self.dissolve_vertex_buffer,
                index_buffer: &self.dissolve_index_buffer,
                indices: 0..self.dissolve_num_indices,
                instances: 0..1,
            };

            // Global dissolve overlay (rendered in virtual space).
            if let Some(color) = dissolve_color {
                self.resources.pipelines.fill.draw(&mut pass, mk_fill_src(), proj, color);
            }

            // Engine dissolve2 overlay (rendered in virtual space).
            if let Some(color) = dissolve2_color {
                self.resources.pipelines.fill.draw(&mut pass, mk_fill_src(), proj, color);
            }

            self.prim_renderer.draw_virtual_overlay(&mut pass, &self.resources.pipelines.sprite, proj);
        }


        // If a SaveWrite is pending, capture the current virtual render target to CPU.
        let save_readback = save_capture.map(|_| {
            self.render_target
                .encode_readback_rgba8(&self.resources.device, &mut encoder)
        });

        // Pass 2: present to the swapchain with aspect-preserving scaling.
        let output = match self.surface.get_current_texture() {
            Ok(o) => o,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                // Recreate swapchain.
                self.surface.configure(&self.resources.device, &self.surface_config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => {
                // Skip a frame.
                return Ok(());
            }
            Err(e) => {
                return Err(e.into());
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rfvp present pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Present the virtual render target into the swapchain while preserving aspect ratio.
            // Coordinate system: origin at top-left, x right, y down.
            //
            // We first scale the virtual space into surface pixel space, then map surface pixels
            // into NDC via the same top-left-origin projection convention used in the virtual pass.
            let vw = self.virtual_size.0.max(1) as f32;
            let vh = self.virtual_size.1.max(1) as f32;
            let sw = self.surface_config.width.max(1) as f32;
            let sh = self.surface_config.height.max(1) as f32;

            // Two fullscreen presentation modes exist in the original engine:
            // - render_flag==2: stretch-to-fill (may distort aspect ratio)
            // - render_flag==3: keep-aspect (letterbox)
            // For windowed modes (0/1), keep-aspect matches typical behavior.
            let render_flag = gd_read(&self.game_data).get_render_flag();

            let (scale_x, scale_y, off_x, off_y) = if render_flag == 2 {
                (sw / vw, sh / vh, 0.0f32, 0.0f32)
            } else {
                let s = (sw / vw).min(sh / vh);
                let dst_w = vw * s;
                let dst_h = vh * s;
                ((s), (s), (sw - dst_w) * 0.5, (sh - dst_h) * 0.5)
            };

            let proj_surface = mat4(
                vec4(2.0 / sw, 0.0, 0.0, 0.0),
                vec4(0.0, -2.0 / sh, 0.0, 0.0),
                vec4(0.0, 0.0, 1.0, 0.0),
                vec4(-1.0, 1.0, 0.0, 1.0),
            );
            let to_surface_px = Mat4::from_translation(vec3(off_x, off_y, 0.0))
                * Mat4::from_scale(vec3(scale_x, scale_y, 1.0));

            let present_m = proj_surface * to_surface_px;

            self.resources.pipelines.sprite_screen.draw(
                &mut pass,
                self.render_target.vertex_source(),
                self.render_target.bind_group(),
                present_m,
            );
        }

        self.resources.queue.submit(Some(encoder.finish()));

        if let (Some(readback), Some((slot, thumb_w, thumb_h))) = (save_readback, save_capture) {
            let rgba = readback.map_to_rgba8(&self.resources.device);
            let src_w = self.virtual_size.0.max(1);
            let src_h = self.virtual_size.1.max(1);

            let thumb_rgba = if thumb_w > 0 && thumb_h > 0 && (thumb_w != src_w || thumb_h != src_h) {
                if let Some(img) = RgbaImage::from_raw(src_w, src_h, rgba.clone()) {
                    let resized = image::imageops::resize(&img, thumb_w, thumb_h, FilterType::Triangle);
                    resized.into_raw()
                } else {
                    rgba
                }
            } else {
                rgba.clone()
            };

            let mut gd = gd_write(&self.game_data);
            let nls = gd.get_nls();
            let state_snap = crate::subsystem::save_state::SaveStateSnapshotV1::capture(&gd);
            gd.save_manager.finalize_save_write(nls, thumb_w, thumb_h, &thumb_rgba, Some(&state_snap))?;
            gd.save_manager.consume_save_write_result();
        }

        output.present();

        Ok(())
    }



    fn set_hud_visible(&mut self, visible: bool) {
        self.hud_visible = visible;
        if let Some(w) = self.hud_window.as_ref() {
            w.set_visible(visible);
            if visible {
                w.request_redraw();
            }
        }
    }

    fn toggle_hud_window(&mut self) {
        let new_visible = !self.hud_visible;
        self.set_hud_visible(new_visible);
    }

    fn render_hud_frame(&mut self) -> anyhow::Result<()> {
        let (hud_surface, hud_cfg, hud_window) = match (
            self.hud_surface.as_ref(),
            self.hud_surface_config.as_ref(),
            self.hud_window.as_ref(),
        ) {
            (Some(s), Some(c), Some(w)) => (s, c, w),
            _ => return Ok(()),
        };

        let output = match hud_surface.get_current_texture() {
            Ok(o) => o,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                hud_surface.configure(&self.resources.device, hud_cfg);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .resources
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rfvp hud encoder"),
            });

        // Clear HUD window to black.
        {
            let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hud clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        if let Some(hud) = self.debug_hud.as_mut() {
            let snap = {
                let gd = gd_read(&self.game_data);
                let graphs = gd.motion_manager.graphs();

                let prim_tiles_enabled = std::env::var("RFVP_HUD_PRIM_TILES").as_deref() == Ok("1")
                    || std::env::var("RFVP_TRACE_PRIM_TILES").as_deref() == Ok("1");

                let prim_tiles = if prim_tiles_enabled {
                    self.prim_renderer.debug_tiles().to_vec()
                } else {
                    Vec::new()
                };

                // Debug-only: ensure that graphs referenced by prim tiles are uploaded so the HUD can
                // show real thumbnails (when CPU pixels exist).
                if !prim_tiles.is_empty() {
                    self.prim_renderer
                        .debug_force_upload_tiles(self.resources.as_ref(), graphs);
                }

                // Texture list: show state, not just file names. This helps identify "not loaded"
                // cases (no cpu pixels / not ready).
                let list_max: usize = std::env::var("RFVP_HUD_TEXTURE_LIST_MAX")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(200);
                let list_all = std::env::var("RFVP_HUD_TEXTURE_LIST_ALL").as_deref() == Ok("1");

                let mut textures: Vec<String> = Vec::new();
                textures.reserve(list_max.min(graphs.len()));

                let mut with_path = 0usize;
                let mut ready = 0usize;
                let mut cpu = 0usize;
                for (i, g) in graphs.iter().enumerate() {
                    if !g.texture_path.is_empty() {
                        with_path += 1;
                    }
                    if g.texture_ready {
                        ready += 1;
                    }
                    if g.texture.is_some() {
                        cpu += 1;
                    }

                    let interesting = g.texture_ready || g.texture.is_some() || !g.texture_path.is_empty();
                    if !list_all && !interesting {
                        continue;
                    }
                    if textures.len() >= list_max {
                        continue;
                    }
                    let path = if g.texture_path.is_empty() { "<none>" } else { g.texture_path.as_str() };
                    textures.push(format!(
                        "[{:04}] ready={} cpu={} gen={} size={}x{} path={}",
                        i,
                        if g.texture_ready { 1 } else { 0 },
                        if g.texture.is_some() { 1 } else { 0 },
                        g.generation,
                        g.width,
                        g.height,
                        path
                    ));
                }

                textures.insert(0, format!(
                    "graphs: total={} with_path={} ready={} cpu_img={} (set RFVP_HUD_TEXTURE_LIST_ALL=1 to show empty slots)",
                    graphs.len(), with_path, ready, cpu
                ));

                let text_lines = gd.motion_manager.text_manager.debug_lines();

                // Input summary (keys/mouse) to quickly diagnose "auto click" and key state.
                let input_line = {
                    const NAMES: [&str; 26] = [
                        "Shift", "Ctrl", "LClick", "RClick", "MouseL", "MouseR", "Esc", "Enter", "Space",
                        "Up", "Down", "Left", "Right",
                        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
                        "Tab",
                    ];

                    let fmt_bits = |bits: u32| -> String {
                        if bits == 0 {
                            return "-".to_string();
                        }
                        let mut parts: Vec<&'static str> = Vec::new();
                        for (i, name) in NAMES.iter().enumerate() {
                            if ((bits >> i) & 1) != 0 {
                                parts.push(*name);
                            }
                        }
                        if parts.is_empty() {
                            "-".to_string()
                        } else {
                            parts.join("|")
                        }
                    };

                    let im = &gd.inputs_manager;
                    let state = im.get_input_state();
                    let down = im.get_input_down();
                    let up = im.get_input_up();
                    let rep = im.get_repeat();
                    let wheel = im.get_wheel_value();
                    let cin = im.get_cursor_in();
                    let cx = im.get_cursor_x();
                    let cy = im.get_cursor_y();

                    format!(
                        "input: state=[{}]  down=[{}]  up=[{}]  repeat=0x{:08X}  cursor_in={}  cursor=({}, {})  wheel={}",
                        fmt_bits(state),
                        fmt_bits(down),
                        fmt_bits(up),
                        rep,
                        if cin { 1 } else { 0 },
                        cx,
                        cy,
                        wheel
                    )
                };

                HudSnapshot {
                    frame_no: self.debug_frame_no,
                    dt_ms: self.last_dt_ms,
                    input_line,
                    render: self.prim_renderer.stats(),
                    se: gd.se_player_ref().debug_summary(),
                    bgm: gd.bgm_player_ref().debug_summary(),
                    vm: gd.debug_vm_ref().clone(),
                    textures,
                    text_slots: gd.motion_manager.text_manager.debug_lines(),
                    text_lines,
                    prim_tiles,
                }
            };

            if !snap.prim_tiles.is_empty() {
                hud.sync_prim_tile_textures(&self.resources.device, &self.prim_renderer, &snap.prim_tiles);
            }


            let ws = hud_window.inner_size();
            let ppp = hud_window.scale_factor() as f32;
            let pointer_pos = self
                .hud_cursor_pos
                .map(|(x, y)| (x as f32 / ppp.max(0.5), y as f32 / ppp.max(0.5)));
            let inp = HudInput {
                pointer_pos,
                pointer_down: self.hud_pointer_down,
                scroll_delta_y: self.hud_scroll_delta_y,
            };
            // Scroll is a per-frame delta.
            self.hud_scroll_delta_y = 0.0;
            hud.prepare_frame((ws.width, ws.height), ppp, &snap, Some(inp));
            hud.render(&self.resources.device, &self.resources.queue, &mut encoder, &view);
        }

        self.resources.queue.submit(Some(encoder.finish()));

        output.present();

        Ok(())
    }
    pub fn find_hcb(game_path: impl AsRef<Path>) -> Result<PathBuf> {
        let mut path = game_path.as_ref().to_path_buf();
        path.push("*.hcb");

        let matches: Vec<_> = glob::glob(&path.to_string_lossy())?.flatten().collect();

        if matches.is_empty() {
            anyhow::bail!("No hcb file found in the game directory: {}", game_path.as_ref().display());
        }

        Ok(matches[0].to_path_buf())
    }
}

pub struct AppBuilder {
    config: AppConfig,
    scheduler: Scheduler,
    scene: Option<Box<dyn Scene>>,
    world: GameData,
    title: String,
    size: (u32, u32),
    parser: Parser,
    script_engine: ThreadManager,
}

impl AppBuilder {
    fn new(config: AppConfig) -> Self {
        let builder = Self {
            config,
            scheduler: Default::default(),
            scene: Default::default(),
            world: Default::default(),
            title: Default::default(),
            size: Default::default(),
            parser: Default::default(),
            script_engine: Default::default(),
        };
        builder
    }

    /// Specify a system to add to the scheduler.
    pub fn with_system(mut self, system: fn(&mut GameData)) -> Self {
        self.scheduler.add_system(system);
        self
    }

    /// Add a normal game layer to the pile. Every layer added before in the pile will be called
    pub fn with_scene<T: Scene + Default + 'static>(mut self) -> Self {
        self.scene = Some(Box::<T>::default());
        self
    }

    pub fn with_vfs(mut self, nls: Nls) -> anyhow::Result<Self> {
        self.world.vfs = crate::subsystem::resources::vfs::Vfs::new(nls)?;
        Ok(self)
    }

    pub fn with_window_title(mut self, title: &str) -> Self {
        self.title = title.to_owned();
        self
    }

    pub fn with_window_size(mut self, size: (u32, u32)) -> Self {
        self.size = size;
        self
    }

    pub fn with_script_engine(mut self, script_engine: ThreadManager) -> Self {
        self.script_engine = script_engine;
        self
    }

    pub fn with_parser(mut self, parser: Parser) -> Self {
        self.parser = parser;
        self
    }

    async fn init_render(
        window: Arc<Window>,
        hud_window: Option<Arc<Window>>,
        virtual_size: (u32, u32),
    ) -> (
        Arc<GpuCommonResources>,
        RenderTarget,
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
        Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    ) {
        let size = window.inner_size();
        let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        // Main surface
        let surface = {
            let s = instance.create_surface(window.as_ref()).unwrap();
            unsafe { std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(s) }
        };

        // Optional HUD surface (same Instance/Adapter)
        let hud_surface: Option<wgpu::Surface<'static>> = hud_window.as_ref().map(|w| {
            let s = instance.create_surface(w.as_ref()).unwrap();
            unsafe { std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(s) }
        });

        let adapter = wgpu::util::initialize_adapter_from_env_or_default(
            &instance,
            Some(&surface),
        )
        .await
        .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::PUSH_CONSTANTS,
                    required_limits: wgpu::Limits {
                        max_push_constant_size: 256,
                        ..wgpu::Limits::downlevel_webgl2_defaults()
                            .using_resolution(adapter.limits())
                    },
                },
                Some(Path::new("wgpu_trace")),
            )
            .await
            .expect("Failed to create device");

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let hud_bundle = if let (Some(hs), Some(hw)) = (hud_surface, hud_window.as_ref()) {
            let hs_caps = hs.get_capabilities(&adapter);
            let hud_format = hs_caps
                .formats
                .iter()
                .copied()
                .find(|f| *f == swapchain_format)
                .unwrap_or(hs_caps.formats[0]);
            let hud_size = hw.inner_size();
            let hud_cfg = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: hud_format,
                width: hud_size.width.max(1),
                height: hud_size.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: hs_caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            hs.configure(&device, &hud_cfg);
            Some((hs, hud_cfg))
        } else {
            None
        };

        let bind_group_layouts = BindGroupLayouts::new(&device);
        let pipelines = Pipelines::new(&device, &bind_group_layouts, swapchain_format);

        let resources = Arc::new(GpuCommonResources {
            device,
            queue,
            render_buffer_size: RwLock::new(virtual_size),
            bind_group_layouts,
            pipelines,
        });

        let render_target = RenderTarget::new(
            &resources,
            virtual_size,
            Some("Window RenderTarget"),
        );

        (resources, render_target, surface, config, hud_bundle)
    }


    /// Builds, setups and runs the application, must be called at the end of the building process.
    pub fn run(mut self) {
        let event_loop = EventLoop::new().expect("Event loop could not be created");
        event_loop.set_control_flow(ControlFlow::Poll);

        let window_builder: WindowAttributes = self
            .config
            .window_config
            .clone()
            .expect("The window configuration has not been found")
            .into(&self.config);
        let window = event_loop.create_window(window_builder)
            .expect("An error occured while building the main game window");

        let window = Arc::new(window);

        // Expose a best-effort "fullscreen capable" flag to scripts via WindowMode(3).
        // The original engine checks platform capabilities; here we treat having a monitor as "capable".
        self.world.set_can_fullscreen(window.current_monitor().is_some());

        // Debug HUD window (created hidden, toggled via F2).
        let hud_window: Option<Arc<Window>> = if debug_ui::enabled() {
            let ms = window.inner_size();
            let attrs = WindowAttributes::default()
                .with_title("rfvp HUD")
                .with_inner_size(Size::Physical(PhysicalSize::new(ms.width.max(1), ms.height.max(1))))
                .with_resizable(true)
                .with_visible(false);
            let w = event_loop
                .create_window(attrs)
                .expect("An error occured while building the HUD window");
            Some(Arc::new(w))
        } else {
            None
        };

        self.add_late_internal_systems_to_schedule();

        // let renderer_state =
        //     futures::executor::block_on(RendererState::new(window.clone()));

        let (resources, render_target, surface, surface_config, hud_bundle) =
            futures::executor::block_on(AppBuilder::init_render(window.clone(), hud_window.clone(), self.size));

        let entry_point = self.parser.get_entry_point();
        let non_volatile_global_count = self.parser.get_non_volatile_global_count();
        let volatile_global_count = self.parser.get_volatile_global_count();
        GLOBAL
            .lock()
            .unwrap()
            .init_with(non_volatile_global_count, volatile_global_count);
        
        self.script_engine.start_main(entry_point);
        self.world.nls = self.parser.nls.clone();


        let mut cursor_table = HashMap::new();
        if let Ok(cursor_paths) = self.world.vfs.find_ani() {
            let re = Regex::new(r"^([a-zA-Z_]+)(\d+)$").unwrap();
            for path in &cursor_paths {
                // split cursor1.ani into `cursor` and `1`
                let filename = path
                    .file_stem()
                    .unwrap_or_default() 
                    .to_string_lossy();

                if let Some(caps) = re.captures(&filename) {
                    let prefix = caps[1].to_string();
                    let number = caps[2].to_string();
                    
                    if let Ok(index) = number.parse::<u32>() {
                        let file = File::open(path).unwrap();
                        if let Ok(cursor) = ani::Decoder::new(file).decode() {
                            let mut failed = false;
                            let mut sources = vec![];
                            for frame in &cursor.frames {
                                match icondir_to_custom_cursor(frame) {
                                    Ok(s) => {
                                        sources.push(s);
                                    }
                                    Err(e) => {
                                        log::error!("{:#?}", e);
                                        failed = true;
                                        break;
                                    }
                                }
                            }

                            if failed {
                                log::error!("Failed to load icon : {}", path.display());
                                continue;
                            }

                            let mut new_cursors = vec![];
                            for s in sources {
                                let c = event_loop.create_custom_cursor(s);
                                new_cursors.push(c);
                            }

                            let cb = CursorBundle {
                                animated_cursor: cursor,
                                frames: new_cursors,
                                current_frame: 0,
                                last_update: Instant::now(),
                            };

                            cursor_table.insert(index, cb);
                        }
                    }
                } else {
                    continue;
                }

            }
        }

        self.world.set_cursor_table(cursor_table);


        // Fullscreen quad used for dissolve overlays (virtual space, pixel coordinates).
        let (dissolve_vertex_buffer, dissolve_index_buffer, dissolve_num_indices) = {
            let w = self.size.0.max(1) as f32;
            let h = self.size.1.max(1) as f32;
            let vertices: [PosVertex; 4] = [
                PosVertex { position: vec3(0.0, 0.0, 0.0) },
                PosVertex { position: vec3(w, 0.0, 0.0) },
                PosVertex { position: vec3(w, h, 0.0) },
                PosVertex { position: vec3(0.0, h, 0.0) },
            ];
            let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

            let vb = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rfvp dissolve quad VB"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rfvp dissolve quad IB"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            (vb, ib, indices.len() as u32)
        };

        let game_data = Arc::new(RwLock::new(self.world));
        let debug_ring = log_ring::get().unwrap_or_else(|| log_ring::init(4096));
        let vm_worker = VmWorker::spawn(game_data.clone(), self.parser, self.script_engine);

        let (hud_surface, hud_surface_config) = match hud_bundle {
            Some((s, c)) => (Some(s), Some(c)),
            None => (None, None),
        };

        let hud_surface_format = hud_surface_config
            .as_ref()
            .map(|c| c.format)
            .unwrap_or(surface_config.format);

        let mut app = App {
            config: self.config,
            game_data,
            title: self.title,
            scheduler: self.scheduler,
            layer_machine: SceneMachine {
                current_scene: self.scene,
            },
            window: Some(window.clone()),
            windowed_restore: None,
            last_fullscreen_flag: 3,

            vm_worker,
            pending_vm_frame_ms: 0,
            pending_vm_frame_ms_valid: false,
            render_target,
            resources: resources.clone(),
            surface,
            surface_config,
            prim_renderer: GpuPrimRenderer::new(resources.clone(), self.size),
            virtual_size: self.size,
            render_tree: RenderTree::new(),
            dissolve_vertex_buffer,
            dissolve_index_buffer,
            dissolve_num_indices,
            last_dissolve_type: DissolveType::None,
            last_dissolve2_transitioning: false,
            debug_hud: if debug_ui::enabled() {
                Some(DebugHud::new(&resources.device, hud_surface_format, debug_ring.clone()))
            } else {
                None
            },
            hud_window: hud_window.clone(),
            hud_surface,
            hud_surface_config,
            hud_visible: false,
            debug_ring: debug_ring.clone(),
            debug_frame_no: 0,
            last_dt_ms: 0.0,
            hud_cursor_pos: None,
            hud_pointer_down: false,
            hud_scroll_delta_y: 0.0,
        };

        app.setup();
        app.run(event_loop);
    }

    fn add_late_internal_systems_to_schedule(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_find_hcb() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase"));

        let hcb_path = App::find_hcb(filepath).unwrap();
        log::info!("{:?}", hcb_path);
    }
}