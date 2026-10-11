use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::font::Font;
use crate::host_api::{
    FatalErrorCode, HitProxyTable, PlatformCallbacks, RfvpAudio, RfvpClock, RfvpError, RfvpEvent,
    RfvpFile, RfvpFileInfo, RfvpFileSystem, RfvpHost, RfvpLogLevel, RfvpRenderer, RfvpResult,
};
use crate::rendering::prim_commands::{render_motion_to_host, HostPrimRenderCache};
use crate::script::global::GLOBAL;
use crate::script::parser::{Nls, Parser};
use crate::subsystem::anzu_scene::AnzuScene;
use crate::subsystem::resources::text_manager::FontEnumerator;
use crate::subsystem::resources::vfs::Vfs;
use crate::subsystem::resources::window::Window;
use crate::subsystem::world::GameData;
use crate::vm_runner::VmRunner;
#[cfg(feature = "old_school")]
use core_maths::CoreFloat;

const MISSING_DEFAULT_FONT_MESSAGE: &str =
    "Required font file default.ttf was not found in the game directory.";
const INVALID_DEFAULT_FONT_MESSAGE: &str =
    "Required font file default.ttf exists but is not a valid TrueType/OpenType font.";
#[cfg(feature = "psx")]
const MISSING_PSX_FONT_MESSAGE: &str =
    "font.psxf is missing or damaged: convert the game with rfvp-psx-convert";
#[cfg(feature = "old_school")]
const MISSING_OLD_SCHOOL_FONT_MESSAGE: &str =
    "Required font file defualt.tmap was not found in the game directory.";
#[cfg(feature = "old_school")]
const INVALID_OLD_SCHOOL_FONT_MESSAGE: &str =
    "Required font file defualt.tmap exists but is not a valid bitmap font.";
#[cfg(feature = "old_school")]
const MISSING_OLD_SCHOOL_CONFIG_MESSAGE: &str =
    "Required old-school config file rfvp.toml was not found in the game directory.";
#[cfg(feature = "old_school")]
const INVALID_OLD_SCHOOL_CONFIG_MESSAGE: &str =
    "Required old-school config file rfvp.toml is invalid: expected exactly one top-level float field, scale.";

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
    /// Memory for the script (`old_school`): a script up to this size is read whole, a larger
    /// one is read in 4 KiB pages through a cache this large. Scripts jump about a lot, and on
    /// a console every page brought in again is a file read.
    pub hcb_cache_bytes: usize,
}

impl<'a> Default for RfvpBootConfig<'a> {
    fn default() -> Self {
        Self {
            asset_root: ".",
            hcb_extension: "hcb",
            max_hcb_bytes: 64 * 1024 * 1024,
            max_manifest_entries: 1024,
            nls: Nls::ShiftJIS,
            hcb_cache_bytes: 32 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfvpTickResult {
    pub frame_index: u64,
    pub consumed_events: usize,
    pub elapsed_us: u64,
    /// Time this tick spent running scripts, updating motions and text, and drawing (presenting
    /// included), for the ports' debug reports.
    pub vm_us: u64,
    pub scene_us: u64,
    pub render_us: u64,
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
    /// Microseconds not yet handed to the scripts (they count whole milliseconds).
    carry_us: u64,
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
    /// Hash of the global save as last written, so an unchanged one is not written again.
    last_global_save_hash: Option<u64>,
    /// Time since the global save was last looked at.
    us_since_global_save: u64,
}

/// How often the global save (read-text marks, flags) is written during play. Consoles are often
/// left through the home button or by power, so it cannot wait for a clean exit.
const GLOBAL_SAVE_INTERVAL_US: u64 = 120_000_000;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl RfvpCore {
    pub fn new(config: RfvpCoreConfig) -> Self {
        Self {
            config,
            pending_events: Vec::new(),
            frame_index: 0,
            last_tick_us: None,
            carry_us: 0,
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
            last_global_save_hash: None,
            us_since_global_save: 0,
        }
    }

    /// Rasterize text at `scale` times the game's virtual size: a host whose screen shows the
    /// game larger than its virtual size (the PS2's 640-pixel picture of a 480-wide repack)
    /// gets text drawn at the screen's own resolution instead of enlarged and blurred.
    pub fn set_text_render_scale(&mut self, scale: f32) {
        self.game_data
            .motion_manager
            .text_manager
            .set_render_scale(scale);
    }

    /// One line per live script thread (id, state, program counter, wait/sleep ms), for a
    /// host's debug report.
    pub fn debug_threads(&self) -> String {
        let mut out = String::new();
        let Some(vm) = self.vm_runner.as_ref() else {
            return out;
        };
        for ctx in &vm.thread_manager().contexts {
            let state = ctx.get_status();
            if state.is_empty() {
                continue;
            }
            out += &format!(
                "thread {}: {:?} pc {:#x} wait {} sleep {}\n",
                ctx.get_id(),
                state,
                ctx.get_pc(),
                ctx.get_waiting_time(),
                ctx.get_sleeping_time()
            );
        }
        out
    }

    /// The script VM's state, for tests.
    #[cfg(test)]
    pub(crate) fn vm_state(
        &self,
    ) -> Option<crate::subsystem::resources::thread_manager::ThreadManagerSnapshotV1> {
        self.vm_runner
            .as_ref()
            .map(|vm| vm.thread_manager().capture_snapshot_v1())
    }

    /// The game state, for tests that drive the engine directly.
    #[cfg(test)]
    pub(crate) fn game_data_mut(&mut self) -> &mut GameData {
        &mut self.game_data
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

    pub fn boot<H: RfvpHost>(&mut self, host: &mut H, boot: RfvpBootConfig<'_>) -> RfvpResult<()>
    where
        <<H as RfvpHost>::FileSystem as RfvpFileSystem>::File: 'static,
    {
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

    fn try_boot<H: RfvpHost>(&mut self, host: &mut H, boot: RfvpBootConfig<'_>) -> RfvpResult<()>
    where
        <<H as RfvpHost>::FileSystem as RfvpFileSystem>::File: 'static,
    {
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

        #[cfg(feature = "old_school")]
        let (hcb, hcb_manifest, parser) = {
            let (path, info, manifest) = find_hcb_info(
                host,
                boot.asset_root,
                boot.hcb_extension,
                boot.max_manifest_entries,
            )?;
            let mut file = host.fs().open(&path)?;
            let len = file.len()?;
            if len == 0 || len as usize > boot.max_hcb_bytes {
                return Err(RfvpError::CapacityExceeded);
            }
            // Small pages on the PlayStation: a scene's hot code is spread over the script, and
            // a 1 KiB page holds less that is not needed (each miss costs a CD seek).
            #[cfg(feature = "psx")]
            const PAGE: usize = 1024;
            #[cfg(not(feature = "psx"))]
            const PAGE: usize = 4096;
            let parser = if len as usize <= boot.hcb_cache_bytes {
                let bytes = file.read_to_vec(len as usize)?;
                Parser::from_bytes(bytes, boot.nls)
            } else {
                let pages = (boot.hcb_cache_bytes / PAGE).max(8);
                Parser::from_paged_file(file, len as usize, boot.nls, PAGE, pages)
            }
            .map_err(|err| {
                self.last_error_detail = Some(err.to_string());
                RfvpError::InvalidData
            })?;
            (
                LoadedHcb {
                    path,
                    bytes: Vec::new(),
                    info,
                },
                manifest,
                parser,
            )
        };

        #[cfg(not(feature = "old_school"))]
        let (hcb, hcb_manifest, parser) = {
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
            (hcb, hcb_manifest, parser)
        };
        #[cfg(feature = "old_school")]
        let old_school_scale = load_required_old_school_scale(host).map_err(|(err, detail)| {
            self.last_error_detail = Some(detail);
            err
        })?;
        #[cfg(feature = "psx")]
        host.log(RfvpLogLevel::Debug, "boot: before font");
        let default_font = load_required_default_font(host).map_err(|(err, detail)| {
            self.last_error_detail = Some(detail);
            err
        })?;
        let mut screen = parser.get_screen_size();
        #[cfg(feature = "old_school")]
        {
            screen = (
                scale_old_school_u32(screen.0, old_school_scale),
                scale_old_school_u32(screen.1, old_school_scale),
            );
        }
        #[cfg(feature = "psx")]
        host.log(RfvpLogLevel::Debug, "boot: before globals");
        GLOBAL.lock().map_err(|_| RfvpError::Backend)?.init_with(
            parser.get_non_volatile_global_count(),
            parser.get_volatile_global_count(),
        );

        #[cfg(feature = "psx")]
        host.log(RfvpLogLevel::Debug, "boot: before packs");
        let mut vfs = build_host_vfs(host, boot)?;
        #[cfg(not(feature = "old_school"))]
        vfs.add_loose_file(&hcb.path, hcb.bytes.clone());

        #[cfg(feature = "psx")]
        host.log(RfvpLogLevel::Debug, "boot: before game data");
        // The PlayStation fills in the core's own (fresh) GameData rather than building a second
        // one beside it: each is over 100 KiB of its 2 MiB.
        #[cfg(feature = "psx")]
        let game_data = &mut self.game_data;
        #[cfg(not(feature = "psx"))]
        let mut game_data = GameData::default();
        game_data
            .save_manager
            .set_storage(game_save_storage(host, &hcb.path, &parser.get_title()));
        #[cfg(feature = "old_school")]
        game_data.set_old_school_scale(old_school_scale);
        game_data.fontface_manager = FontEnumerator::from_default_font(default_font);
        game_data.vfs = vfs;
        game_data.nls = boot.nls;
        #[cfg(feature = "psx")]
        host.log(RfvpLogLevel::Debug, "boot: after game data");
        game_data.set_window(Window::new(screen, 1.0));
        #[cfg(feature = "psx")]
        let global = crate::subsystem::global_savedata::try_load_global_savedata_v1(game_data);
        #[cfg(not(feature = "psx"))]
        let global = crate::subsystem::global_savedata::try_load_global_savedata_v1(&mut game_data);
        match global {
            Ok(true) => host.log(RfvpLogLevel::Info, "restored the global save"),
            Ok(false) => {}
            Err(err) => host.log(
                RfvpLogLevel::Warn,
                &format!("could not read the global save: {err:#}"),
            ),
        }

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
        #[cfg(not(feature = "psx"))]
        {
            self.game_data = game_data;
        }
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
            self.save_global_if_changed(host);
        }

        crate::platform_time::set_host_time_us(now);
        host.audio().tick(elapsed_us)?;
        {
            let game = &mut self.game_data;
            if let Err(err) = game.video_manager.tick(&mut game.motion_manager) {
                host.log(RfvpLogLevel::Warn, &format!("movie stopped: {err:?}"));
                game.video_manager.stop(&mut game.motion_manager);
            }
        }
        let (mut vm_us, mut scene_us, mut render_us) = (0, 0, 0);
        if let (Some(parser), Some(vm_runner)) = (self.parser.as_mut(), self.vm_runner.as_mut()) {
            let pending_us = self.carry_us + elapsed_us;
            let frame_time_ms = pending_us / 1_000;
            self.carry_us = pending_us % 1_000;
            // Script timers (TimerSet/TimerGet) advance before the scripts run, as on desktop.
            self.game_data
                .timer_manager
                .tick(frame_time_ms.min(u32::MAX as u64) as u32);
            let t0 = host.clock().ticks_us();
            crate::perf::mark(100);
            if let Err(err) = vm_runner.tick(&mut self.game_data, parser, frame_time_ms) {
                let message = err.to_string();
                host.log(RfvpLogLevel::Error, &message);
                self.last_error = Some(RfvpError::Unsupported);
                self.last_error_detail = Some(message);
                return Err(RfvpError::Unsupported);
            }

            // The original engine advances scripts before text and motion updates.
            let t1 = host.clock().ticks_us();
            crate::perf::stop(17, t0);
            crate::perf::mark(200);
            let mut scene = AnzuScene::new();
            scene.update_after_vm(&mut self.game_data, frame_time_ms);
            crate::perf::mark(300);
            let t2 = host.clock().ticks_us();

            let t = crate::perf::start();
            self.flush_audio(host)?;
            crate::perf::stop(18, t);
            let t = crate::perf::start();
            // A SaveWrite whose payload was prepared earlier needs no capture of this frame.
            self.commit_prepared_save(host);
            crate::perf::stop(19, t);
            crate::perf::mark(400);
            self.render_game_frame(host)?;
            crate::perf::mark(500);
            let t3 = host.clock().ticks_us();
            vm_us = t1.saturating_sub(t0);
            scene_us = t2.saturating_sub(t1);
            render_us = t3.saturating_sub(t2);
            let t = crate::perf::start();
            self.capture_pending_save(host);
            crate::perf::stop(19, t);
            self.us_since_global_save = self.us_since_global_save.saturating_add(elapsed_us);
            // The PlayStation writes the global save with each slot save only: encoding every
            // global on a timer needs RAM it does not have.
            if !cfg!(feature = "psx") && self.us_since_global_save >= GLOBAL_SAVE_INTERVAL_US {
                self.save_global_if_changed(host);
            }
        } else if self.run_state == RfvpCoreRunState::BootFailed {
            return Err(self.last_error.unwrap_or(RfvpError::InvalidData));
        }
        self.frame_index = self.frame_index.wrapping_add(1);

        Ok(RfvpTickResult {
            frame_index: self.frame_index,
            consumed_events,
            elapsed_us,
            vm_us,
            scene_us,
            render_us,
        })
    }

    /// Write the global save (flags, read-text marks) if it changed since it was last written.
    fn save_global_if_changed<H: RfvpHost>(&mut self, host: &mut H) {
        self.us_since_global_save = 0;
        let Some(storage) = self.game_data.save_manager.storage().cloned() else {
            return;
        };
        let bytes = match crate::subsystem::global_savedata::encode_global_savedata_v1(
            &self.game_data,
        ) {
            Ok(bytes) => bytes,
            Err(err) => {
                host.log(
                    RfvpLogLevel::Warn,
                    &format!("could not encode the global save: {err:#}"),
                );
                return;
            }
        };
        let hash = fnv1a(&bytes);
        if self.last_global_save_hash == Some(hash) {
            return;
        }
        match storage.write(crate::subsystem::global_savedata::GLOBAL_SAVE_STORAGE_PATH, &bytes) {
            Ok(()) => self.last_global_save_hash = Some(hash),
            Err(err) => host.log(
                RfvpLogLevel::Warn,
                &format!("could not write the global save: {err:?}"),
            ),
        }
    }

    /// Commit a `SaveWrite(slot)` whose payload `SaveCreate(3, ..)` prepared earlier.
    fn commit_prepared_save<H: RfvpHost>(&mut self, host: &mut H) {
        let nls = self.game_data.get_nls();
        match self.game_data.save_manager.try_commit_local_savedata(nls) {
            Ok(true) => {
                self.game_data.save_manager.consume_save_write_result();
                self.save_global_if_changed(host);
            }
            Ok(false) => {}
            Err(err) => {
                host.log(RfvpLogLevel::Error, &format!("could not write the save: {err:#}"));
                self.game_data.save_manager.consume_save_write_result();
            }
        }
    }

    /// If a save asked for the current frame, take its thumbnail and the engine state now, and
    /// write the slot (or prepare the payload for a later `SaveWrite`).
    fn capture_pending_save<H: RfvpHost>(&mut self, host: &mut H) {
        let Some((slot, thumb_w, thumb_h)) = self.game_data.save_manager.pending_save_capture()
        else {
            return;
        };
        let expected = thumb_w as usize * thumb_h as usize * 4;
        let thumb = match host.renderer().capture_thumbnail(thumb_w, thumb_h) {
            Some(thumb) if thumb.len() == expected => thumb,
            // A renderer that cannot read its frame back leaves the thumbnail blank (on the
            // PlayStation without making a frame of RGBA first).
            #[cfg(feature = "psx")]
            _ => Vec::new(),
            #[cfg(not(feature = "psx"))]
            _ => alloc::vec![0u8; expected],
        };
        self.game_data
            .save_manager
            .set_wall_time(host.clock().wall_time());
        let nls = self.game_data.get_nls();
        #[cfg(feature = "psx")]
        let result = self
            .game_data
            .save_manager
            .slot_file_head(nls.clone(), &thumb)
            .and_then(|mut bytes| {
                drop(thumb);
                crate::subsystem::save_state::append_state_chunk_streamed(&mut bytes, &mut self.game_data)?;
                let saves = &mut self.game_data.save_manager;
                if slot == u32::MAX {
                    saves.finish_prepare(bytes);
                    if saves.try_commit_local_savedata(nls)? {
                        saves.consume_save_write_result();
                        self.save_global_if_changed(host);
                    }
                } else {
                    saves.finish_write(bytes, nls)?;
                    saves.consume_save_write_result();
                    self.save_global_if_changed(host);
                }
                Ok(())
            });
        #[cfg(not(feature = "psx"))]
        let snap = crate::subsystem::save_state::SaveStateSnapshotV1::capture(&mut self.game_data);

        #[cfg(not(feature = "psx"))]
        let result = if slot == u32::MAX {
            // SaveCreate(3, ..): prepare the payload in memory, and commit it if SaveWrite has
            // already named a slot.
            self.game_data
                .save_manager
                .finalize_local_savedata_prepare(nls.clone(), thumb_w, thumb_h, &thumb, Some(&snap))
                .and_then(|()| {
                    if self.game_data.save_manager.try_commit_local_savedata(nls)? {
                        self.game_data.save_manager.consume_save_write_result();
                        self.save_global_if_changed(host);
                    }
                    Ok(())
                })
        } else {
            self.game_data
                .save_manager
                .finalize_save_write(nls, thumb_w, thumb_h, &thumb, Some(&snap))
                .map(|()| {
                    self.game_data.save_manager.consume_save_write_result();
                    self.save_global_if_changed(host);
                })
        };
        if let Err(err) = result {
            host.log(RfvpLogLevel::Error, &format!("could not save: {err:#}"));
            self.game_data.save_manager.consume_save_write_result();
        }
    }

    pub fn render_empty_frame<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<()> {
        self.render_game_frame(host)
    }

    pub fn render_status_frame<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<()> {
        self.render_game_frame(host)
    }

    fn render_game_frame<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<()> {
        // Free the host textures of graphs the game unloaded, and re-decode evicted graphs
        // whose host copy is gone, before drawing.
        let motion = &mut self.game_data.motion_manager;
        for graph_id in motion.take_pending_gpu_graph_unloads() {
            self.render_cache.remove_graph(host.renderer(), graph_id);
        }
        #[cfg(feature = "psx")]
        {
            let cache = core::cell::RefCell::new(&mut self.render_cache);
            let renderer = host.renderer();
            motion.upload_evicted_one_by_one(
                &self.game_data.vfs,
                |id, generation| cache.borrow().is_graph_uploaded(id, generation),
                |id, graph| cache.borrow_mut().upload_graph(renderer, id, graph),
            );
        }
        #[cfg(not(feature = "psx"))]
        {
            let cache = &self.render_cache;
            motion.restore_evicted_cpu_pixels(&self.game_data.vfs, |id, generation| {
                cache.is_graph_uploaded(id, generation)
            });
        }
        let frame = render_motion_to_host(
            host.renderer(),
            &mut self.render_cache,
            &self.game_data.motion_manager,
            (self.config.virtual_width, self.config.virtual_height),
        )?;
        self.hit_proxies = frame.hit_proxies;
        // Pictures the host now holds need no second copy here: a console has little memory.
        let cache = &self.render_cache;
        self.game_data
            .motion_manager
            .evict_uploaded_cpu_pixels(|id, generation| cache.is_graph_uploaded(id, generation));
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
                RfvpEvent::PointerDown {
                    button: crate::host_api::PointerButton::Right,
                    ..
                } => {
                    self.game_data.inputs_manager.notify_mouse_down(
                        crate::subsystem::resources::input_manager::KeyCode::MouseRight,
                    );
                }
                RfvpEvent::PointerUp {
                    button: crate::host_api::PointerButton::Right,
                    ..
                } => {
                    self.game_data.inputs_manager.notify_mouse_up(
                        crate::subsystem::resources::input_manager::KeyCode::MouseRight,
                    );
                }
                RfvpEvent::KeyDown { key, repeat, .. } => {
                    if let Some(code) = engine_key(key) {
                        self.game_data.inputs_manager.notify_keycode_down(code, repeat);
                    }
                }
                RfvpEvent::KeyUp { key, .. } => {
                    if let Some(code) = engine_key(key) {
                        self.game_data.inputs_manager.notify_keycode_up(code);
                    }
                }
                RfvpEvent::Wheel { delta_y, .. } => {
                    self.game_data.inputs_manager.notify_mouse_wheel(delta_y);
                }
                _ => {}
            }
        }
        self.game_data.inputs_manager.begin_frame();
    }

    /// Hand the frame's audio commands to the host. A sound the host cannot play is logged and
    /// skipped: one bad or unsupported file must not stop the game.
    fn flush_audio<H: RfvpHost>(&mut self, host: &mut H) -> RfvpResult<()> {
        let mut commands = Vec::new();
        self.game_data.audio_manager().drain_commands(&mut commands);
        for command in commands {
            crate::perf::mark(1000 + audio_command_index(&command));
            let result = match command {
                crate::rfvp_audio::AudioCommand::LoadEncoded { id, kind, bytes } => {
                    host.audio().load_encoded(id, kind, &bytes)
                }
                crate::rfvp_audio::AudioCommand::LoadStream { id, kind, stream } => {
                    match host.audio().load_stream(id, kind, stream.clone()) {
                        Err(RfvpError::Unsupported) => stream
                            .read_all()
                            .and_then(|bytes| host.audio().load_encoded(id, kind, &bytes)),
                        other => other,
                    }
                }
                crate::rfvp_audio::AudioCommand::CreateStream { id, desc } => {
                    host.audio().create_stream(id, desc)
                }
                crate::rfvp_audio::AudioCommand::SubmitI16 { id, samples } => {
                    host.audio().submit_i16(id, &samples)
                }
                crate::rfvp_audio::AudioCommand::SubmitF32 { id, samples } => {
                    host.audio().submit_f32(id, &samples)
                }
                crate::rfvp_audio::AudioCommand::Play {
                    id,
                    params,
                    fade_in_ms,
                } => {
                    host.audio().play(id, params, fade_in_ms)
                }
                crate::rfvp_audio::AudioCommand::Stop { id, fade_ms } => {
                    host.audio().stop(id, fade_ms)
                }
                crate::rfvp_audio::AudioCommand::Pause { id } => {
                    host.audio().pause(id)
                }
                crate::rfvp_audio::AudioCommand::Resume { id } => {
                    host.audio().resume(id)
                }
                crate::rfvp_audio::AudioCommand::SetParams { id, params } => {
                    host.audio().set_params(id, params)
                }
                crate::rfvp_audio::AudioCommand::DestroyStream { id } => {
                    host.audio().destroy_stream(id);
                    Ok(())
                }
                crate::rfvp_audio::AudioCommand::MasterVolume { volume } => {
                    host.audio().set_master_volume(volume)
                }
            };
            if let Err(err) = result {
                log::warn!("audio command failed: {err:?}");
            }
        }
        Ok(())
    }
}

/// A number for each kind of audio command, for [`crate::perf::mark`].
fn audio_command_index(command: &crate::rfvp_audio::AudioCommand) -> u32 {
    use crate::rfvp_audio::AudioCommand::*;
    match command {
        LoadEncoded { .. } => 1,
        LoadStream { .. } => 2,
        CreateStream { .. } => 3,
        SubmitI16 { .. } => 4,
        SubmitF32 { .. } => 5,
        Play { .. } => 6,
        Stop { .. } => 7,
        Pause { .. } => 8,
        Resume { .. } => 9,
        SetParams { .. } => 10,
        DestroyStream { .. } => 11,
        MasterVolume { .. } => 12,
    }
}

fn load_required_default_font<H: RfvpHost>(host: &mut H) -> Result<Font, (RfvpError, String)> {
    let callbacks = host.platform_callbacks();
    let mut bytes = Vec::new();
    #[cfg(feature = "psx")]
    {
        if host.fs().read_required_file("font.psxf", &mut bytes).is_err() {
            notify_fatal(callbacks, FatalErrorCode::MissingDefaultFont, MISSING_PSX_FONT_MESSAGE);
            return Err((RfvpError::NotFound, MISSING_PSX_FONT_MESSAGE.to_string()));
        }
        Font::from_psxf(bytes).map_err(|_| {
            notify_fatal(callbacks, FatalErrorCode::InvalidDefaultFont, MISSING_PSX_FONT_MESSAGE);
            (RfvpError::InvalidData, MISSING_PSX_FONT_MESSAGE.to_string())
        })
    }
    #[cfg(all(feature = "old_school", not(feature = "psx")))]
    {
        if host
            .fs()
            .read_required_file("defualt.tmap", &mut bytes)
            .is_err()
        {
            notify_fatal(
                callbacks,
                FatalErrorCode::MissingDefaultFont,
                MISSING_OLD_SCHOOL_FONT_MESSAGE,
            );
            return Err((
                RfvpError::NotFound,
                MISSING_OLD_SCHOOL_FONT_MESSAGE.to_string(),
            ));
        }
        match Font::from_old_school_tmap(bytes) {
            Ok(font) => Ok(font),
            Err(_) => {
                notify_fatal(
                    callbacks,
                    FatalErrorCode::InvalidDefaultFont,
                    INVALID_OLD_SCHOOL_FONT_MESSAGE,
                );
                Err((
                    RfvpError::InvalidData,
                    INVALID_OLD_SCHOOL_FONT_MESSAGE.to_string(),
                ))
            }
        }
    }

    #[cfg(not(feature = "old_school"))]
    {
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
}

fn notify_fatal(callbacks: PlatformCallbacks, code: FatalErrorCode, message: &str) {
    if let Some(callback) = callbacks.fatal_error {
        callback(callbacks.user_data, code, message.as_ptr(), message.len());
    }
}

#[cfg(feature = "old_school")]
fn load_required_old_school_scale<H: RfvpHost>(host: &mut H) -> Result<f32, (RfvpError, String)> {
    let mut bytes = Vec::new();
    if host
        .fs()
        .read_required_file("rfvp.toml", &mut bytes)
        .is_err()
    {
        return Err((
            RfvpError::NotFound,
            MISSING_OLD_SCHOOL_CONFIG_MESSAGE.to_string(),
        ));
    }
    parse_old_school_scale(&bytes).map_err(|detail| (RfvpError::InvalidData, detail))
}

#[cfg(feature = "old_school")]
fn parse_old_school_scale(bytes: &[u8]) -> Result<f32, String> {
    let text =
        core::str::from_utf8(bytes).map_err(|_| INVALID_OLD_SCHOOL_CONFIG_MESSAGE.to_string())?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("rfvp.toml is missing required top-level scale field".to_string());
    }

    let mut lines = trimmed.lines();
    let line = lines
        .next()
        .ok_or_else(|| "rfvp.toml is missing required top-level scale field".to_string())?
        .trim();
    if lines.any(|line| !line.trim().is_empty()) {
        return Err("rfvp.toml must contain only one top-level field: scale".to_string());
    }

    let Some((key, value)) = line.split_once('=') else {
        return Err("rfvp.toml is missing required top-level scale field".to_string());
    };
    if key.trim() != "scale" {
        return Err("rfvp.toml must contain only the top-level scale field".to_string());
    }

    let value = value.trim();
    if !(value.contains('.') || value.contains('e') || value.contains('E')) {
        return Err("rfvp.toml scale must be a floating-point number".to_string());
    }

    let scale: f32 = value
        .parse()
        .map_err(|_| "rfvp.toml scale must be a floating-point number".to_string())?;
    if !scale.is_finite() || scale <= 0.0 {
        return Err("rfvp.toml scale must be greater than 0".to_string());
    }
    Ok(scale)
}

#[cfg(feature = "old_school")]
fn scale_old_school_u32(value: u32, scale: f32) -> u32 {
    let scaled = round_half_up(value as f32 * scale);
    scaled.max(0) as u32
}

#[cfg(feature = "old_school")]
fn round_half_up(v: f32) -> i32 {
    if v >= 0.0 {
        (v + 0.5).floor() as i32
    } else {
        (v - 0.5).ceil() as i32
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

#[cfg(feature = "old_school")]
fn find_hcb_info<H: RfvpHost>(
    host: &mut H,
    root: &str,
    extension: &str,
    max_manifest_entries: usize,
) -> RfvpResult<(String, RfvpFileInfo, Vec<(String, RfvpFileInfo)>)> {
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
    let manifest = found.into_iter().skip(1).collect();
    Ok((path, info, manifest))
}

/// Where this game's saves go: its own folder in the platform's data directory
/// (`saves/<script name>-<title hash>/`), or the game directory on a host without one.
///
/// The folder is named after the game, not the script's bytes, so a patched or translated script
/// keeps its saves. Saves an earlier build kept in the game directory (`save/`) are copied over the
/// first time; the originals are left where they are.
fn game_save_storage<H: RfvpHost>(
    host: &mut H,
    hcb_path: &str,
    title: &str,
) -> Option<alloc::sync::Arc<dyn crate::host_api::RfvpStorage>> {
    let legacy = host.fs().storage();
    let Some(base) = host.fs().save_storage() else {
        return legacy;
    };
    let saves: alloc::sync::Arc<dyn crate::host_api::RfvpStorage> = alloc::sync::Arc::new(
        crate::host_api::SubdirStorage::new(base, &format!("saves/{}", game_save_key(hcb_path, title))),
    );
    if let Some(legacy) = legacy {
        let fresh = saves.list("save").map_or(true, |names| names.is_empty());
        if fresh {
            for name in legacy.list("save").unwrap_or_default() {
                let path = format!("save/{name}");
                if let Ok(bytes) = legacy.read(&path) {
                    let _ = saves.write(&path, &bytes);
                }
            }
        }
    }
    Some(saves)
}

/// A folder name for a game: the script's file name (letters, digits, `-` and `_`) and a hash of
/// the game's title, which tells apart two games whose scripts are named alike.
fn game_save_key(hcb_path: &str, title: &str) -> String {
    let file = hcb_path.rsplit(['/', '\\']).next().unwrap_or(hcb_path);
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let mut key: String = stem
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    if key.is_empty() {
        key.push_str("game");
    }
    // FNV-1a.
    let mut hash: u32 = 0x811c_9dc5;
    for byte in title.bytes() {
        hash = (hash ^ byte as u32).wrapping_mul(0x0100_0193);
    }
    format!("{key}-{hash:08x}")
}

struct LoadedHcb {
    path: String,
    bytes: Vec<u8>,
    info: RfvpFileInfo,
}

/// The engine's own key for a host key, if it has one.
fn engine_key(key: crate::host_api::KeyCode) -> Option<crate::subsystem::resources::input_manager::KeyCode> {
    use crate::host_api::KeyCode as Host;
    use crate::subsystem::resources::input_manager::KeyCode as Engine;
    Some(match key {
        Host::Escape => Engine::Esc,
        Host::Return => Engine::Enter,
        Host::Space => Engine::Space,
        Host::Tab => Engine::Tab,
        Host::Up => Engine::UpArrow,
        Host::Down => Engine::DownArrow,
        Host::Left => Engine::LeftArrow,
        Host::Right => Engine::RightArrow,
        Host::Shift => Engine::Shift,
        Host::Control => Engine::Ctrl,
        Host::Function(n) => match n {
            1 => Engine::F1,
            2 => Engine::F2,
            3 => Engine::F3,
            4 => Engine::F4,
            5 => Engine::F5,
            6 => Engine::F6,
            7 => Engine::F7,
            8 => Engine::F8,
            9 => Engine::F9,
            10 => Engine::F10,
            11 => Engine::F11,
            12 => Engine::F12,
            _ => return None,
        },
        _ => return None,
    })
}

/// Whether `path` is inside a directory named `save` (case-insensitive).
fn is_in_save_dir(path: &str) -> bool {
    let mut parts: Vec<&str> = path.split(|c| c == '/' || c == '\\').collect();
    parts.pop(); // the file name
    parts.iter().any(|part| part.eq_ignore_ascii_case("save"))
}

fn build_host_vfs<H: RfvpHost>(host: &mut H, boot: RfvpBootConfig<'_>) -> RfvpResult<Vfs>
where
    <<H as RfvpHost>::FileSystem as RfvpFileSystem>::File: 'static,
{
    let mut vfs = Vfs::new(boot.nls).map_err(|_| RfvpError::InvalidData)?;
    vfs.set_loose_dir(host.fs().game_files());
    // Packs too (also on the small consoles: a repack is loose folders, but a disc (the PS2's)
    // cannot hold its file names, so it carries the folders as packs).
    {
        let mut packs = Vec::new();
        {
            let visitor = &mut |path: &str, info: RfvpFileInfo| -> RfvpResult<()> {
                // Save files are `.bin` too, but they are not packs.
                if info.kind == crate::host_api::RfvpFileKind::File && !is_in_save_dir(path) {
                    packs.push(path.to_string());
                }
                Ok(())
            };
            host.fs()
                .enumerate_by_extension(boot.asset_root, "bin", visitor)?;
        }
        for path in packs {
            // Packs can be over a gigabyte: keep the file open and read entries on demand.
            let file = host.fs().open(&path)?;
            let source = alloc::sync::Arc::new(
                crate::subsystem::resources::vfs::HostPackFile::new(file)?,
            );
            let name = path.rsplit(['/', '\\']).next().unwrap_or(path.as_str());
            let folder = match name.len().checked_sub(4) {
                Some(stem) if name.is_char_boundary(stem) && name[stem..].eq_ignore_ascii_case(".bin") => {
                    &name[..stem]
                }
                _ => name,
            };
            // A file that is not a valid pack must not stop the game from starting.
            if let Err(err) = vfs.add_pack_source(folder, source) {
                host.log(
                    RfvpLogLevel::Warn,
                    &format!("skipping {path}, not a valid pack: {err}"),
                );
            }
        }
        Ok(vfs)
    }
}
