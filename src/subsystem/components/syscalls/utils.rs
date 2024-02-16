use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

pub fn debug_message(
    _game_data: &mut GameData,
    message: &Variant,
    var: &Variant,
) -> Result<Variant> {
    let msg = if let Variant::String(message) = message {
        message
    } else {
        bail!("Invalid message type");
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
        bail!("float_to_int: Invalid value type");
    };

    Ok(Variant::Int(value))
}

pub fn int_to_text(_game_data: &mut GameData, value: &Variant, width: &Variant) -> Result<Variant> {
    let value = if let Variant::Int(value) = value {
        *value
    } else {
        bail!("int_to_text: Invalid value type");
    };

    let width = if let Variant::Int(width) = width {
        *width
    } else {
        bail!("int_to_text: Invalid width type");
    };

    let value = format!("{:width$}", value, width = width as usize);
    Ok(Variant::String(value))
}

pub fn rand(_game_data: &mut GameData) -> Result<Variant> {
    Ok(Variant::Float(rand::random()))
}

pub fn system_project_dir(game_data: &mut GameData, _dir: &Variant) -> Result<Variant> {
    Ok(Variant::Nil)
}

pub fn system_at_skipname(
    game_data: &mut GameData,
    _arg0: &Variant,
    _arg1: &Variant,
) -> Result<Variant> {
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
