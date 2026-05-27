//! Desktop host loop for the CPU software renderer.
//!
//! This module is intentionally outside `soft_render`: it owns windowing,
//! input dispatch, and CPU framebuffer presentation, while `soft_render`
//! stays platform-independent.

use std::{
    collections::HashMap,
    fs::File,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use crate::platform_time::Instant;
use anyhow::{Context, Result};
use regex::Regex;
use winit::event_loop::OwnedDisplayHandle;
use winit::{
    dpi::{PhysicalSize, Size},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use crate::{
    script::{global::GLOBAL, parser::Nls},
    soft_render::{create_soft_renderer, PixelFormat, SoftFramebuffer, SoftRenderer},
    subsystem::{
        anzu_scene::AnzuScene,
        event_handler::update_input_events,
        resources::{
            motion_manager::DissolveType, thread_manager::ThreadManager, vfs::Vfs,
            window::Window as EngineWindow,
        },
        scene::{SceneAction, SceneMachine},
        scheduler::Scheduler,
        world::GameData,
    },
    utils::{
        ani::{self, icondir_to_custom_cursor, CursorBundle},
        file::set_base_path,
        logger::Logger,
    },
    vm_worker::VmWorker,
};

#[inline]
fn gd_read<'a>(gd: &'a Arc<RwLock<Box<GameData>>>) -> RwLockReadGuard<'a, Box<GameData>> {
    match gd.read() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[inline]
fn gd_write<'a>(gd: &'a Arc<RwLock<Box<GameData>>>) -> RwLockWriteGuard<'a, Box<GameData>> {
    match gd.write() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[derive(Debug, Clone)]
struct SoftHostArgs {
    project_dir: Option<String>,
    nls: Nls,
    system_font: bool,
    window_size: Option<(u32, u32)>,
}

impl Default for SoftHostArgs {
    fn default() -> Self {
        Self {
            project_dir: None,
            nls: Nls::ShiftJIS,
            system_font: false,
            window_size: None,
        }
    }
}

pub fn run_from_args() -> Result<()> {
    run(parse_args())
}

fn run(args: SoftHostArgs) -> Result<()> {
    if let Some(project_dir) = args.project_dir.as_deref() {
        set_base_path(project_dir);
    }

    let hcb_path = find_hcb(crate::utils::file::app_base_path().get_path())?;
    if let Some(parent) = hcb_path.parent() {
        if let Some(parent) = parent.to_str() {
            set_base_path(parent);
            crate::utils::file::set_hcb_root_path(parent);
        }
    }
    let mut parser = crate::script::parser::Parser::new(hcb_path, args.nls)?;
    let title = parser.get_title();
    let virtual_size = parser.get_screen_size();

    Logger::init_logging(Some(crate::config::logger_config::LoggerConfig {
        app_level_filter: log::LevelFilter::Info,
        level_filter: log::LevelFilter::Debug,
    }));
    log::info!("Starting soft-render host");

    let event_loop = EventLoop::new().context("Event loop could not be created")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let window_size = args.window_size.unwrap_or(virtual_size);
    let window_attrs = WindowAttributes::default()
        .with_title(title.clone())
        .with_inner_size(Size::Physical(PhysicalSize::new(
            window_size.0.max(1),
            window_size.1.max(1),
        )))
        .with_resizable(true);
    let window = Arc::new(
        event_loop
            .create_window(window_attrs)
            .context("An error occurred while building the soft-render window")?,
    );

    let context = softbuffer::Context::new(event_loop.owned_display_handle())
        .map_err(|e| anyhow::anyhow!("create softbuffer context: {e:?}"))?;
    let mut surface = softbuffer::Surface::new(&context, window.clone())
        .map_err(|e| anyhow::anyhow!("create softbuffer surface: {e:?}"))?;

    let mut host = SoftHost::new(
        &event_loop,
        &mut parser,
        title,
        virtual_size,
        window.clone(),
        args,
    )?;
    let _result = event_loop.run(move |event, loopd| {
        loopd.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => {
                    loopd.exit();
                }
                WindowEvent::Resized(size) => {
                    host.resize_window(size.width, size.height);
                    window.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    if let Err(e) =
                        host.step_and_present(&mut surface, window.as_ref(), window.inner_size())
                    {
                        log::error!("soft-render frame failed: {e:?}");
                        loopd.exit();
                    }
                }
                event => {
                    host.handle_window_event(&event, window.inner_size());
                }
            },
            Event::AboutToWait => {
                if host.should_exit() {
                    loopd.exit();
                    return;
                }
                window.request_redraw();
            }
            _ => {}
        }
    });

    Ok(())
}

struct SoftHost {
    game_data: Arc<RwLock<Box<GameData>>>,
    vm_worker: VmWorker,
    scheduler: Scheduler,
    layer_machine: SceneMachine,
    renderer: SoftRenderer,
    virtual_size: (u32, u32),
    last_dissolve_type: DissolveType,
    last_dissolve2_transitioning: bool,
}

impl SoftHost {
    fn new(
        event_loop: &EventLoop<()>,
        parser: &mut crate::script::parser::Parser,
        _title: String,
        virtual_size: (u32, u32),
        window: Arc<Window>,
        args: SoftHostArgs,
    ) -> Result<Self> {
        let mut world = boxed_default_game_data();
        world.vfs = Vfs::new(args.nls)?;
        world.nls = parser.nls;
        world.set_can_fullscreen(false);
        world.set_window(EngineWindow::new(
            (window.inner_size().width, window.inner_size().height),
            window.scale_factor(),
        ));
        if args.system_font {
            world
                .fontface_manager
                .set_system_font_fallback_enabled(true);
        }
        if let Err(e) = world.fontface_manager.init_fontface() {
            log::error!("Failed to scan font directory: {:#}", e);
        }
        if let Err(e) = crate::subsystem::global_savedata::try_load_global_savedata_v1(&mut world) {
            log::error!("Failed to load global savedata: {:#}", e);
        }

        let cursor_table = load_ani_cursor_table(event_loop, &world.vfs);
        log::info!("soft-render ANI cursor table size: {}", cursor_table.len());
        world.set_cursor_table(cursor_table);
        if world.has_cursor(1) {
            world.set_current_cursor_index(0);
            world.switch_cursor(1);
        } else {
            world.set_current_cursor_index(0);
            world.window_mut().set_cursor_kind(0);
        }

        GLOBAL.lock().unwrap().init_with(
            parser.get_non_volatile_global_count(),
            parser.get_volatile_global_count(),
        );

        let mut script_engine = ThreadManager::new();
        script_engine.start_main(parser.get_entry_point());

        let mut layer_machine = SceneMachine {
            current_scene: Some(Box::<AnzuScene>::default()),
        };
        layer_machine.apply_scene_action(SceneAction::Start, &mut world);

        let game_data = Arc::new(RwLock::new(world));
        let vm_worker = VmWorker::spawn(game_data.clone(), parser.clone(), script_engine);

        Ok(Self {
            game_data,
            vm_worker,
            scheduler: Scheduler::default(),
            layer_machine,
            renderer: create_soft_renderer(
                virtual_size.0.max(1),
                virtual_size.1.max(1),
                PixelFormat::Rgba8,
            )?,
            virtual_size,
            last_dissolve_type: DissolveType::None,
            last_dissolve2_transitioning: false,
        })
    }

    fn resize_window(&mut self, width: u32, height: u32) {
        let mut gd = gd_write(&self.game_data);
        gd.window_mut().set_dimensions(width.max(1), height.max(1));
    }

    fn handle_window_event(&mut self, event: &WindowEvent, window_size: PhysicalSize<u32>) {
        let mut gd = gd_write(&self.game_data);
        update_input_events(
            event,
            &mut gd,
            (window_size.width.max(1), window_size.height.max(1)),
            self.virtual_size,
        );
    }

    fn should_exit(&self) -> bool {
        let gd = gd_read(&self.game_data);
        gd.get_lock_scripter() && gd.get_main_thread_exited()
    }

    fn step_and_present(
        &mut self,
        surface: &mut softbuffer::Surface<OwnedDisplayHandle, Arc<Window>>,
        window: &Window,
        window_size: PhysicalSize<u32>,
    ) -> Result<()> {
        let (frame_ms, notify_dissolve_done) = self.next_frame();
        if notify_dissolve_done {
            self.vm_worker.send_dissolve_done_sync();
        }
        let _ = self.vm_worker.send_frame_ms_sync(frame_ms);

        {
            let mut gd = gd_write(&self.game_data);
            self.layer_machine
                .apply_scene_action(SceneAction::EndFrame, &mut gd);
        }

        {
            let gd = gd_read(&self.game_data);
            self.renderer.render_frame(&gd.motion_manager)?;
        }

        present_framebuffer(
            surface,
            self.renderer.framebuffer(),
            window_size.width.max(1),
            window_size.height.max(1),
        )?;

        self.update_cursor(window, window_size);

        gd_write(&self.game_data).inputs_manager.frame_reset();
        Ok(())
    }

    fn virtual_to_window_cursor_pos(
        &self,
        vx: i32,
        vy: i32,
        window_size: PhysicalSize<u32>,
    ) -> winit::dpi::PhysicalPosition<f64> {
        let sw = window_size.width.max(1) as f64;
        let sh = window_size.height.max(1) as f64;
        let vw = self.virtual_size.0.max(1) as f64;
        let vh = self.virtual_size.1.max(1) as f64;
        let vx = vx.clamp(0, (vw as i32).saturating_sub(1)) as f64;
        let vy = vy.clamp(0, (vh as i32).saturating_sub(1)) as f64;
        let scale = (sw / vw).min(sh / vh);
        let dst_w = vw * scale;
        let dst_h = vh * scale;
        let off_x = (sw - dst_w) * 0.5;
        let off_y = (sh - dst_h) * 0.5;
        winit::dpi::PhysicalPosition::new(off_x + (vx + 0.5) * scale, off_y + (vy + 0.5) * scale)
    }

    fn update_cursor(&mut self, window: &Window, window_size: PhysicalSize<u32>) {
        let (cursor_frame, pending_cursor_kind, pending_cursor_visible, pending_cursor_pos) = {
            let mut gd = gd_write(&self.game_data);
            let frame = gd.update_cursor();
            let cursor_kind = *gd.window_ref().new_cursor();
            let visible = gd.window_ref().new_cursor_visible();
            let pos = gd.window_ref().new_cursor_pos();
            (frame, cursor_kind, visible, pos)
        };

        if let Some(frame) = cursor_frame {
            window.set_cursor(frame);
        } else if let Some(icon) = pending_cursor_kind {
            window.set_cursor(icon);
        }
        if let Some(visible) = pending_cursor_visible {
            window.set_cursor_visible(visible);
        }
        if let Some((vx, vy)) = pending_cursor_pos {
            let pos = self.virtual_to_window_cursor_pos(vx, vy, window_size);
            let _ = window.set_cursor_position(pos);
        }

        gd_write(&self.game_data)
            .window_mut()
            .reset_future_settings();
    }

    fn next_frame(&mut self) -> (u64, bool) {
        let mut notify_dissolve_done = false;
        let frame_ms: u64;

        {
            let mut gd_guard = gd_write(&self.game_data);
            let gd = &mut *gd_guard;
            gd.motion_manager.text_manager.set_render_scale(1.0);

            let frame_duration = gd.time_mut_ref().frame();
            let frame_us = frame_duration.as_micros() as u64;
            frame_ms = if frame_us == 0 {
                0
            } else {
                (frame_us + 999) / 1000
            };
            gd.timer_manager.tick(frame_ms.min(u32::MAX as u64) as u32);
            gd.inputs_manager.begin_frame();

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
                let (video_manager, motion_manager) =
                    (&mut gd.video_manager, &mut gd.motion_manager);
                video_manager.stop(motion_manager);
                gd.set_halt(false);
            }

            let prev_dissolve = self.last_dissolve_type;
            let prev_dissolve2 = self.last_dissolve2_transitioning;
            let modal_movie = gd.video_manager.is_modal_active();

            if !modal_movie {
                self.layer_machine
                    .apply_scene_action(SceneAction::Update, gd);
                self.scheduler.execute(gd);
                self.layer_machine
                    .apply_scene_action(SceneAction::LateUpdate, gd);
            }

            let cur_dissolve = gd.motion_manager.get_dissolve_type();
            if (prev_dissolve != DissolveType::None && prev_dissolve != DissolveType::Static)
                && (cur_dissolve == DissolveType::None || cur_dissolve == DissolveType::Static)
            {
                notify_dissolve_done = true;
            }

            let cur_dissolve2 = gd.motion_manager.is_dissolve2_transitioning();
            if prev_dissolve2 && !cur_dissolve2 {
                notify_dissolve_done = true;
            }
            self.last_dissolve2_transitioning = cur_dissolve2;
            self.last_dissolve_type = cur_dissolve;

            gd.set_current_thread(0);
            if gd.get_halt() && !gd.video_manager.is_modal_active() {
                gd.set_halt(false);
            }
        }

        (frame_ms, notify_dissolve_done)
    }
}

fn present_framebuffer(
    surface: &mut softbuffer::Surface<OwnedDisplayHandle, Arc<Window>>,
    framebuffer: &SoftFramebuffer,
    surface_width: u32,
    surface_height: u32,
) -> Result<()> {
    let sw = surface_width.max(1);
    let sh = surface_height.max(1);
    surface
        .resize(NonZeroU32::new(sw).unwrap(), NonZeroU32::new(sh).unwrap())
        .map_err(|e| anyhow::anyhow!("resize softbuffer surface: {e:?}"))?;

    let mut buffer = surface
        .buffer_mut()
        .map_err(|e| anyhow::anyhow!("borrow softbuffer buffer: {e:?}"))?;
    buffer.fill(0);

    let vw = framebuffer.width().max(1);
    let vh = framebuffer.height().max(1);
    let scale = ((sw as f32) / (vw as f32)).min((sh as f32) / (vh as f32));
    let dst_w = ((vw as f32) * scale).round().max(1.0) as u32;
    let dst_h = ((vh as f32) * scale).round().max(1.0) as u32;
    let off_x = (sw - dst_w) / 2;
    let off_y = (sh - dst_h) / 2;
    let pixels = framebuffer.pixels();
    let stride = framebuffer.stride() as usize;

    for y in 0..dst_h {
        let sy = ((y as f32) / scale).floor() as u32;
        let sy = sy.min(vh - 1);
        let src_row = sy as usize * stride;
        let dst_row = (off_y + y) as usize * sw as usize;
        for x in 0..dst_w {
            let sx = ((x as f32) / scale).floor() as u32;
            let sx = sx.min(vw - 1);
            let src = src_row + sx as usize * 4;
            let r = pixels[src] as u32;
            let g = pixels[src + 1] as u32;
            let b = pixels[src + 2] as u32;
            buffer[dst_row + (off_x + x) as usize] = (r << 16) | (g << 8) | b;
        }
    }

    buffer
        .present()
        .map_err(|e| anyhow::anyhow!("present softbuffer buffer: {e:?}"))?;
    Ok(())
}

fn load_ani_cursor_table(event_loop: &EventLoop<()>, vfs: &Vfs) -> HashMap<u32, CursorBundle> {
    let mut cursor_table = HashMap::new();
    let Ok(cursor_paths) = vfs.find_ani() else {
        return cursor_table;
    };
    let re = match Regex::new(r"^([a-zA-Z_]+)(\d+)$") {
        Ok(re) => re,
        Err(e) => {
            log::error!("Failed to build ANI cursor filename regex: {e:#}");
            return cursor_table;
        }
    };

    for path in &cursor_paths {
        let filename = path.file_stem().unwrap_or_default().to_string_lossy();
        let Some(caps) = re.captures(&filename) else {
            continue;
        };
        let Ok(index) = caps[2].parse::<u32>() else {
            continue;
        };
        let Ok(file) = File::open(path) else {
            log::error!("Failed to open cursor: {}", path.display());
            continue;
        };
        let Ok(cursor) = ani::Decoder::new(file).decode() else {
            log::error!("Failed to decode ANI cursor: {}", path.display());
            continue;
        };

        let mut frames = Vec::new();
        let mut failed = false;
        for frame in &cursor.frames {
            match icondir_to_custom_cursor(frame) {
                Ok(source) => frames.push(event_loop.create_custom_cursor(source)),
                Err(e) => {
                    log::error!(
                        "Failed to create cursor frame for {}: {e:#}",
                        path.display()
                    );
                    failed = true;
                    break;
                }
            }
        }
        if failed || frames.is_empty() {
            continue;
        }

        log::info!(
            "loaded soft-render ANI cursor slot {} from {}",
            index,
            path.display()
        );
        cursor_table.insert(
            index,
            CursorBundle {
                animated_cursor: cursor,
                frames,
                current_frame: 0,
                last_update: Instant::now(),
            },
        );
    }

    cursor_table
}

fn boxed_default_game_data() -> Box<GameData> {
    let mut boxed: Box<std::mem::MaybeUninit<GameData>> = Box::new_uninit();
    unsafe {
        GameData::init_default_in_place(boxed.as_mut_ptr().cast());
        let raw: *mut GameData = Box::into_raw(boxed).cast();
        Box::from_raw(raw)
    }
}

fn find_hcb(game_path: impl AsRef<Path>) -> Result<PathBuf> {
    let mut path = game_path.as_ref().to_path_buf();
    path.push("*.hcb");

    let matches: Vec<_> = glob::glob(&path.to_string_lossy())?.flatten().collect();
    if matches.is_empty() {
        anyhow::bail!(
            "No hcb file found in the game directory: {}",
            game_path.as_ref().display()
        );
    }

    Ok(matches[0].to_path_buf())
}

fn parse_args() -> SoftHostArgs {
    let mut args_out = SoftHostArgs::default();
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if let Some(value) = arg.strip_prefix("--project-dir=") {
            if !value.is_empty() {
                args_out.project_dir = Some(value.to_string());
            }
        } else if arg == "--project-dir" {
            if let Some(value) = args.get(i + 1) {
                if !value.is_empty() {
                    args_out.project_dir = Some(value.to_string());
                }
                i += 1;
            }
        } else if let Some(value) = arg.strip_prefix("--nls=") {
            args_out.nls = parse_nls(value);
        } else if arg == "--nls" {
            if let Some(value) = args.get(i + 1) {
                args_out.nls = parse_nls(value);
                i += 1;
            } else {
                eprintln!("rfvp: --nls requires a value (sjis, gbk, utf8)");
                std::process::exit(1);
            }
        } else if arg == "--system-font" {
            args_out.system_font = true;
        } else if let Some(value) = arg.strip_prefix("--width=") {
            width = Some(parse_dimension("--width", value));
        } else if arg == "--width" {
            if let Some(value) = args.get(i + 1) {
                width = Some(parse_dimension("--width", value));
                i += 1;
            }
        } else if let Some(value) = arg.strip_prefix("--height=") {
            height = Some(parse_dimension("--height", value));
        } else if arg == "--height" {
            if let Some(value) = args.get(i + 1) {
                height = Some(parse_dimension("--height", value));
                i += 1;
            }
        }
        i += 1;
    }

    if width.is_some() || height.is_some() {
        args_out.window_size = Some((width.unwrap_or(1280).max(1), height.unwrap_or(720).max(1)));
    }
    args_out
}

fn parse_nls(value: &str) -> Nls {
    value.parse().unwrap_or_else(|e| {
        eprintln!("rfvp: {e}");
        std::process::exit(1);
    })
}

fn parse_dimension(name: &str, value: &str) -> u32 {
    value.parse::<u32>().unwrap_or_else(|e| {
        eprintln!("rfvp: invalid {name} value {value:?}: {e}");
        std::process::exit(2);
    })
}
