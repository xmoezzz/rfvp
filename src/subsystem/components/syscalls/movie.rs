use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::resources::videoplayer::MovieMode;
use crate::subsystem::world::GameData;

use super::Syscaller;

/// Movie(path, flag)
///
/// flag == 0:
///   - Normal playback (video + audio later)
///   - Pause other engine actions (halt VM loop + skip scheduler)
///
/// flag != 0:
///   - Render as a layer (video only)
///   - Engine continues
pub fn movie_play(game_data: &mut GameData, path: &Variant, flag: &Variant) -> Result<Variant> {
    let path = match path {
        Variant::String(path) | Variant::ConstString(path, _) => path.as_str(),
        _ => return Ok(Variant::Nil),
    };

    let flag_i = flag.as_int().unwrap_or(0);

    // Cross-platform restriction: old formats (wmv/mpg) are remapped to mp4 (H264/AAC).
    let mapped = map_legacy_movie_ext_to_mp4(path);

    // Prefer mapped name, but fall back to the original if not found.
    let bytes = match game_data.vfs.read_file(&mapped) {
        Ok(b) => b,
        Err(_) => match game_data.vfs.read_file(path) {
            Ok(b) => b,
            Err(e) => {
                log::error!("Movie: failed to read {} (mapped from {}): {:?}", mapped, path, e);
                return Ok(Variant::Nil);
            }
        },
    };

    let (w, h) = (game_data.get_width() as u32, game_data.get_height() as u32);

    let mode = if flag_i == 0 {
        // Modal playback: freeze other actions.
        game_data.set_halt(true);
        game_data.thread_wrapper.should_break();
        MovieMode::ModalWithAudio
    } else {
        MovieMode::LayerNoAudio
    };

    let audio_manager = if matches!(mode, MovieMode::ModalWithAudio) {
        Some(game_data.audio_manager())
    } else {
        None
    };

    game_data.video_manager.start(
        bytes,
        mode,
        w,
        h,
        &mut game_data.motion_manager,
        audio_manager,
    )?;

    Ok(Variant::Nil)
}

pub fn movie_state(game_data: &mut GameData, _arg: &Variant) -> Result<Variant> {
    if game_data.video_manager.is_playing() {
        Ok(Variant::True)
    } else {
        Ok(Variant::Nil)
    }
}

pub fn movie_stop(game_data: &mut GameData) -> Result<Variant> {
    game_data.video_manager.stop(&mut game_data.motion_manager);
    // If Movie was modal, allow the engine to resume.
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
