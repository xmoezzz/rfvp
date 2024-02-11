use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

pub fn debug_message(game_data: &mut GameData, message: &str, var: &Variant) -> Result<Variant> {
    log::info!("DEBUG => {}: {:?}", message, var);
    Ok(Variant::Nil)
}

pub fn break_point(game_data: &mut GameData) -> Result<Variant> {
    log::info!("Break point");
    Ok(Variant::Nil)
}

pub fn float_to_int(game_data: &mut GameData, value: f32) -> Result<Variant> {
    Ok(Variant::Int(value as i32))
}

pub fn int_to_text(game_data: &mut GameData, value: i32, width: i32) -> Result<Variant> {
    let value = format!("{:width$}", value, width = width as usize);
    Ok(Variant::String(value))
}

pub fn rand(game_data: &mut GameData) -> Result<Variant> {
    Ok(Variant::Float(rand::random()))
}

pub fn system_project_dir(game_data: &mut GameData, _dir: &str) -> Result<Variant> {
    Ok(Variant::Nil)
}


pub fn system_at_skipname(game_data: &mut GameData, _arg0: &Variant, _arg1: &Variant) -> Result<Variant> {
    Ok(Variant::Nil)
}