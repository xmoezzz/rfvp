#![cfg(all(rfvp_switch, feature = "switch-core"))]

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use anyhow::{Context, Result};
use rfvp_switch_render::RenderCommand;
use rfvp_switch_core_abi::{
    RfvpSwitchCoreStats, RfvpSwitchCoreStatus, RfvpSwitchInputFrame,
    RFVP_SWITCH_BUTTON_A, RFVP_SWITCH_BUTTON_B, RFVP_SWITCH_BUTTON_DOWN,
    RFVP_SWITCH_BUTTON_LEFT, RFVP_SWITCH_BUTTON_RIGHT, RFVP_SWITCH_BUTTON_UP,
    RFVP_SWITCH_CORE_ABI_VERSION,
};

use crate::platform_time::Duration;
use crate::script::parser::{Nls, Parser};
use crate::subsystem::anzu_scene::AnzuScene;
use crate::subsystem::resources::input_manager::KeyCode;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::subsystem::resources::vfs::Vfs;
use crate::subsystem::resources::window::Window;
use crate::subsystem::scene::Scene;
use crate::subsystem::world::GameData;
use crate::utils::file::set_base_path;
use crate::vm_runner::VmRunner;
use crate::switch_render_bridge::SwitchPrimRenderer;

pub struct RfvpSwitchCore {
    parser: Parser,
    game: Box<GameData>,
    vm: VmRunner,
    scene: AnzuScene,
    renderer: SwitchPrimRenderer,
    frame_no: u64,
    last_status: i32,
    forced_yield: u32,
    forced_yield_contexts: u32,
}

impl RfvpSwitchCore {
    fn create(game_root: &str, nls: Nls, width: u32, height: u32) -> Result<Self> {
        set_base_path(game_root);

        let hcb_path = find_hcb(game_root)
            .with_context(|| format!("find .hcb under Switch game root {}", game_root))?;
        let parser = Parser::new(hcb_path, nls)
            .context("parse .hcb script for Switch core")?;

        let mut game = Box::<GameData>::default();
        game.vfs = Vfs::new(nls).context("initialize Switch core VFS")?;
        game.nls = nls;

        let script_size = parser.get_screen_size();
        let window_size = if width != 0 && height != 0 {
            (width, height)
        } else {
            script_size
        };
        game.set_window(Window::new(window_size, 1.0));

        let mut vm = VmRunner::new(ThreadManager::new());
        vm.start_main(parser.get_entry_point());

        let mut scene = AnzuScene::default();
        scene.on_start(&mut *game);

        Ok(Self {
            parser,
            game,
            vm,
            scene,
            renderer: SwitchPrimRenderer::new(window_size),
            frame_no: 0,
            last_status: RfvpSwitchCoreStatus::Ok as i32,
            forced_yield: 0,
            forced_yield_contexts: 0,
        })
    }

    fn tick(&mut self, frame_time_ms: u32, input: Option<&RfvpSwitchInputFrame>) -> i32 {
        if let Some(input) = input {
            self.apply_input(input);
        }

        let frame_ms = frame_time_ms as u64;
        let delta = Duration::from_millis(frame_ms.max(1));
        self.game.time_mut_ref().set_external_delta(delta);
        let frame_duration = self.game.time_mut_ref().frame();
        let frame_ms = frame_duration.as_millis().min(u64::MAX as u128) as u64;

        self.game.inputs_manager.begin_frame();
        self.game
            .timer_manager
            .tick(frame_ms.min(u32::MAX as u64) as u32);

        if !self.game.video_manager.is_modal_active() {
            self.scene.on_update(&mut *self.game);
        }

        match self.vm.tick(&mut *self.game, &mut self.parser, frame_ms) {
            Ok(report) => {
                if !self.game.video_manager.is_modal_active() {
                    self.scene.late_update(&mut *self.game);
                }
                let audio_ms = frame_ms.min(u32::MAX as u64) as u32;
                self.game.audio_manager().mix_to_ring(audio_ms);
                if let Err(e) = self.renderer.rebuild(&self.game.motion_manager) {
                    log::error!("rfvp_switch_core_tick: render command build failed: {:#}", e);
                }
                self.frame_no = self.frame_no.wrapping_add(1);
                self.forced_yield = report.forced_yield as u32;
                self.forced_yield_contexts = report.forced_yield_contexts;
                self.last_status = RfvpSwitchCoreStatus::Ok as i32;
                self.last_status
            }
            Err(e) => {
                log::error!("rfvp_switch_core_tick: VM tick failed: {:#}", e);
                self.last_status = RfvpSwitchCoreStatus::VmTickFailed as i32;
                self.last_status
            }
        }
    }

    fn apply_input(&mut self, input: &RfvpSwitchInputFrame) {
        self.game.inputs_manager.notify_mouse_move(input.touch_x, input.touch_y);
        self.game.inputs_manager.set_mouse_in(input.touch_active != 0);

        if input.touch_down != 0 {
            self.game.inputs_manager.notify_mouse_down(KeyCode::MouseLeft);
        }
        if input.touch_up != 0 {
            self.game.inputs_manager.notify_mouse_up(KeyCode::MouseLeft);
        }

        self.apply_button(input.buttons_down, input.buttons_up, RFVP_SWITCH_BUTTON_A, KeyCode::MouseLeft);
        self.apply_button(input.buttons_down, input.buttons_up, RFVP_SWITCH_BUTTON_B, KeyCode::MouseRight);
        self.apply_button(input.buttons_down, input.buttons_up, RFVP_SWITCH_BUTTON_UP, KeyCode::UpArrow);
        self.apply_button(input.buttons_down, input.buttons_up, RFVP_SWITCH_BUTTON_DOWN, KeyCode::DownArrow);
        self.apply_button(input.buttons_down, input.buttons_up, RFVP_SWITCH_BUTTON_LEFT, KeyCode::LeftArrow);
        self.apply_button(input.buttons_down, input.buttons_up, RFVP_SWITCH_BUTTON_RIGHT, KeyCode::RightArrow);
    }

    fn apply_button(&mut self, down: u32, up: u32, mask: u32, keycode: KeyCode) {
        if (down & mask) != 0 {
            match keycode {
                KeyCode::MouseLeft | KeyCode::MouseRight => self.game.inputs_manager.notify_mouse_down(keycode),
                _ => self.game.inputs_manager.notify_keycode_down(keycode, false),
            }
        } else if (up & mask) != 0 {
            match keycode {
                KeyCode::MouseLeft | KeyCode::MouseRight => self.game.inputs_manager.notify_mouse_up(keycode),
                _ => self.game.inputs_manager.notify_keycode_up(keycode),
            }
        }
    }

    fn render_commands(&self) -> &[RenderCommand] {
        self.renderer.commands()
    }

    fn audio_pop_i16(&self, out: &mut [i16]) -> usize {
        self.game.audio_manager().pop_interleaved_i16(out)
    }

    fn audio_queued_samples(&self) -> usize {
        self.game.audio_manager().queued_samples()
    }

    fn stats(&self) -> RfvpSwitchCoreStats {
        RfvpSwitchCoreStats {
            abi_version: RFVP_SWITCH_CORE_ABI_VERSION,
            frame_no: self.frame_no,
            last_status: self.last_status,
            forced_yield: self.forced_yield,
            forced_yield_contexts: self.forced_yield_contexts,
            main_thread_exited: self.game.get_main_thread_exited() as u32,
            game_should_exit: self.game.get_game_should_exit() as u32,
        }
    }
}

fn parse_cstr(ptr: *const c_char) -> Result<String, RfvpSwitchCoreStatus> {
    if ptr.is_null() {
        return Err(RfvpSwitchCoreStatus::Null);
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|_| RfvpSwitchCoreStatus::InvalidUtf8)
}

fn parse_nls(ptr: *const c_char) -> Result<Nls, RfvpSwitchCoreStatus> {
    let nls = parse_cstr(ptr)?;
    nls.parse().map_err(|_| RfvpSwitchCoreStatus::InvalidNls)
}

fn find_hcb(game_root: impl AsRef<Path>) -> Result<PathBuf> {
    let mut pattern = game_root.as_ref().to_path_buf();
    pattern.push("*.hcb");

    let matches: Vec<_> = glob::glob(&pattern.to_string_lossy())?.flatten().collect();
    if matches.is_empty() {
        anyhow::bail!("no .hcb file found in {}", game_root.as_ref().display());
    }
    Ok(matches[0].clone())
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_abi_version() -> u32 {
    RFVP_SWITCH_CORE_ABI_VERSION
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_create(
    game_root_utf8: *const c_char,
    nls_utf8: *const c_char,
    width: u32,
    height: u32,
) -> *mut RfvpSwitchCore {
    let game_root = match parse_cstr(game_root_utf8) {
        Ok(v) if !v.is_empty() => v,
        Ok(_) => return null_mut(),
        Err(_) => return null_mut(),
    };
    let nls = match parse_nls(nls_utf8) {
        Ok(v) => v,
        Err(_) => return null_mut(),
    };

    match RfvpSwitchCore::create(&game_root, nls, width, height) {
        Ok(core) => Box::into_raw(Box::new(core)),
        Err(e) => {
            log::error!("rfvp_switch_core_create failed: {:#}", e);
            null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_tick(
    core: *mut RfvpSwitchCore,
    frame_time_ms: u32,
    input: *const RfvpSwitchInputFrame,
) -> i32 {
    if core.is_null() {
        return RfvpSwitchCoreStatus::Null as i32;
    }
    let input_ref = if input.is_null() { None } else { Some(&*input) };
    (*core).tick(frame_time_ms, input_ref)
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_stats(
    core: *const RfvpSwitchCore,
    out: *mut RfvpSwitchCoreStats,
) -> i32 {
    if core.is_null() || out.is_null() {
        return RfvpSwitchCoreStatus::Null as i32;
    }
    out.write((*core).stats());
    RfvpSwitchCoreStatus::Ok as i32
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_destroy(core: *mut RfvpSwitchCore) {
    if !core.is_null() {
        drop(Box::from_raw(core));
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_render_command_count(core: *const RfvpSwitchCore) -> usize {
    if core.is_null() {
        return 0;
    }
    (*core).render_commands().len()
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_render_commands(core: *const RfvpSwitchCore) -> *const RenderCommand {
    if core.is_null() {
        return std::ptr::null();
    }
    (*core).render_commands().as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_audio_queued_samples(core: *const RfvpSwitchCore) -> usize {
    if core.is_null() {
        return 0;
    }
    (*core).audio_queued_samples()
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_audio_pop_i16(
    core: *const RfvpSwitchCore,
    out: *mut i16,
    len: usize,
) -> usize {
    if core.is_null() || out.is_null() || len == 0 {
        return 0;
    }
    let out = std::slice::from_raw_parts_mut(out, len);
    (*core).audio_pop_i16(out)
}
