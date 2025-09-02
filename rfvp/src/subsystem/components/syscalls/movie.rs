use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{Syscaller, get_var};

pub fn movie_play(game_data: &mut GameData, path: &Variant, _flag: &Variant) -> Result<Variant> {
    let path = match path {
        Variant::String(path) | Variant::ConstString(path, _) => path,
        _ => return Ok(Variant::Nil),
    };

    let width = game_data.get_width();
    let height = game_data.get_height();
    game_data.video_manager.play(path, width, height)?;
    Ok(Variant::Nil)
}

pub fn movie_state(game_data: &GameData) -> Result<Variant> {
    if game_data.video_manager.is_playing() {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

pub fn movie_stop(game_data: &mut GameData) -> Result<Variant> {
    game_data.video_manager.stop();
    Ok(Variant::Nil)
}


pub struct Movie;
impl Syscaller for Movie {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        movie_play(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for Movie {}
unsafe impl Sync for Movie {}

pub struct MovieState;
impl Syscaller for MovieState {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        movie_state(game_data)
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
