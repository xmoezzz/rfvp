use anyhow::{bail, Result};

use crate::subsystem::world::GameData;
use crate::{script::Variant, subsystem::resources::save_manager::SaveDataFunction};

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
        }
    };

    match fnid {
        0 => {
            if let Variant::ConstString(title, _) | Variant::String(title) = value {
                game_data
                    .save_manager
                    .set_current_scene_title(title.to_string());
            }
        }
        1 => {
            if let Variant::ConstString(title, _) | Variant::String(title) = value {
                game_data.save_manager.set_current_title(title.to_string());
            }
        }
        2 => {
            if let Variant::ConstString(content, _) | Variant::String(content) = value {
                game_data
                    .save_manager
                    .set_current_script_content(content.to_string());
            }
        }
        3 => {
            if let Variant::Int(slot) = value {
                let slot = *slot as u32;
                if (0..1000).contains(&slot) {
                    game_data.save_manager.asynchronously_save(slot);
                }
            }
        }
        _ => {
            log::error!("save_create: invalid fnid: {}", fnid);
        }
    }

    Ok(Variant::Nil)
}

pub fn save_data(
    game_data: &mut GameData,
    fnid: &Variant,
    value: &Variant,
    value2: &Variant,
) -> Result<Variant> {
    let fnid = match fnid {
        Variant::Int(fnid) => *fnid,
        Variant::Nil => {
            let nls = game_data.get_nls();
            for slot in 0..1000 {
                if let Err(e) = game_data.save_manager.load_savedata(slot, nls.clone()) {
                    log::error!("save_data: failed to load save data: {}", e);
                }
            }
            return Ok(Variant::Nil);
        }
        _ => {
            log::error!("save_data: invalid fnid type: {:?}", fnid);
            return Ok(Variant::Nil);
        }
    };

    match fnid.try_into() {
        Ok(SaveDataFunction::LoadSaveThumbToTexture) => {
            let mut slot_id = 0;
            let mut texture_id = 0;

            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    slot_id = slot as u32;
                } else {
                    return Ok(Variant::Nil);
                }
            } else {
                return Ok(Variant::Nil);
            }

            if let Some(texture) = value2.as_int() {
                if texture >= 0 || texture <= 4095 {
                    texture_id = texture as u32;
                } else {
                    return Ok(Variant::Nil);
                }
            } else {
                return Ok(Variant::Nil);
            }

            let thumb_width = game_data.save_manager.get_thumb_width();
            let thumb_height = game_data.save_manager.get_thumb_height();
            let thumb = game_data
                .save_manager
                .get_save_thumb(slot_id, thumb_width, thumb_height);
            let thumb = match thumb {
                Ok(thumb) => thumb,
                Err(e) => {
                    log::error!("save_data: failed to get save thumb: {}", e);
                    return Ok(Variant::Nil);
                }
            };

            if let Err(e) = game_data.motion_manager.load_texture_from_buff(
                texture_id as u16,
                thumb,
                thumb_width,
                thumb_height,
            ) {
                log::error!("save_data: failed to load texture from buff: {}", e);
                return Ok(Variant::Nil);
            }
        }
        Ok(SaveDataFunction::TestSaveData) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let test = game_data.save_manager.test_save_slot(slot as u32);
                    if test {
                        return Ok(Variant::True);
                    } else {
                        return Ok(Variant::Nil);
                    }
                }
            }
        }
        Ok(SaveDataFunction::DeleteSaveData) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    game_data.save_manager.delete_savedata(slot as u32);
                }
            }
        }
        Ok(SaveDataFunction::CopySaveData) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    if let Some(slot2) = value2.as_int() {
                        if slot2 >= 0 || slot2 <= 999 {
                            if let Err(e) = game_data
                                .save_manager
                                .copy_savedata(slot as u32, slot2 as u32)
                            {
                                log::error!("save_data: failed to copy save data: {}", e);
                            }
                        }
                    }
                }
            }
        }
        Ok(SaveDataFunction::GetSaveTitle) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let title = game_data.save_manager.get_save_title(slot as u32);
                    return Ok(Variant::String(title));
                }
            }
        }
        Ok(SaveDataFunction::GetSaveSceneTitle) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let title = game_data.save_manager.get_save_scene_title(slot as u32);
                    return Ok(Variant::String(title));
                }
            }
        }
        Ok(SaveDataFunction::GetScriptContent) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let content = game_data.save_manager.get_script_content(slot as u32);
                    return Ok(Variant::String(content));
                }
            }
        }
        Ok(SaveDataFunction::GetYear) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let year = game_data.save_manager.get_year(slot as u32);
                    return Ok(Variant::Int(year as i32));
                }
            }
        }
        Ok(SaveDataFunction::GetMonth) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let month = game_data.save_manager.get_month(slot as u32);
                    return Ok(Variant::Int(month as i32));
                }
            }
        }
        Ok(SaveDataFunction::GetDay) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let day = game_data.save_manager.get_day(slot as u32);
                    return Ok(Variant::Int(day as i32));
                }
            }
        }
        Ok(SaveDataFunction::GetDayOfWeek) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let day_of_week = game_data.save_manager.get_day_of_week(slot as u32);
                    return Ok(Variant::Int(day_of_week as i32));
                }
            }
        }
        Ok(SaveDataFunction::GetHour) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let hour = game_data.save_manager.get_hour(slot as u32);
                    return Ok(Variant::Int(hour as i32));
                }
            }
        }
        Ok(SaveDataFunction::GetMinute) => {
            if let Some(slot) = value.as_int() {
                if slot >= 0 || slot <= 999 {
                    let minute = game_data.save_manager.get_minute(slot as u32);
                    return Ok(Variant::Int(minute as i32));
                }
            }
        }
        Err(e) => {
            log::error!("save_data: invalid fnid: {}", e);
            return Ok(Variant::Nil);
        }
    }

    Ok(Variant::Nil)
}

pub fn save_thumb_size(
    game_data: &mut GameData,
    width: &Variant,
    height: &Variant,
) -> Result<Variant> {
    if let Variant::Int(width) = width {
        if let Variant::Int(height) = height {
            let width = *width;
            let height = *height;
            if (20..=200).contains(&width) && (15..=150).contains(&height) {
                game_data
                    .save_manager
                    .set_thumb_size(width as u32, height as u32);
            }
        }
    }

    Ok(Variant::Nil)
}

pub fn save_write(game_data: &mut GameData, slot: &Variant) -> Result<Variant> {
    let nls = game_data.get_nls();
    let cache = game_data.get_cache();
    if let Variant::Int(slot) = slot {
        let slot = *slot as u32;
        if (0..1000).contains(&slot) {
            game_data.save_manager.set_savedata_requested(true);
            game_data.save_manager.set_current_save_slot(slot);
            if game_data.save_manager.is_savedata_prepared() {
                if let Err(e) = game_data.save_manager.load_save_buff(slot, nls, &cache) {
                    log::error!("save_write: failed to load save buff: {}", e);
                }
            }
        }
    }

    Ok(Variant::Nil)
}

pub fn load(game_data: &mut GameData, slot: &Variant) -> Result<Variant> {
    if let Variant::Int(slot) = slot {
        let slot = *slot as u32;
        if (0..1000).contains(&slot) {
            game_data.save_manager.set_current_save_slot(slot);
            game_data.save_manager.set_should_load(true);
            game_data.thread_manager.set_should_break(true);
        }
    }

    Ok(Variant::Nil)
}

pub struct SaveCreate;
impl Syscaller for SaveCreate {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        save_create(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for SaveCreate {}
unsafe impl Sync for SaveCreate {}

pub struct SaveData;
impl Syscaller for SaveData {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        save_data(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
        )
    }
}

unsafe impl Send for SaveData {}
unsafe impl Sync for SaveData {}

pub struct SaveThumbSize;
impl Syscaller for SaveThumbSize {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        save_thumb_size(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for SaveThumbSize {}
unsafe impl Sync for SaveThumbSize {}

pub struct SaveWrite;
impl Syscaller for SaveWrite {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        save_write(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for SaveWrite {}
unsafe impl Sync for SaveWrite {}

pub struct Load;
impl Syscaller for Load {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        load(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for Load {}
unsafe impl Sync for Load {}
