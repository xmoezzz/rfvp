use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;


pub fn prim_exit_group(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_exit_group: invalid id : {:?}", id),
    };

    if !(0..=4095).contains(&id) {
        bail!("prim_exit_group: invalid id : {}", id);
    }

    Ok(Variant::Nil)
}

pub fn prim_group_in(game_data: &mut GameData, id: &Variant, id2: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_group_in: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_group_in: invalid id : {}", id);
    }

    let id2 = match id2.as_int() {
        Some(id2) => id2,
        None => bail!("prim_group_in: invalid id2 : {:?}", id2),
    };

    if !(0..=4095).contains(&id2) {
        bail!("prim_group_in: invalid id2 : {}", id2);
    }

    game_data.prim_manager.set_prim_group_in(id2, id);

    Ok(Variant::Nil)
}

pub fn prim_group_move(game_data: &mut GameData, id: &Variant, id2: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_group_move: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_group_move: invalid id : {}", id);
    }

    let id2 = match id2.as_int() {
        Some(id2) => id2,
        None => bail!("prim_group_move: invalid id2 : {:?}", id2),
    };

    if !(1..=4095).contains(&id2) {
        bail!("prim_group_move: invalid id2 : {}", id2);
    }

    game_data.prim_manager.prim_move(id2, id);

    Ok(Variant::Nil)
}

pub fn prim_group_out(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_group_out: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_group_out: invalid id : {}", id);
    }

    game_data.prim_manager.unlink_prim(id as i16);

    Ok(Variant::Nil)
}
