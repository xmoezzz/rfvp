use anyhow::{bail, Result};

use crate::subsystem::world::GameData;
use crate::{script::Variant, subsystem::resources::text_manager::FONTFACE_MS_GOTHIC};

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

pub fn text_clear(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("text_clear: invalid id type"),
    };

    if !(0..32).contains(&id) {
        bail!("text_clear: id should be in range 0..32");
    }

    game_data.text_manager.set_text_clear(id);
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

// ＭＳ ゴシック
// ＭＳ 明朝
// ＭＳ Ｐゴシック
// ＭＳ Ｐ明朝
pub fn text_font(
    game_data: &mut GameData,
    id: &Variant,
    font_id: &Variant,
    font_id2: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("text_font: invalid id type"),
    };

    if !(0..32).contains(&id) {
        bail!("text_font: id should be in range 0..32");
    }

    let font_id = match font_id {
        Variant::Int(id) => *id,
        _ => bail!("text_font: invalid font_id type"),
    };

    let max_count = game_data.fontface_manager.get_font_count();

    if font_id >= -5 && font_id < max_count && max_count != 0 {
        game_data.text_manager.set_font_name(id, font_id);
    } else {
        game_data.text_manager.set_font_name(id, FONTFACE_MS_GOTHIC);
    }

    let font_id2 = match font_id2 {
        Variant::Int(id) => *id,
        _ => bail!("text_font: invalid font_id2 type"),
    };

    if font_id2 >= -5 && font_id2 < max_count && max_count != 0 {
        game_data.text_manager.set_font_text(id, font_id2);
    } else {
        game_data.text_manager.set_font_text(id, FONTFACE_MS_GOTHIC);
    }

    Ok(Variant::Nil)
}


pub fn text_font_count(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.fontface_manager.get_font_count()))
}


pub fn text_font_get(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.fontface_manager.get_system_fontface_id()))
}

pub fn text_font_name(game_data: &GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("text_font_name: invalid id type"),
    };

    match game_data.fontface_manager.get_font_name(id) {
        Some(name) => Ok(Variant::String(name)),
        None => Ok(Variant::Nil),
    }
}

pub fn text_set_font(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("text_set_font: invalid id type"),
    };

    if id >= -5 && id < game_data.fontface_manager.get_font_count() {
        if let Some(font_name) = game_data.fontface_manager.get_font_name(id) {
            game_data.fontface_manager.set_system_fontface_id(id);
            game_data.fontface_manager.set_current_font_name(&font_name)
        }
    } else {
        game_data.fontface_manager.set_system_fontface_id(FONTFACE_MS_GOTHIC);
        game_data.fontface_manager.set_current_font_name("ＭＳ ゴシック");
    }
    game_data.fontface_manager.set_system_fontface_id(id);
    Ok(Variant::Nil)
}

