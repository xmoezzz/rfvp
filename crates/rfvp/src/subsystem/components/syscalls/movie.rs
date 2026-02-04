use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::resources::videoplayer::MovieMode;
use crate::subsystem::world::GameData;

use super::Syscaller;

/// Movie(path, flag)
///
/// Original-engine semantics (IDA):
/// - `flag` is treated as a boolean by **Type**, not by value.
///   - `flag == nil`  => effect/layer movie (video only)
///   - `flag != nil`  => normal modal movie (video + audio)
///
/// In layer mode, the movie is rendered into a reserved GraphBuff slot and drawn via a reserved
/// sprite in the root=0 prim tree. In modal mode, we additionally halt script/scheduler execution
/// while the movie is playing.
pub fn movie_play(game_data: &mut GameData, path: &Variant, flag: &Variant) -> Result<Variant> {
    let path = match path {
        Variant::String(path) | Variant::ConstString(path, _) => path.as_str(),
        _ => return Ok(Variant::Nil),
    };

    // IMPORTANT: match original engine: nil vs non-nil, not integer truthiness.
    let is_layer_effect = flag.is_nil();

    // Cross-platform restriction: old formats (wmv/mpg) are remapped to mp4 (H264/AAC).
    let mapped = map_legacy_movie_ext_to_mp4(path);
    let mapped_path = std::path::Path::new(&mapped);

    let (w, h) = (game_data.get_width() as u32, game_data.get_height() as u32);

    let mode = if is_layer_effect {
        MovieMode::LayerNoAudio
    } else {
        MovieMode::ModalWithAudio
    };

    let audio_manager = if matches!(mode, MovieMode::ModalWithAudio) {
        Some(game_data.audio_manager())
    } else {
        None
    };

    // NOTE: In the original engine, Movie returns True on success and Nil on failure.
    if let Err(e) = game_data.video_manager.start(
        &mapped_path,
        mode,
        w,
        h,
        &mut game_data.motion_manager,
        audio_manager,
    ) {
        log::error!("Movie: start failed for {} (orig {}): {e:?}", mapped, path);
        return Ok(Variant::Nil);
    }

    if matches!(mode, MovieMode::ModalWithAudio) {
        // Freeze other actions while the modal movie is active.
        game_data.set_halt(true);
        game_data.thread_wrapper.should_break();
    }

    Ok(Variant::True)
}

/// MovieState(mode)
///
/// Original-engine semantics (IDA):
/// - `MovieState(0)` => True iff a movie is currently playing.
/// - `MovieState(1)` => True iff no movie is loaded (used to restart looping effect movies).
/// - Otherwise => Nil.
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
    // If Movie was modal, allow the engine to resume immediately.
    game_data.set_halt(false);
    Ok(Variant::Nil)
}

fn map_legacy_movie_ext_to_mp4(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".wmv") || lower.ends_with(".mpg") || lower.ends_with(".mpeg") {
        if let Some(idx) = path.rfind('.') {
            let mut s = path.to_string();
            s.replace_range(idx.., ".mp4");
            return s;
        }
    }
    path.to_string()
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
