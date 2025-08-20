use anyhow::{Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

/// stupid api
/// 11111111|111 means id_bit_pos can not larger than 2047
/// the highest 8 bits is used for id
/// the lowest 3 bits is used for the bits position
pub fn flag_set(game_data: &mut GameData, id_bit_pos: &Variant, on: &Variant) -> Result<Variant> {
    let id_bit_pos = if let Variant::Int(id_bit_pos) = id_bit_pos {
        *id_bit_pos
    } else {
        log::error!("set_flag: Invalid id_bit_pos type");
        return Ok(Variant::Nil);
    };

    let on = on.canbe_true();

    if !(0..=2047).contains(&id_bit_pos) {
        log::error!("set_flag: invalid id_bit_pos : {}", id_bit_pos);
        return Ok(Variant::Nil);
    }

    let id = (id_bit_pos / 8) as u8;
    let bits = (id_bit_pos & 7) as u8;

    game_data.flag_manager.set_flag(id, bits, on);
    Ok(Variant::Nil)
}

pub fn flag_get(game_data: &mut GameData, id_bit_pos: &Variant) -> Result<Variant> {
    let id_bit_pos = if let Variant::Int(id_bit_pos) = id_bit_pos {
        *id_bit_pos
    } else {
        log::error!("get_flag: Invalid id_bit_pos type");
        return Ok(Variant::Nil);
    };

    if !(0..=2047).contains(&id_bit_pos) {
        log::error!("get_flag: invalid id_bit_pos : {}", id_bit_pos);
        return Ok(Variant::Nil);
    }

    let id = (id_bit_pos / 8) as u8;
    let bits = (id_bit_pos & 7) as u8;

    if game_data.flag_manager.get_flag(id, bits) {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

pub struct FlagSet;
impl Syscaller for FlagSet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        flag_set(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for FlagSet {}
unsafe impl Sync for FlagSet {}

pub struct FlagGet;
impl Syscaller for FlagGet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        flag_get(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for FlagGet {}
unsafe impl Sync for FlagGet {}
