use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, errlog, Syscaller};

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
        _ => errlog!("Invalid color id"),
    };

    if !(0..256).contains(&id) {
        errlog!("id must be in range 0..256");
    }

    let color = game_data.color_manager.get_entry_mut(id as u8);
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
