use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::resources::history_manager::HistoryFunction;
use crate::subsystem::world::GameData;

use super::Syscaller;

pub fn history_set(game_data: &mut GameData, fnid: &Variant, value: &Variant) -> Result<Variant> {
    if fnid.is_nil() {
        game_data.history_manager.push();
        return Ok(Variant::Nil);
    }

    let id = match fnid.as_int() {
        Some(id) => id,
        None => {
            log::error!("history_set: unexpected fnid: {:?}", fnid);
            return Ok(Variant::Nil);
        }
    };

    match id.try_into() {
        Ok(HistoryFunction::Name) => {
            match value.as_string() {
                Some(value) => {
                    game_data.history_manager.set_name(value.to_owned());
                }
                _ => {
                    log::error!("history_set: unexpected value for set_name : {:?}", value);
                }
            }
        }
        Ok(HistoryFunction::Content) => {
            match value.as_string() {
                Some(value) => {
                    game_data.history_manager.set_content(value.to_owned());
                }
                _ => {
                    log::error!("history_set: unexpected value for set_content : {:?}", value);
                }
            }
        }
        Ok(HistoryFunction::Voice) => {
            match value.as_int() {
                Some(value) => {
                    game_data.history_manager.set_voice(value);
                }
                _ => {
                    log::error!("history_set: unexpected value for set_voice : {:?}", value);
                }
            }
        }
        _ => {
            log::error!("history_set: unexpected fnid value: {:?}", id);
        }
    };

    Ok(Variant::Nil)
}

pub fn history_get(game_data: &mut GameData, kind: &Variant, idx: &Variant) -> Result<Variant> {
    // Original engine behavior (HistoryGet):
    // - If arg0 is nil, return the current history count as an int.
    // - Otherwise, arg0 is kind (0=name, 1=content, 2=voice), arg1 is index (0=most recent).
    if kind.is_nil() {
        return Ok(Variant::Int(game_data.history_manager.len() as i32));
    }

    let kind = match kind.as_int() {
        Some(v) => v,
        None => {
            log::error!("history_get: unexpected kind: {:?}", kind);
            return Ok(Variant::Nil);
        }
    };

    let idx = match idx.as_int() {
        Some(v) => v,
        None => {
            log::error!("history_get: unexpected idx: {:?}", idx);
            return Ok(Variant::Nil);
        }
    };

    if idx < 0 {
        return Ok(Variant::Nil);
    }

    let value = match kind.try_into() {
        Ok(HistoryFunction::Name) => match game_data.history_manager.get_name(idx as u32) {
            Some(s) => Variant::String(s),
            None => Variant::Nil,
        },
        Ok(HistoryFunction::Content) => match game_data.history_manager.get_content(idx as u32) {
            Some(s) => Variant::String(s),
            None => Variant::Nil,
        },
        Ok(HistoryFunction::Voice) => match game_data.history_manager.get_voice(idx as u32) {
            Some(i) => Variant::Int(i),
            None => Variant::Nil,
        },
        _ => {
            log::error!("history_get: unexpected kind value: {:?}", kind);
            Variant::Nil
        }
    };

    Ok(value)
}


///
/// Retrieves a value from the history manager.
/// Arguments:
/// Arg1: kind (int or nil) - 0=name, 1=content, 2=voice. If nil, returns the history count.
/// Arg2: idx (int) - 0 means the most recent entry, 1 the one before that, etc.
/// 
pub struct HistoryGet;
impl Syscaller for HistoryGet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        history_get(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for HistoryGet {}
unsafe impl Sync for HistoryGet {}

///
/// Sets a certain value in the history manager.
/// Arguments:
/// Arg1: fnid (int or nil) - The kind of value to set.
/// Arg2: The value
/// 
pub struct HistorySet;
impl Syscaller for HistorySet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        history_set(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for HistorySet {}
unsafe impl Sync for HistorySet {}
