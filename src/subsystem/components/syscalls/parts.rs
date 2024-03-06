use anyhow::{bail, Result};

use crate::subsystem::world::GameData;
use crate::script::Variant;

use super::{get_var, Syscaller};

pub fn parts_load(game_data: &mut GameData, id: &Variant, path: &Variant) -> Result<Variant> {

    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("parts_load: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_load: id should be in range 0..64");
    }

    let path = match path {
        Variant::String(path) => path,
        _ => bail!("parts_load: invalid path type"),
    };

    game_data.parts_manager.load_parts(id as u16, path);
    Ok(Variant::Nil)
}

pub fn parts_rgb(game_data: &mut GameData, id: &Variant, r: &Variant, g: &Variant, b: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("parts_rgb: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_rgb: id should be in range 0..64");
    }

    let r = match r {
        Variant::Int(r) => {
            if *r >= 0 && *r <= 200 {
                *r
            } else {
                100
            }
        },
        _ => bail!("parts_rgb: invalid r type"),
    };

    let g = match g {
        Variant::Int(g) => {
            if *g >= 0 && *g <= 200 {
                *g
            } else {
                100
            }
        }
        _ => bail!("parts_rgb: invalid g type"),
    };

    let b = match b {
        Variant::Int(b) => {
            if *b >= 0 && *b <= 200 {
                *b
            } else {
                100
            }
        }
        _ => bail!("parts_rgb: invalid b type"),
    };

    game_data.parts_manager.set_rgb(id as u16, r as u8, g as u8, b as u8);
    Ok(Variant::Nil)
}


