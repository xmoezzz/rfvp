use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::resources::flag_manager::FlagManager;
use crate::subsystem::world::GameData;


/// stupid api
/// 11111111|111 means id_bit_pos can not larger than 2047
/// the highest 8 bits is used for id
/// the lowest 3 bits is used for the bits position
pub fn set_flag(game_data: &mut GameData, id_bit_pos: i32, on: bool) -> Result<Variant> {
    if !(0..=2047).contains(&id_bit_pos) {
        bail!("set_flag: invalid id_bit_pos : {}", id_bit_pos);
    }

    let id = (id_bit_pos / 8) as u8;
    let bits = (id_bit_pos & 7) as u8;

    game_data.flag_manager.set_flag(id, bits, on);
    Ok(Variant::Nil)
}


pub fn get_flag(game_data: &mut GameData, id_bit_pos: i32) -> Result<Variant> {
    if !(0..=2047).contains(&id_bit_pos) {
        bail!("get_flag: invalid id_bit_pos : {}", id_bit_pos);
    }

    let id = (id_bit_pos / 8) as u8;
    let bits = (id_bit_pos & 7) as u8;

    if game_data.flag_manager.get_flag(id, bits) {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

