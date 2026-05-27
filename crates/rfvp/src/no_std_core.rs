use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::font::Font;
use crate::host_api::{
    FatalErrorCode, HitProxyTable, PlatformCallbacks, RfvpAudio, RfvpClock, RfvpError, RfvpEvent,
    RfvpFile, RfvpFileInfo, RfvpFileSystem, RfvpHost, RfvpLogLevel, RfvpResult,
};
use crate::rendering::prim_commands::{render_motion_to_host, HostPrimRenderCache};
use crate::script::global::GLOBAL;
use crate::script::parser::{Nls, Parser};
use crate::subsystem::resources::text_manager::FontEnumerator;
use crate::subsystem::resources::vfs::Vfs;
use crate::subsystem::resources::window::Window;
use crate::subsystem::world::GameData;
use crate::vm_runner::VmRunner;

const MISSING_DEFAULT_FONT_MESSAGE: &str =
    "Required font file default.ttf was not found in the game directory.";
const INVALID_DEFAULT_FONT_MESSAGE: &str =
    "Required font file default.ttf exists but is not a valid TrueType/OpenType font.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfvpCoreConfig {
    pub virtual_width: u32,
    pub virtual_height: u32,
    pub max_pending_events: usize,
}

impl Default for RfvpCoreConfig {
    fn default() -> Self {
        Self {
            virtual_width: 800,
            virtual_height: 600,
            max_pending_events: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfvpBootConfig<'a> {
    pub asset_root: &'a str,
    pub hcb_extension: &'a str,
    pub max_hcb_bytes: usize,
    pub max_manifest_entries: usize,
    pub nls: Nls,
}

impl<'a> Default for RfvpBootConfig<'a> {
    fn default() -> Self {
        Self {
            asset_root: ".",
            hcb_extension: "hcb",
            max_hcb_bytes: 64 * 1024 * 1024,
            max_manifest_entries: 1024,
            nls: Nls::ShiftJIS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfvpTickResult {
    pub frame_index: u64,
    pub consumed_events: usize,
    pub elapsed_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfvpCoreRunState {
    NotBooted,
    Booted,
    BootFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfvpResourceEntry {
    pub path: String,
    pub info: RfvpFileInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfvpLoadedGame {
    pub asset_root: String,
    pub hcb_path: String,
    pub hcb_bytes: Vec<u8>,
    pub hcb_info: RfvpFileInfo,
    pub hcb_manifest: Vec<RfvpResourceEntry>,
}

pub struct RfvpCore {
    config: RfvpCoreConfig,
    pending_events: Vec<RfvpEvent>,
    frame_index: u64,
    last_tick_us: Option<u64>,
    quit_requested: bool,
    run_state: RfvpCoreRunState,
    loaded_game: Option<RfvpLoadedGame>,
    parser: Option<Parser>,
    game_data: GameData,
    vm_runner: Option<VmRunner>,
    render_cache: HostPrimRenderCache,
    hit_proxies: HitProxyTable,
    last_error: Option<RfvpError>,
    last_error_detail: Option<String>,
}

impl RfvpCore {
    pub fn new(config: RfvpCoreConfig) -> Self {
        Self {
            config,
            pending_events: Vec::new(),
            frame_index: 0,
            last_tick_us: None,
            quit_requested: false,
            run_state: RfvpCoreRunState::NotBooted,
            loaded_game: None,
            parser: None,
            game_data: GameData::default(),
            vm_runner: None,
            render_cache: HostPrimRenderCache::new(),
            hit_proxies: HitProxyTable::default(),
            last_error: None,
            last_error_detail: None,
        }
    }

    pub fn config(&self) -> RfvpCoreConfig {
        self.config
    }

    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn run_state(&self) -> RfvpCoreRunState {
        self.run_state
    }

    pub fn loaded_game(&self) -> Option<&RfvpLoadedGame> {
        self.loaded_game.as_ref()
    }

    pub fn last_error(&self) -> Option<RfvpError> {
        self.last_error
    }

    pub fn last_error_detail(&self) -> Option<&str> {
        self.last_error_detail.as_deref()
    }

    pub fn push_event(&mut self, event: RfvpEvent) -> RfvpResult<()> {
        if self.pending_events.len() >= self.config.max_pending_events {
            return Err(RfvpError::CapacityExceeded);
        }
        self.pending_events.push(event);
        Ok(())
    }

    pub fn clear_events(&mut self) {
        self.pending_events.clear();
    }

    pub fn boot<H: RfvpHost>(&mut self, host: &mut H, boot: RfvpBootConfig<'_>) -> RfvpResult<()> {
        match self.try_boot(host, boot) {
            Ok(()) => {
                self.run_state = RfvpCoreRunState::Booted;
                self.last_error = None;
                self.last_error_detail = None;
                Ok(())
            }
            Err(err) => {
                self.run_state = RfvpCoreRunState::BootFailed;
                self.last_error = Some(err);
                Err(err)
            }
        }
    }

    fn try_boot<H: RfvpHost>(&mut self, host: &mut H, boot: RfvpBootConfig<'_>) -> RfvpResult<()> {
        if boot.asset_root.as_bytes().iter().any(|b| *b == 0) || boot.asset_root.is_empty() {
            return Err(RfvpError::InvalidArgument);
        }
        if boot.hcb_extension.as_bytes().iter().any(|b| *b == 0) || boot.hcb_extension.is_empty() {
            return Err(RfvpError::InvalidArgument);
        }
        if boot.max_hcb_bytes == 0 || boot.max_manifest_entries == 0 {
            return Err(RfvpError::InvalidArgument);
        }

        host.log(
            RfvpLogLevel::Info,
            "rfvp no_std boot: scanning asset root for HCB",
        );

        let (hcb, hcb_manifest) = find_and_load_hcb(
            host,
            boot.asset_root,
            boot.hcb_extension,
            boot.max_hcb_bytes,
            boot.max_manifest_entries,
        )?;
        let parser = Parser::from_bytes(hcb.bytes.clone(), boot.nls).map_err(|err| {
            self.last_error_detail = Some(err.to_string());
            RfvpError::InvalidData
        })?;
        let default_font = load_required_default_font(host).map_err(|(err, detail)| {
            self.last_error_detail = Some(detail);
            err
        })?;
        let screen = parser.get_screen_size();
        GLOBAL.lock().map_err(|_| RfvpError::Backend)?.init_with(
            parser.get_non_volatile_global_count(),
            parser.get_volatile_global_count(),
        );

        let mut vfs = build_host_vfs(host, boot)?;
        vfs.add_loose_file(&hcb.path, hcb.bytes.clone());

        let mut game_data = GameData::default();
        game_data.fontface_manager = FontEnumerator::from_default_font(default_font);
        game_data.vfs = vfs;
        game_data.nls = boot.nls;
        game_data.set_window(Window::new(screen, 1.0));

        let mut vm_runner =
            VmRunner::new(crate::subsystem::resources::thread_manager::ThreadManager::new());
        vm_runner.start_main(parser.get_entry_point());
        host.log(
            RfvpLogLevel::Info,
            "rfvp no_std boot: parsed real HCB, initialized real GameData, and started VmRunner",
        );

        self.loaded_game = Some(RfvpLoadedGame {
            asset_root: boot.asset_root.to_string(),
            hcb_path: hcb.path.clone(),
            hcb_info: hcb.info,
            hcb_bytes: hcb.bytes,
            hcb_manifest: hcb_manifest
                .into_iter()
                .map(|(path, info)| RfvpResourceEntry { path, info })
                .collect(),
        });
        self.config.virtual_width = screen.0;
        self.config.virtual_height = screen.1;
        self.game_data = game_data;
        self.vm_runner = Some(vm_runner);
        self.parser = Some(parser);
        Ok(())
    }

    pub fn tick<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<RfvpTickResult> {
        let now = host.clock().ticks_us();
        let elapsed_us = match self.last_tick_us.replace(now) {
            Some(prev) => now.saturating_sub(prev),
            None => 0,
        };

        let consumed_events = self.pending_events.len();
        self.apply_pending_events_to_game_data();
        let mut quit_requested = false;
        for event in self.pending_events.drain(..) {
            if matches!(event, RfvpEvent::Quit) {
                quit_requested = true;
            }
        }
        if quit_requested {
            self.quit_requested = true;
            host.log(RfvpLogLevel::Info, "quit requested by host event");
        }

        crate::platform_time::set_host_time_us(now);
        host.audio().tick(elapsed_us)?;
        if let (Some(parser), Some(vm_runner)) = (self.parser.as_mut(), self.vm_runner.as_mut()) {
            let frame_time_ms = elapsed_us / 1_000;
            if let Err(err) = vm_runner.tick(&mut self.game_data, parser, frame_time_ms) {
                let message = err.to_string();
                host.log(RfvpLogLevel::Error, &message);
                self.last_error = Some(RfvpError::Unsupported);
                self.last_error_detail = Some(message);
                return Err(RfvpError::Unsupported);
            }
            self.flush_audio(host)?;
            self.render_game_frame(host)?;
        } else if self.run_state == RfvpCoreRunState::BootFailed {
            return Err(self.last_error.unwrap_or(RfvpError::InvalidData));
        }
        self.frame_index = self.frame_index.wrapping_add(1);

        Ok(RfvpTickResult {
            frame_index: self.frame_index,
            consumed_events,
            elapsed_us,
        })
    }

    pub fn render_empty_frame<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<()> {
        self.render_game_frame(host)
    }

    pub fn render_status_frame<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<()> {
        self.render_game_frame(host)
    }

    fn render_game_frame<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<()> {
        let frame = render_motion_to_host(
            host.renderer(),
            &mut self.render_cache,
            &self.game_data.motion_manager,
            (self.config.virtual_width, self.config.virtual_height),
        )?;
        self.hit_proxies = frame.hit_proxies;
        Ok(())
    }

    fn apply_pending_events_to_game_data(&mut self) {
        for event in &self.pending_events {
            match *event {
                RfvpEvent::PointerMove { x, y, in_screen } => {
                    self.game_data.inputs_manager.notify_mouse_move(x, y);
                    self.game_data.inputs_manager.set_mouse_in(in_screen);
                }
                RfvpEvent::PointerDown {
                    button: crate::host_api::PointerButton::Left,
                    ..
                } => {
                    self.game_data.inputs_manager.notify_mouse_down(
                        crate::subsystem::resources::input_manager::KeyCode::MouseLeft,
                    );
                }
                RfvpEvent::PointerUp {
                    button: crate::host_api::PointerButton::Left,
                    ..
                } => {
                    self.game_data.inputs_manager.notify_mouse_up(
                        crate::subsystem::resources::input_manager::KeyCode::MouseLeft,
                    );
                }
                RfvpEvent::Wheel { delta_y, .. } => {
                    self.game_data.inputs_manager.notify_mouse_wheel(delta_y);
                }
                _ => {}
            }
        }
        self.game_data.inputs_manager.begin_frame();
    }

    fn flush_audio<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<()> {
        let mut commands = Vec::new();
        self.game_data.audio_manager().drain_commands(&mut commands);
        for command in commands {
            match command {
                crate::rfvp_audio::AudioCommand::LoadEncoded { id, kind, bytes } => {
                    host.audio().load_encoded(id, kind, &bytes)?;
                }
                crate::rfvp_audio::AudioCommand::CreateStream { id, desc } => {
                    host.audio().create_stream(id, desc)?;
                }
                crate::rfvp_audio::AudioCommand::SubmitI16 { id, samples } => {
                    host.audio().submit_i16(id, &samples)?;
                }
                crate::rfvp_audio::AudioCommand::SubmitF32 { id, samples } => {
                    host.audio().submit_f32(id, &samples)?;
                }
                crate::rfvp_audio::AudioCommand::Play {
                    id,
                    params,
                    fade_in_ms,
                } => {
                    host.audio().play(id, params, fade_in_ms)?;
                }
                crate::rfvp_audio::AudioCommand::Stop { id, fade_ms } => {
                    host.audio().stop(id, fade_ms)?;
                }
                crate::rfvp_audio::AudioCommand::Pause { id } => {
                    host.audio().pause(id)?;
                }
                crate::rfvp_audio::AudioCommand::Resume { id } => {
                    host.audio().resume(id)?;
                }
                crate::rfvp_audio::AudioCommand::SetParams { id, params } => {
                    host.audio().set_params(id, params)?;
                }
                crate::rfvp_audio::AudioCommand::DestroyStream { id } => {
                    host.audio().destroy_stream(id);
                }
                crate::rfvp_audio::AudioCommand::MasterVolume { volume } => {
                    host.audio().set_master_volume(volume)?;
                }
            }
        }
        Ok(())
    }
}

fn load_required_default_font<H: RfvpHost>(host: &mut H) -> Result<Font, (RfvpError, String)> {
    let callbacks = host.platform_callbacks();
    let mut bytes = Vec::new();
    if host
        .fs()
        .read_required_file("default.ttf", &mut bytes)
        .is_err()
    {
        notify_fatal(
            callbacks,
            FatalErrorCode::MissingDefaultFont,
            MISSING_DEFAULT_FONT_MESSAGE,
        );
        return Err((
            RfvpError::NotFound,
            MISSING_DEFAULT_FONT_MESSAGE.to_string(),
        ));
    }
    match Font::from_vec(bytes) {
        Ok(font) => Ok(font),
        Err(_) => {
            notify_fatal(
                callbacks,
                FatalErrorCode::InvalidDefaultFont,
                INVALID_DEFAULT_FONT_MESSAGE,
            );
            Err((
                RfvpError::InvalidData,
                INVALID_DEFAULT_FONT_MESSAGE.to_string(),
            ))
        }
    }
}

fn notify_fatal(callbacks: PlatformCallbacks, code: FatalErrorCode, message: &str) {
    if let Some(callback) = callbacks.fatal_error {
        callback(callbacks.user_data, code, message.as_ptr(), message.len());
    }
}

fn find_and_load_hcb<H: RfvpHost>(
    host: &mut H,
    root: &str,
    extension: &str,
    max_hcb_bytes: usize,
    max_manifest_entries: usize,
) -> RfvpResult<(LoadedHcb, Vec<(String, RfvpFileInfo)>)> {
    let mut found = Vec::new();
    {
        let visitor = &mut |path: &str, info: RfvpFileInfo| -> RfvpResult<()> {
            if found.len() >= max_manifest_entries {
                return Err(RfvpError::CapacityExceeded);
            }
            found.push((path.to_string(), info));
            Ok(())
        };
        host.fs().enumerate_by_extension(root, extension, visitor)?;
    }
    let Some((path, info)) = found.first().cloned() else {
        return Err(RfvpError::NotFound);
    };
    let mut file = host.fs().open(&path)?;
    let bytes = file.read_to_vec(max_hcb_bytes)?;
    let manifest = found.into_iter().skip(1).collect();
    Ok((LoadedHcb { path, bytes, info }, manifest))
}

struct LoadedHcb {
    path: String,
    bytes: Vec<u8>,
    info: RfvpFileInfo,
}

fn build_host_vfs<H: RfvpHost>(host: &mut H, boot: RfvpBootConfig<'_>) -> RfvpResult<Vfs> {
    let mut vfs = Vfs::new(boot.nls).map_err(|_| RfvpError::InvalidData)?;
    let mut packs = Vec::new();
    {
        let visitor = &mut |path: &str, info: RfvpFileInfo| -> RfvpResult<()> {
            if info.kind == crate::host_api::RfvpFileKind::File {
                packs.push(path.to_string());
            }
            Ok(())
        };
        host.fs()
            .enumerate_by_extension(boot.asset_root, "bin", visitor)?;
    }
    for path in packs {
        let mut file = host.fs().open(&path)?;
        let bytes = file.read_to_vec(usize::MAX)?;
        let folder = path
            .rsplit('/')
            .next()
            .unwrap_or(path.as_str())
            .strip_suffix(".bin")
            .unwrap_or(path.as_str());
        vfs.add_pack_bytes(folder, bytes).map_err(|err| {
            host.log(
                RfvpLogLevel::Warn,
                &format!("failed to parse host pack {path}: {err}"),
            );
            RfvpError::InvalidData
        })?;
    }
    Ok(vfs)
}
