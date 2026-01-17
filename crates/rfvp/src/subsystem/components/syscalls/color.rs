use anyhow::{Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

pub fn color_set(
    game_data: &mut GameData,
    id: &Variant,
    r: &Variant,
    g: &Variant,
    b: &Variant,
    a: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("Invalid color id");
            return Ok(Variant::Nil);
        },
    };

    let id = id as u8;  // compiler optimization

    if !(0..=255).contains(&id) {
        log::error!("id must be in range 0..256");
        return Ok(Variant::Nil);
    }

    let color = game_data.motion_manager.color_manager.get_entry_mut(id as u8);
    if let Variant::Int(r) = r {
        color.set_r(*r as u8);
    }

    if let Variant::Int(g) = g {
        color.set_g(*g as u8);
    }

    if let Variant::Int(b) = b {
        color.set_b(*b as u8);
    }

    if let Variant::Int(a) = a {
        color.set_a(*a as u8);
    }

    Ok(Variant::Nil)
}


/// Set color value (rbga) for the corresponding slot
/// 
/// Arg1: color index (0~255)
/// Arg2: the red value (0~255)
/// Arg2: the green value (0~255)
/// Arg2: the blue value (0~255)
/// Arg2: the alpha value (0~255)
pub struct ColorSet;
impl Syscaller for ColorSet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        color_set(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
            get_var!(args, 3),
            get_var!(args, 4),
        )
    }
}

unsafe impl Send for ColorSet {}
unsafe impl Sync for ColorSet {}
