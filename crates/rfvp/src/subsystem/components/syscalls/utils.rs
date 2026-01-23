use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

pub fn debug_message(
    _game_data: &mut GameData,
    message: &Variant,
    var: &Variant,
) -> Result<Variant> {
    let msg = match message {
        Variant::String(message) | Variant::ConstString(message, _) => message.clone(),
        _ => {
            log::error!("debug_message: Invalid message type");
            return Ok(Variant::Nil);
        },
    };

    log::info!("DEBUG => {}: {:?}", msg, var);
    Ok(Variant::Nil)
}

pub fn break_point(_game_data: &mut GameData) -> Result<Variant> {
    log::info!("Break point");
    Ok(Variant::Nil)
}

pub fn float_to_int(_game_data: &mut GameData, value: &Variant) -> Result<Variant> {
    let value = if let Variant::Int(value) = value {
        *value
    } else {
        log::error!("float_to_int: Invalid value type");
        return Ok(Variant::Nil);
    };

    Ok(Variant::Int(value))
}

pub fn int_to_text(_game_data: &mut GameData, value: &Variant, width: &Variant) -> Result<Variant> {
    let value = if let Variant::Int(value) = value {
        *value
    } else {
        log::error!("int_to_text: Invalid value type");
        return Ok(Variant::Nil);
    };

    let width = if let Variant::Int(width) = width {
        *width
    } else {
        log::error!("int_to_text: Invalid width type");
        return Ok(Variant::Nil);
    };

    // pad with zeros to the left
    let value = format!("{:0width$}", value, width = width as usize);
    Ok(Variant::String(value))
}

pub fn rand(_game_data: &mut GameData) -> Result<Variant> {
    Ok(Variant::Float(rand::random()))
}

pub fn system_project_dir(_game_data: &mut GameData, _dir: &Variant) -> Result<Variant> {
    Ok(Variant::Nil)
}

pub fn system_at_skipname(
    _game_data: &mut GameData,
    _arg0: &Variant,
    _arg1: &Variant,
) -> Result<Variant> {
    Ok(Variant::Nil)
}

/// WindowMode(mode)
///
/// This syscall is lifecycle-critical in the original engine; it is not a simple "flag".
/// The script uses it to:
///   - switch between windowed/fullscreen rendering modes (via an internal `render_flag`)
///   - query the current mode
///   - query fullscreen capability
///   - control a special "first frame" behavior used by the engine when losing focus
///
/// We preserve observable semantics at the script level, while the backend (winit) may
/// choose to honor the requested mode change.
pub fn window_mode(game_data: &mut GameData, mode: &Variant) -> Result<Variant> {
    let mode = match mode {
        Variant::Int(m) => *m,
        _ => {
            log::error!("window_mode: invalid mode type: {:?}", mode);
            return Ok(Variant::Nil);
        }
    };

    match mode {
        // -1: try fullscreen. Returns 1 if fullscreen is not supported; otherwise returns -1.
        -1 => {
            if game_data.get_can_fullscreen() {
                // Original engine: render_flag = 2 indicates fullscreen request.
                game_data.set_render_flag(2);
                return Ok(Variant::Int(-1));
            }
            return Ok(Variant::Int(1));
        }
        // 0: windowed.
        0 => {
            game_data.set_render_flag(0);
            return Ok(Variant::Int(0));
        }
        // 1: fullscreen (engine uses a distinct path from -1).
        1 => {
            game_data.set_render_flag(1);
            return Ok(Variant::Int(1));
        }
        // 2: query current mode. Special return values for render_flag 2/3.
        2 => {
            let rf = game_data.get_render_flag();
            if rf == 2 {
                return Ok(Variant::Int(-1));
            }
            if rf == 3 {
                return Ok(Variant::Int(-2));
            }
            return Ok(Variant::Int(rf));
        }
        // 3: query fullscreen capability.
        3 => {
            return Ok(if game_data.get_can_fullscreen() {
                Variant::True
            } else {
                Variant::Nil
            });
        }
        // 4: set "first frame" flag.
        4 => {
            game_data.set_is_first_frame(true);
            return Ok(Variant::Nil);
        }
        // 5: clear "first frame" flag.
        5 => {
            game_data.set_is_first_frame(false);
            return Ok(Variant::Nil);
        }
        // 6: query "first frame" flag.
        6 => {
            return Ok(if game_data.get_is_first_frame() {
                Variant::True
            } else {
                Variant::Nil
            });
        }
        _ => {
            log::warn!("window_mode: unsupported mode {}", mode);
            Ok(Variant::Nil)
        }
    }
}

pub fn title_menu(_game_data: &mut GameData, _title: &Variant) -> Result<Variant> {
    Ok(Variant::Nil)
}

pub fn exit_mode(game_data: &mut GameData, mode: &Variant) -> Result<Variant> {
    let mode = match mode {
        Variant::Int(mode) => *mode,
        _ => {
            log::error!("exit_mode: Invalid mode type");
            return Ok(Variant::True);
        },
    };

    if mode == 0 {
        if game_data.get_close_pending() {
            game_data.set_close_pending(false);
            return Ok(Variant::True);
        }
    }
    else if mode == 1 {
        game_data.set_close_immediate(true);
    }
    else if mode == 2 {
        game_data.set_close_immediate(false);
    }
    else if mode == 3 {
        game_data.set_lock_scripter(true);
        game_data.set_last_current_thread(game_data.get_current_thread());
        game_data.set_game_should_exit(true);
    }
    else if mode == 4 {
        game_data.set_lock_scripter(false);
        game_data.set_game_should_exit(false);
    }

    Ok(Variant::Nil)
}

pub struct DebugMessage;
impl Syscaller for DebugMessage {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        debug_message(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for DebugMessage {}
unsafe impl Sync for DebugMessage {}

pub struct BreakPoint;
impl Syscaller for BreakPoint {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        break_point(game_data)
    }
}

unsafe impl Send for BreakPoint {}
unsafe impl Sync for BreakPoint {}

pub struct FloatToInt;
impl Syscaller for FloatToInt {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        float_to_int(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for FloatToInt {}
unsafe impl Sync for FloatToInt {}

pub struct IntToText;
impl Syscaller for IntToText {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        int_to_text(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for IntToText {}
unsafe impl Sync for IntToText {}

pub struct Rand;
impl Syscaller for Rand {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        rand(game_data)
    }
}

unsafe impl Send for Rand {}
unsafe impl Sync for Rand {}

pub struct SysProjFolder;
impl Syscaller for SysProjFolder {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        system_project_dir(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for SysProjFolder {}
unsafe impl Sync for SysProjFolder {}

pub struct SysAtSkipName;
impl Syscaller for SysAtSkipName {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        system_at_skipname(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for SysAtSkipName {}
unsafe impl Sync for SysAtSkipName {}


pub struct WindowMode;
impl Syscaller for WindowMode {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        window_mode(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for WindowMode {}
unsafe impl Sync for WindowMode {}


pub struct ExitMode;
impl Syscaller for ExitMode {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let mode = get_var!(args, 0);
        exit_mode(game_data, mode)
    }
}

unsafe impl Send for ExitMode {}
unsafe impl Sync for ExitMode {}



pub struct TitleMenu;
impl Syscaller for TitleMenu {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        title_menu(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for TitleMenu {}
unsafe impl Sync for TitleMenu {}

mod tests {
    use super::*;

    #[test]
    fn test_int_to_text() {
        let result = int_to_text(&mut GameData::default(), &Variant::Int(42), &Variant::Int(5)).unwrap();
        crate::trace::syscall(format_args!("Result: {:?}", result));
    }
}

/// Debmess(level, msg)
/// IDA SYSCALL_SPECS: argc=2
pub struct Debmess;
impl Syscaller for Debmess {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        // Reuse DebugMessage implementation if present.
        DebugMessage.call(game_data, args)
    }
}



/// nullsub_2(...)
/// Used by auto-generated syscall specs for no-op placeholders.
pub struct nullsub_2;
impl Syscaller for nullsub_2 {
    fn call(&self, _game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        Ok(Variant::Nil)
    }
}


/// DissolveWait()
/// IDA SYSCALL_SPECS: argc=1
pub struct DissolveWait;
impl Syscaller for DissolveWait {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        game_data.thread_wrapper.dissolve_wait();
        Ok(Variant::Nil)
    }
}


/// ExitDialog()
/// IDA SYSCALL_SPECS: argc=0
pub struct ExitDialog;
impl Syscaller for ExitDialog {
    fn call(&self, _game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        Ok(Variant::Nil)
    }
}


/// MenuMessSkip()
/// IDA SYSCALL_SPECS: argc=1
pub struct MenuMessSkip;
impl Syscaller for MenuMessSkip {
    fn call(&self, _game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        Ok(Variant::Nil)
    }
}



/// A minimal named stub used to keep the script VM running even when a syscall is not implemented yet.
/// This is intentionally "soft-fail": it logs once per call site and returns Nil.
pub struct UnimplementedNamed {
    name: &'static str,
}

impl UnimplementedNamed {
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl Syscaller for UnimplementedNamed {
    fn call(&self, _game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        log::warn!("Unimplemented syscall: {}", self.name);
        Ok(Variant::Nil)
    }
}

unsafe impl Send for UnimplementedNamed {}
unsafe impl Sync for UnimplementedNamed {}
