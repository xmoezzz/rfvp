use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};


/// prepare a save data for writing
/// this syscall has 4 functions:
/// 1. set the scene title of the save
/// 2. set the title of the save
/// 3. set the script content of the save
/// 4. asynchronously write the save data to disk
pub fn save_create(game_data: &mut GameData, fnid: &Variant, value: &Variant) -> Result<Variant> {
    let fnid = match fnid {
        Variant::Int(fnid) => *fnid,
        _ => {
            log::error!("save_create: invalid fnid type: {:?}", fnid);
            return Ok(Variant::Nil);
        },
    };

    match fnid {
        0 => {
            if let Variant::ConstString(title, _) | Variant::String(title) = value {
                game_data.save_manager.set_current_scene_title(title.to_string());
            }
        },
        1 => {
            if let Variant::ConstString(title, _) | Variant::String(title) = value {
                game_data.save_manager.set_current_title(title.to_string());
            }
        },
        2 => {
            if let Variant::ConstString(content, _) | Variant::String(content) = value {
                game_data.save_manager.set_current_script_content(content.to_string());
            }
        },
        3 => {
            if let Variant::Int(slot) = value {
                let slot = *slot as u32;
                if (0..1000).contains(&slot) {
                    game_data.save_manager.asynchronously_save(slot);
                }
            }
        },
        _ => {
            log::error!("save_create: invalid fnid: {}", fnid);
        },
    }

    Ok(Variant::Nil)
}

pub fn save_data(game_data: &mut GameData, fnid: &Variant, value: &Variant) -> Result<Variant> {
    let fnid = match fnid {
        Variant::Int(fnid) => *fnid,
        _ => {
            log::error!("save_data: invalid fnid type: {:?}", fnid);
            return Ok(Variant::Nil);
        },
    };

    match fnid {
        0 => {
            if let Variant::Int(slot) = value {
                let slot = *slot as u32;
                game_data.save_manager.current_save_slot = slot;
            }
        },
        1 => {
            if let Variant::Int(slot) = value {
                let slot = *slot as u32;
                game_data.save_manager.current_save_slot = slot;
                game_data.save_manager.save_requested = true;
            }
        },
        _ => {
            log::error!("save_data: invalid fnid: {}", fnid);
        },
    }

    Ok(Variant::Nil)
}

pub fn save_thumb_size(game_data: &mut GameData, width: &Variant, height: &Variant) -> Result<Variant> {
    if let Variant::Int(width) = width {
        if let Variant::Int(height) = height {
            let width = *width;
            let height = *height;
            if (20..=200).contains(&width) && (15..=150).contains(&height) {
                game_data.save_manager.set_thumb_size(width as u32, height as u32);
            }
        }
    }
    
    Ok(Variant::Nil)
}