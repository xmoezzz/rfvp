//! Platform-neutral OS/UEFI-style host runtime for `rfvp-os` builds.
//!
//! This module does not own a desktop window, GPU surface, or audio device.
//! A UEFI or bare OS host maps native input into [`RfvpOsEvent`], advances
//! [`RfvpOsRuntime`], and copies the returned [`SoftFramebuffer`] into its own
//! display target.

/// Calibrate the UEFI platform clock (TSC on x86_64, CNTPCT_EL0 on aarch64)
/// and set the PCIe ECAM base for the HDA audio driver (aarch64 only).
///
/// Must be called once at UEFI startup, before any [`RfvpOsRuntime::new`] call
/// or any use of `Instant::now()` / `SystemTime::now()`.  Has no effect on
/// non-UEFI targets.
#[cfg(target_os = "uefi")]
pub fn init_uefi_platform() {
    crate::platform_time::calibrate_uefi_clock();

    // On aarch64 the HDA driver needs the PCIe ECAM base address.
    // Only probe when anzu-audio is enabled; skip the ACPI walk otherwise.
    #[cfg(all(target_arch = "aarch64", feature = "anzu-audio"))]
    if let Some(ecam) = find_pcie_ecam_base() {
        anzu_hal::set_pcie_ecam_base(ecam);
    }
}

/// Parse the ACPI MCFG table to find the PCIe ECAM base address.
#[cfg(all(target_os = "uefi", target_arch = "aarch64", feature = "anzu-audio"))]
fn find_pcie_ecam_base() -> Option<u64> {
    use uefi::table::cfg::ConfigTableEntry;

    uefi::system::with_config_table(|tables| {
        let rsdp_ptr = tables
            .iter()
            .find(|t| t.guid == ConfigTableEntry::ACPI2_GUID)
            .map(|t| t.address as *const u8)?;

        // Safety: UEFI guarantees this pointer is valid for the duration of boot services.
        unsafe {
            // RSDP v2 layout (bytes):
            //   0..8  signature ("RSD PTR ")
            //   8     checksum
            //   9..15 OEM ID
            //   15    revision
            //   16..20 RSDT address (32-bit)
            //   20..24 length
            //   24..32 XSDT address (64-bit)
            let xsdt_addr = core::ptr::read_unaligned(rsdp_ptr.add(24) as *const u64);
            if xsdt_addr == 0 {
                return None;
            }

            // XSDT header: sig(4) + length(4) + ... entries start at byte 36
            let xsdt_len = core::ptr::read_unaligned((xsdt_addr + 4) as *const u32) as usize;
            let entry_count = xsdt_len.saturating_sub(36) / 8;
            let entries_base = (xsdt_addr + 36) as *const u64;

            for i in 0..entry_count {
                let entry_addr = core::ptr::read_unaligned(entries_base.add(i));
                if entry_addr == 0 {
                    continue;
                }
                let sig = core::ptr::read(entry_addr as *const [u8; 4]);
                if &sig == b"MCFG" {
                    // MCFG: header(36) + reserved(8) + first ECAM entry
                    // ECAM entry layout: base_addr(8) + seg(2) + start_bus(1) + end_bus(1) + rsvd(4)
                    let base = core::ptr::read_unaligned((entry_addr + 44) as *const u64);
                    log::info!("rfvp: ACPI MCFG PCIe ECAM base = 0x{:016x}", base);
                    return Some(base);
                }
            }
            None
        }
    })
}

#[cfg(not(target_os = "uefi"))]
pub fn init_uefi_platform() {}

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[cfg(not(target_os = "uefi"))]
use std::path::{Path, PathBuf};

use anyhow::Result as AnyResult;

use crate::{
    script::{global::GLOBAL, parser::Nls},
    soft_render::{
        create_soft_renderer, PixelFormat, SoftFramebuffer, SoftRenderError, SoftRenderer,
    },
    subsystem::{
        anzu_scene::AnzuScene,
        resources::{
            input_manager::{InputManager, KeyCode},
            motion_manager::{DissolveType, MotionManager},
            thread_manager::ThreadManager,
            vfs::Vfs,
            window::Window as EngineWindow,
        },
        scene::{SceneAction, SceneMachine},
        scheduler::Scheduler,
        world::GameData,
    },
    utils::file::set_base_path,
    vm_worker::VmWorker,
};

#[cfg(target_os = "uefi")]
const UEFI_VERBOSE_STAGE_LOGS: bool = false;

#[cfg(target_os = "uefi")]
macro_rules! uefi_stage {
    ($($arg:tt)*) => {
        if UEFI_VERBOSE_STAGE_LOGS {
            log::info!($($arg)*);
        }
    };
}

#[cfg(not(target_os = "uefi"))]
macro_rules! uefi_stage {
    ($($arg:tt)*) => {};
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfvpOsKey {
    Shift,
    Ctrl,
    Esc,
    Enter,
    Space,
    Up,
    Down,
    Left,
    Right,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Tab,
}

impl RfvpOsKey {
    fn keycode(self) -> KeyCode {
        match self {
            Self::Shift => KeyCode::Shift,
            Self::Ctrl => KeyCode::Ctrl,
            Self::Esc => KeyCode::Esc,
            Self::Enter => KeyCode::Enter,
            Self::Space => KeyCode::Space,
            Self::Up => KeyCode::UpArrow,
            Self::Down => KeyCode::DownArrow,
            Self::Left => KeyCode::LeftArrow,
            Self::Right => KeyCode::RightArrow,
            Self::F1 => KeyCode::F1,
            Self::F2 => KeyCode::F2,
            Self::F3 => KeyCode::F3,
            Self::F4 => KeyCode::F4,
            Self::F5 => KeyCode::F5,
            Self::F6 => KeyCode::F6,
            Self::F7 => KeyCode::F7,
            Self::F8 => KeyCode::F8,
            Self::F9 => KeyCode::F9,
            Self::F10 => KeyCode::F10,
            Self::F11 => KeyCode::F11,
            Self::F12 => KeyCode::F12,
            Self::Tab => KeyCode::Tab,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfvpOsPointerButton {
    Left,
    Right,
}

impl RfvpOsPointerButton {
    fn keycode(self) -> KeyCode {
        match self {
            Self::Left => KeyCode::MouseLeft,
            Self::Right => KeyCode::MouseRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RfvpOsEvent {
    KeyDown { key: RfvpOsKey, repeat: bool },
    KeyUp { key: RfvpOsKey },
    PointerMove { x: i32, y: i32, in_screen: bool },
    PointerDown { button: RfvpOsPointerButton },
    PointerUp { button: RfvpOsPointerButton },
    Wheel { delta: i32 },
}

#[derive(Debug, Clone)]
pub struct RfvpOsConfig {
    #[cfg(not(target_os = "uefi"))]
    pub project_dir: String,
    #[cfg(target_os = "uefi")]
    pub project_dir: &'static str,
    #[cfg(target_os = "uefi")]
    pub hcb_path: Option<&'static str>,
    #[cfg(target_os = "uefi")]
    pub hcb_bytes: Option<Vec<u8>>,
    pub nls: Nls,
    pub screen_size: Option<(u32, u32)>,
    pub system_font: bool,
}

impl RfvpOsConfig {
    #[cfg(not(target_os = "uefi"))]
    pub fn new(project_dir: impl Into<String>, nls: Nls) -> Self {
        Self {
            project_dir: project_dir.into(),
            nls,
            screen_size: None,
            system_font: false,
        }
    }

    #[cfg(target_os = "uefi")]
    pub fn new(project_dir: &'static str, nls: Nls) -> Self {
        Self {
            project_dir,
            hcb_path: None,
            hcb_bytes: None,
            nls,
            screen_size: None,
            system_font: false,
        }
    }

    #[cfg(target_os = "uefi")]
    pub fn with_hcb_path(mut self, hcb_path: &'static str) -> Self {
        self.hcb_path = Some(hcb_path);
        self
    }

    #[cfg(target_os = "uefi")]
    pub fn with_hcb_bytes(mut self, hcb_bytes: Vec<u8>) -> Self {
        self.hcb_bytes = Some(hcb_bytes);
        self
    }

    pub fn with_screen_size(mut self, width: u32, height: u32) -> Self {
        self.screen_size = Some((width.max(1), height.max(1)));
        self
    }

    pub fn with_system_font(mut self, enabled: bool) -> Self {
        self.system_font = enabled;
        self
    }
}

pub struct RfvpOsRuntime {
    game_data: Arc<RwLock<Box<GameData>>>,
    vm_worker: VmWorker,
    scheduler: Scheduler,
    layer_machine: SceneMachine,
    renderer: SoftRenderer,
    virtual_size: (u32, u32),
    last_dissolve_type: DissolveType,
    last_dissolve2_transitioning: bool,
    #[cfg(target_os = "uefi")]
    uefi_render_log_frames: usize,
}

impl RfvpOsRuntime {
    pub fn new(mut config: RfvpOsConfig) -> AnyResult<Self> {
        uefi_stage!("[UEFI] RfvpOsRuntime::new entered");
        uefi_stage!("[UEFI] before set_base_path");
        set_base_path(&config.project_dir);
        uefi_stage!("[UEFI] after set_base_path");

        uefi_stage!("[UEFI] before find_hcb");
        let hcb_path = find_hcb(&config)?;
        uefi_stage!("[UEFI] after find_hcb path={:?}", hcb_path);
        uefi_stage!("[UEFI] before Parser::new");
        #[cfg(not(target_os = "uefi"))]
        let mut parser = crate::script::parser::Parser::new(hcb_path, config.nls)?;
        #[cfg(target_os = "uefi")]
        let mut parser = {
            let hcb_bytes = config.hcb_bytes.take().ok_or_else(|| {
                anyhow::anyhow!("No UEFI hcb bytes were provided for hcb path: {}", hcb_path)
            })?;
            crate::script::parser::Parser::from_bytes(hcb_bytes, config.nls)?
        };
        uefi_stage!("[UEFI] after Parser::new");
        uefi_stage!("[UEFI] before parser.get_screen_size");
        let virtual_size = parser.get_screen_size();
        uefi_stage!(
            "[UEFI] after parser.get_screen_size virtual={}x{}",
            virtual_size.0,
            virtual_size.1
        );
        let screen_size = config.screen_size.unwrap_or(virtual_size);

        uefi_stage!("[UEFI] before boxed_default_game_data");
        let mut world = boxed_default_game_data();
        uefi_stage!("[UEFI] after boxed_default_game_data");
        uefi_stage!("[UEFI] before Vfs::new");
        world.vfs = Vfs::new(config.nls)?;
        uefi_stage!("[UEFI] after Vfs::new");
        world.nls = parser.nls;
        world.set_can_fullscreen(false);
        uefi_stage!("[UEFI] before EngineWindow::new");
        world.set_window(EngineWindow::new(screen_size, 1.0));
        uefi_stage!("[UEFI] after EngineWindow::new");
        if config.system_font {
            world
                .fontface_manager
                .set_system_font_fallback_enabled(true);
        }
        uefi_stage!("[UEFI] before fontface_manager.init_fontface");
        #[cfg(not(target_os = "uefi"))]
        if let Err(e) = world.fontface_manager.init_fontface() {
            log::error!("Failed to scan font directory: {:#}", e);
        }
        #[cfg(target_os = "uefi")]
        {
            log::info!("[UEFI] skipping host font directory scan");
        }
        uefi_stage!("[UEFI] after fontface_manager.init_fontface");
        uefi_stage!("[UEFI] before try_load_global_savedata_v1");
        #[cfg(not(target_os = "uefi"))]
        if let Err(e) = crate::subsystem::global_savedata::try_load_global_savedata_v1(&mut world) {
            log::error!("Failed to load global savedata: {:#}", e);
        }
        #[cfg(target_os = "uefi")]
        {
            log::info!("[UEFI] skipping host global savedata load");
        }
        uefi_stage!("[UEFI] after try_load_global_savedata_v1");

        uefi_stage!("[UEFI] before GLOBAL init");
        GLOBAL.lock().unwrap().init_with(
            parser.get_non_volatile_global_count(),
            parser.get_volatile_global_count(),
        );
        uefi_stage!("[UEFI] after GLOBAL init");

        uefi_stage!("[UEFI] before ThreadManager::new");
        let mut script_engine = ThreadManager::new();
        uefi_stage!("[UEFI] after ThreadManager::new");
        uefi_stage!("[UEFI] before script_engine.start_main");
        script_engine.start_main(parser.get_entry_point());
        uefi_stage!("[UEFI] after script_engine.start_main");

        uefi_stage!("[UEFI] before SceneMachine init");
        let mut layer_machine = SceneMachine {
            current_scene: Some(Box::<AnzuScene>::default()),
        };
        uefi_stage!("[UEFI] after SceneMachine init");
        uefi_stage!("[UEFI] before SceneAction::Start");
        layer_machine.apply_scene_action(SceneAction::Start, &mut world);
        uefi_stage!("[UEFI] after SceneAction::Start");

        uefi_stage!("[UEFI] before Arc<RwLock<GameData>>");
        let game_data = Arc::new(RwLock::new(world));
        uefi_stage!("[UEFI] after Arc<RwLock<GameData>>");
        uefi_stage!("[UEFI] before VmWorker::spawn");
        let vm_worker = VmWorker::spawn(game_data.clone(), parser.clone(), script_engine);
        uefi_stage!("[UEFI] after VmWorker::spawn");
        uefi_stage!("[UEFI] before create_soft_renderer");
        let renderer = create_soft_renderer(
            virtual_size.0.max(1),
            virtual_size.1.max(1),
            PixelFormat::Rgba8,
        )?;
        uefi_stage!("[UEFI] after create_soft_renderer");

        Ok(Self {
            game_data,
            vm_worker,
            scheduler: Scheduler::default(),
            layer_machine,
            renderer,
            virtual_size,
            last_dissolve_type: DissolveType::None,
            last_dissolve2_transitioning: false,
            #[cfg(target_os = "uefi")]
            uefi_render_log_frames: 0,
        })
    }

    pub fn virtual_size(&self) -> (u32, u32) {
        self.virtual_size
    }

    pub fn framebuffer(&self) -> &SoftFramebuffer {
        self.renderer.framebuffer()
    }

    pub fn resize_screen(&mut self, width: u32, height: u32) {
        gd_write(&self.game_data)
            .window_mut()
            .set_dimensions(width.max(1), height.max(1));
    }

    pub fn push_event(&mut self, event: RfvpOsEvent) {
        let mut gd = gd_write(&self.game_data);
        Self::apply_input_event(&mut gd.inputs_manager, event);
    }

    pub fn should_exit(&self) -> bool {
        let gd = gd_read(&self.game_data);
        gd.get_lock_scripter() && gd.get_main_thread_exited()
    }

    pub fn step_frame(&mut self) -> AnyResult<&SoftFramebuffer> {
        uefi_stage!("[UEFI] step_frame before next_frame");
        let (frame_ms, notify_dissolve_done) = self.next_frame();
        uefi_stage!(
            "[UEFI] step_frame after next_frame frame_ms={} notify_dissolve_done={}",
            frame_ms,
            notify_dissolve_done
        );
        if notify_dissolve_done {
            uefi_stage!("[UEFI] step_frame before send_dissolve_done_sync");
            self.vm_worker.send_dissolve_done_sync();
            uefi_stage!("[UEFI] step_frame after send_dissolve_done_sync");
        }
        uefi_stage!("[UEFI] step_frame before send_frame_ms_sync");
        let _ = self.vm_worker.send_frame_ms_sync(frame_ms);
        uefi_stage!("[UEFI] step_frame after send_frame_ms_sync");

        {
            uefi_stage!("[UEFI] step_frame before SceneAction::EndFrame");
            let mut gd = gd_write(&self.game_data);
            self.layer_machine
                .apply_scene_action(SceneAction::EndFrame, &mut gd);
            uefi_stage!("[UEFI] step_frame after SceneAction::EndFrame");
        }

        {
            uefi_stage!("[UEFI] step_frame before render_frame");
            let gd = gd_read(&self.game_data);
            self.renderer.render_frame(&gd.motion_manager)?;
            #[cfg(target_os = "uefi")]
            {
                let frame_index = self.uefi_render_log_frames;
                if false && (frame_index < 8 || frame_index % 120 == 0) {
                    let stats = self.renderer.stats();
                    let motion = &gd.motion_manager;
                    let prim_manager = motion.prim_manager();
                    log::info!(
                        "[UEFI] soft render frame={} draw_calls={} quads={} root={} custom_root={} graphs={} dissolve={:?} dissolve_alpha={} dissolve_color_id={} dissolve2_mode={} dissolve2_alpha={} dissolve2_color_id={}",
                        self.uefi_render_log_frames,
                        stats.draw_calls,
                        stats.quad_count,
                        0,
                        prim_manager.get_custom_root_prim_id(),
                        motion.graphs().len(),
                        motion.get_dissolve_type(),
                        motion.get_dissolve_alpha(),
                        motion.get_dissolve_color_id(),
                        motion.get_dissolve2_mode(),
                        motion.get_dissolve2_alpha(),
                        motion.get_dissolve2_color_id()
                    );
                }
                self.uefi_render_log_frames = self.uefi_render_log_frames.saturating_add(1);
            }
            uefi_stage!("[UEFI] step_frame after render_frame");
        }

        uefi_stage!("[UEFI] step_frame before inputs frame_reset");
        gd_write(&self.game_data).inputs_manager.frame_reset();
        uefi_stage!("[UEFI] step_frame after inputs frame_reset");
        Ok(self.renderer.framebuffer())
    }

    pub fn apply_input_event(input: &mut InputManager, event: RfvpOsEvent) {
        match event {
            RfvpOsEvent::KeyDown { key, repeat } => {
                input.notify_keycode_down(key.keycode(), repeat);
            }
            RfvpOsEvent::KeyUp { key } => {
                input.notify_keycode_up(key.keycode());
            }
            RfvpOsEvent::PointerMove { x, y, in_screen } => {
                input.notify_mouse_move(x, y);
                input.set_mouse_in(in_screen);
            }
            RfvpOsEvent::PointerDown { button } => {
                input.notify_mouse_down(button.keycode());
            }
            RfvpOsEvent::PointerUp { button } => {
                input.notify_mouse_up(button.keycode());
            }
            RfvpOsEvent::Wheel { delta } => {
                input.notify_mouse_wheel(delta);
            }
        }
    }

    fn next_frame(&mut self) -> (u64, bool) {
        uefi_stage!("[UEFI] next_frame entered");
        let mut notify_dissolve_done = false;
        let frame_ms: u64;

        {
            uefi_stage!("[UEFI] next_frame before game_data write lock");
            let mut gd_guard = gd_write(&self.game_data);
            uefi_stage!("[UEFI] next_frame after game_data write lock");
            let gd = &mut *gd_guard;
            gd.motion_manager.text_manager.set_render_scale(1.0);

            uefi_stage!("[UEFI] next_frame before time frame");
            let frame_duration = gd.time_mut_ref().frame();
            let frame_us = frame_duration.as_micros() as u64;
            frame_ms = if frame_us == 0 {
                0
            } else {
                (frame_us + 999) / 1000
            };
            uefi_stage!("[UEFI] next_frame after time frame frame_ms={}", frame_ms);
            uefi_stage!("[UEFI] next_frame before timer tick");
            gd.timer_manager.tick(frame_ms.min(u32::MAX as u64) as u32);
            uefi_stage!("[UEFI] next_frame after timer tick");
            #[cfg(feature = "anzu-audio")]
            gd.audio_manager()
                .tick(frame_ms.min(u32::MAX as u64) as u32);
            uefi_stage!("[UEFI] next_frame before inputs begin_frame");
            gd.inputs_manager.begin_frame();
            uefi_stage!("[UEFI] next_frame after inputs begin_frame");

            let mut video_tick_failed = false;
            {
                uefi_stage!("[UEFI] next_frame before video tick");
                let (video_manager, motion_manager) =
                    (&mut gd.video_manager, &mut gd.motion_manager);
                if let Err(e) = video_manager.tick(motion_manager) {
                    log::error!("VideoPlayerManager::tick failed: {:?}", e);
                    video_tick_failed = true;
                }
                uefi_stage!("[UEFI] next_frame after video tick");
            }
            if video_tick_failed {
                uefi_stage!("[UEFI] next_frame before video stop");
                let (video_manager, motion_manager) =
                    (&mut gd.video_manager, &mut gd.motion_manager);
                video_manager.stop(motion_manager);
                gd.set_halt(false);
                uefi_stage!("[UEFI] next_frame after video stop");
            }

            let prev_dissolve = self.last_dissolve_type;
            let prev_dissolve2 = self.last_dissolve2_transitioning;
            let modal_movie = gd.video_manager.is_modal_active();

            if !modal_movie {
                uefi_stage!("[UEFI] next_frame before SceneAction::Update");
                self.layer_machine
                    .apply_scene_action(SceneAction::Update, gd);
                uefi_stage!("[UEFI] next_frame after SceneAction::Update");
                uefi_stage!("[UEFI] next_frame before scheduler execute 1");
                self.scheduler.execute(gd);
                uefi_stage!("[UEFI] next_frame after scheduler execute 1");
                uefi_stage!("[UEFI] next_frame before scheduler execute 2");
                self.scheduler.execute(gd);
                uefi_stage!("[UEFI] next_frame after scheduler execute 2");
                uefi_stage!("[UEFI] next_frame before SceneAction::LateUpdate");
                self.layer_machine
                    .apply_scene_action(SceneAction::LateUpdate, gd);
                uefi_stage!("[UEFI] next_frame after SceneAction::LateUpdate");
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

/// Compatibility wrapper kept for existing rfvp-os smoke checks.
pub struct RfvpOsHost {
    renderer: SoftRenderer,
}

impl RfvpOsHost {
    pub fn new(width: u32, height: u32) -> std::result::Result<Self, SoftRenderError> {
        Ok(Self {
            renderer: create_soft_renderer(width, height, PixelFormat::Rgba8)?,
        })
    }

    pub fn framebuffer(&self) -> &SoftFramebuffer {
        self.renderer.framebuffer()
    }

    pub fn render_frame(
        &mut self,
        motion: &MotionManager,
    ) -> std::result::Result<&SoftFramebuffer, SoftRenderError> {
        self.renderer.render_frame(motion)
    }
}

#[derive(Debug, Clone)]
struct RfvpOsArgs {
    project_dir: Option<String>,
    nls: Nls,
    width: u32,
    height: u32,
    system_font: bool,
}

impl Default for RfvpOsArgs {
    fn default() -> Self {
        Self {
            project_dir: None,
            nls: Nls::ShiftJIS,
            width: 320,
            height: 240,
            system_font: false,
        }
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn run_from_args() -> AnyResult<()> {
    let args = parse_args();
    let Some(project_dir) = args.project_dir else {
        let host = RfvpOsHost::new(args.width, args.height)?;
        let fb = host.framebuffer();
        println!(
            "rfvp-os initialized: {}x{} stride={} format={:?} bytes={}",
            fb.width(),
            fb.height(),
            fb.stride(),
            fb.format(),
            fb.pixels().len()
        );
        println!("Pass --project-dir to run the real rfvp-os engine loop.");
        return Ok(());
    };

    let mut runtime = RfvpOsRuntime::new(
        RfvpOsConfig::new(project_dir, args.nls)
            .with_screen_size(args.width, args.height)
            .with_system_font(args.system_font),
    )?;

    for _ in 0..3 {
        let fb = runtime.step_frame()?;
        println!(
            "rfvp-os frame: {}x{} stride={} format={:?} bytes={}",
            fb.width(),
            fb.height(),
            fb.stride(),
            fb.format(),
            fb.pixels().len()
        );
        if runtime.should_exit() {
            break;
        }
    }
    Ok(())
}

fn parse_args() -> RfvpOsArgs {
    let mut out = RfvpOsArgs::default();
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--project-dir" => {
                if let Some(v) = args.get(i + 1) {
                    if !v.is_empty() {
                        out.project_dir = Some(v.clone());
                    }
                    i += 1;
                }
            }
            "--nls" => {
                if let Some(v) = args.get(i + 1) {
                    out.nls = v.parse().unwrap_or_else(|e| {
                        eprintln!("rfvp: {e}");
                        std::process::exit(1);
                    });
                    i += 1;
                }
            }
            "--width" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse::<u32>().ok()) {
                    out.width = v.max(1);
                    i += 1;
                }
            }
            "--height" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse::<u32>().ok()) {
                    out.height = v.max(1);
                    i += 1;
                }
            }
            "--system-font" => out.system_font = true,
            _ => {
                if let Some(v) = args[i].strip_prefix("--project-dir=") {
                    if !v.is_empty() {
                        out.project_dir = Some(v.to_string());
                    }
                } else if let Some(v) = args[i].strip_prefix("--nls=") {
                    out.nls = v.parse().unwrap_or_else(|e| {
                        eprintln!("rfvp: {e}");
                        std::process::exit(1);
                    });
                } else if let Some(v) = args[i].strip_prefix("--width=") {
                    if let Ok(v) = v.parse::<u32>() {
                        out.width = v.max(1);
                    }
                } else if let Some(v) = args[i].strip_prefix("--height=") {
                    if let Ok(v) = v.parse::<u32>() {
                        out.height = v.max(1);
                    }
                }
            }
        }
        i += 1;
    }
    out
}

fn boxed_default_game_data() -> Box<GameData> {
    uefi_stage!("[UEFI] boxed_default_game_data before Box::new_uninit");
    let mut boxed: Box<std::mem::MaybeUninit<GameData>> = Box::new_uninit();
    uefi_stage!("[UEFI] boxed_default_game_data after Box::new_uninit");
    unsafe {
        uefi_stage!("[UEFI] boxed_default_game_data before init_default_in_place");
        GameData::init_default_in_place(boxed.as_mut_ptr().cast());
        uefi_stage!("[UEFI] boxed_default_game_data after init_default_in_place");
        let raw: *mut GameData = Box::into_raw(boxed).cast();
        uefi_stage!("[UEFI] boxed_default_game_data before Box::from_raw");
        Box::from_raw(raw)
    }
}

#[cfg(not(target_os = "uefi"))]
fn find_hcb(config: &RfvpOsConfig) -> AnyResult<PathBuf> {
    let game_path = Path::new(&config.project_dir);

    for entry in std::fs::read_dir(game_path)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();

        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("hcb"))
        {
            return Ok(path);
        }
    }

    anyhow::bail!(
        "No hcb file found in the game directory: {}",
        game_path.display()
    );
}

#[cfg(target_os = "uefi")]
fn find_hcb(config: &RfvpOsConfig) -> AnyResult<&'static str> {
    if let Some(hcb_path) = &config.hcb_path {
        return Ok(*hcb_path);
    }

    anyhow::bail!(
        "No UEFI hcb path was provided for the game directory: {}",
        config.project_dir
    );
}
