use anyhow::{bail, Result};

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

pub enum WinMode {
    _Unknown = -1,
    Windowed = 0,
    Fullscreen = 1,
    GetWindowed = 2,
    GetResizable = 3,
    Resizable = 4,
    NonResizable = 5,
    _UnUsed = 6,
}

impl TryFrom<i32> for WinMode {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> anyhow::Result<Self> {
        match value {
            -1 => Ok(WinMode::_Unknown),
            0 => Ok(WinMode::Windowed),
            1 => Ok(WinMode::Fullscreen),
            2 => Ok(WinMode::GetWindowed),
            3 => Ok(WinMode::GetResizable),
            4 => Ok(WinMode::Resizable),
            5 => Ok(WinMode::NonResizable),
            6 => Ok(WinMode::_UnUsed),
            _ => bail!("Invalid value for WinMode"),
        }
    }
}

pub fn window_mode(_game_data: &mut GameData, mode: &Variant) -> Result<Variant> {
    let mode = match mode {
        Variant::Int(mode) => WinMode::try_from(*mode)?,
        _ => {
            log::error!("window_mode: Invalid mode type: {:?}", mode);
            return Ok(Variant::True);
        },
    };

    // emulate the behavior of the original engine
    match mode {
        WinMode::_Unknown => return Ok(Variant::Int(0)),
        WinMode::Windowed => return Ok(Variant::Int(0)),
        WinMode::Fullscreen => return Ok(Variant::Int(1)),
        WinMode::GetWindowed => return Ok(Variant::Int(0)),
        WinMode::GetResizable => {},
        WinMode::Resizable => {},
        WinMode::NonResizable => {},
        WinMode::_UnUsed => {
            log::warn!("window_mode: Unused mode");
        },
    }

    Ok(Variant::Nil)
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
        println!("Result: {:?}", result);
    }
}