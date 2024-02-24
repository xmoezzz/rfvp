use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

pub fn text_buff(
    game_data: &mut GameData,
    id: &Variant,
    w: &Variant,
    h: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("text_buff: invalid id type"),
    };

    if !(0..32).contains(&id) {
        bail!("text_buff: id should be in range 0..32");
    }

    let w = if let Variant::Int(w) = w {
        if *w < 0 {
            8
        } else {
            *w
        }
    } else {
        bail!("text_buff: invalid w type");
    };

    let h = if let Variant::Int(h) = h {
        if *h < 0 {
            8
        } else {
            *h
        }
    } else {
        bail!("text_buff: invalid h type");
    };

    game_data.text_manager.set_text_buff(id, w, h);
    Ok(Variant::Nil)
}

pub fn text_color(
    game_data: &mut GameData,
    id: &Variant,
    color1_id: &Variant,
    color2_id: &Variant,
    color3_id: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("text_color: invalid id type"),
    };

    if !(0..32).contains(&id) {
        bail!("text_color: id should be in range 0..32");
    }

    let color1_id = match color1_id {
        Variant::Int(id) => *id,
        _ => bail!("text_color: invalid color1_id type"),
    };

    if (0..256).contains(&color1_id) {
        let color = game_data.color_manager.get_entry(color1_id as u8);
        game_data.text_manager.set_text_color1(id, color);
    }

    let color2_id = match color2_id {
        Variant::Int(id) => *id,
        _ => bail!("text_color: invalid color2_id type"),
    };

    if (0..256).contains(&color2_id) {
        let color = game_data.color_manager.get_entry(color2_id as u8);
        game_data.text_manager.set_text_color2(id, color);
    }

    let color3_id = match color3_id {
        Variant::Int(id) => *id,
        _ => bail!("text_color: invalid color3_id type"),
    };

    if (0..256).contains(&color3_id) {
        let color = game_data.color_manager.get_entry(color3_id as u8);
        game_data.text_manager.set_text_color3(id, color);
    }

    Ok(Variant::Nil)
}

