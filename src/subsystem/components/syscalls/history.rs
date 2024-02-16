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

pub fn history_get(game_data: &mut GameData, id: &Variant, fnid: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("history_get: unexpected id: {:?}", id);
            return Ok(Variant::Nil);
        }
    };

    let fnid = match fnid.as_int() {
        Some(id) => id,
        None => {
            log::error!("history_get: unexpected fnid: {:?}", fnid);
            return Ok(Variant::Nil);
        }
    };

    let value = match fnid.try_into() {
        Ok(HistoryFunction::Name) => {
            match game_data.history_manager.get_name(id as u32) {
                Some(s) => Variant::String(s),
                _ => Variant::Nil,
            }
        }
        Ok(HistoryFunction::Content) => {
            match game_data.history_manager.get_content(id as u32) {
                Some(s) => Variant::String(s),
                _ => Variant::Nil,
            }
        }
        Ok(HistoryFunction::Voice) => {
            match game_data.history_manager.get_voice(id as u32) {
                Some(i) => Variant::Int(i),
                _ => Variant::Nil,
            }
        }
        _ => {
            log::error!("history_get: unexpected fnid value: {:?}", fnid);
            return Ok(Variant::Nil);
        }
    };

    Ok(value)
}


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
