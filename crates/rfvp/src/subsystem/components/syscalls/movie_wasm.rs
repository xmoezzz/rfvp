use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::Syscaller;

pub fn movie_play(_game_data: &mut GameData, _path: &Variant, _flag: &Variant) -> Result<Variant> {
    log::warn!("MoviePlay is disabled in the wasm build");
    Ok(Variant::Nil)
}

pub fn movie_state(game_data: &mut GameData, arg: &Variant) -> Result<Variant> {
    let Some(mode) = arg.as_int() else {
        return Ok(Variant::Nil);
    };

    match mode {
        0 => {
            if game_data.video_manager.is_playing() {
                Ok(Variant::True)
            } else {
                Ok(Variant::Nil)
            }
        }
        1 => {
            if !game_data.video_manager.is_loaded() {
                Ok(Variant::True)
            } else {
                Ok(Variant::Nil)
            }
        }
        _ => Ok(Variant::Nil),
    }
}

pub fn movie_stop(game_data: &mut GameData) -> Result<Variant> {
    game_data.video_manager.stop(&mut game_data.motion_manager);
    game_data.set_halt(false);
    Ok(Variant::Nil)
}

pub struct Movie;
impl Syscaller for Movie {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        movie_play(game_data, super::get_var!(args, 0), super::get_var!(args, 1))
    }
}

unsafe impl Send for Movie {}
unsafe impl Sync for Movie {}

pub struct MovieState;
impl Syscaller for MovieState {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        movie_state(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for MovieState {}
unsafe impl Sync for MovieState {}

pub struct MovieStop;
impl Syscaller for MovieStop {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        movie_stop(game_data)
    }
}

unsafe impl Send for MovieStop {}
unsafe impl Sync for MovieStop {}
