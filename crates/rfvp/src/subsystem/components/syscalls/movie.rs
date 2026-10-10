#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::resources::videoplayer::{MovieMode, MovieRequest};
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
/// sprite in the root=0 prim tree. A non-nil flag enables movie audio, but does not halt the
/// script engine; the original executable keeps running script contexts while a movie is active.
pub fn movie_play(game_data: &mut GameData, path: &Variant, flag: &Variant) -> Result<Variant> {
    let path = match path {
        Variant::String(path) | Variant::ConstString(path, _) => path.as_str(),
        _ => return Ok(Variant::Nil),
    };

    let is_layer_effect = flag.is_nil();

    log::info!(
        "MoviePlay: path={} flag={:?} is_layer_effect={}",
        path,
        flag,
        is_layer_effect
    );

    let (w, h) = (game_data.get_width() as u32, game_data.get_height() as u32);

    let mode = if is_layer_effect {
        MovieMode::LayerNoAudio
    } else {
        MovieMode::ModalWithAudio
    };

    let audio = if matches!(mode, MovieMode::ModalWithAudio) {
        Some(game_data.audio_manager())
    } else {
        None
    };

    for cand in movie_path_candidates(path) {
        let cand = normalize_vfs_path(&cand);
        let request = MovieRequest {
            name: &cand,
            mode,
            screen_w: w,
            screen_h: h,
            vfs: &game_data.vfs,
            audio: audio.clone(),
        };
        match game_data
            .video_manager
            .start(request, &mut game_data.motion_manager)
        {
            Ok(()) => return Ok(Variant::True),
            Err(e) => {
                log::debug!("Movie: start failed for {} (orig {}): {e:?}", cand, path);
            }
        }
    }

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
    // If Movie was modal, allow the engine to resume immediately.
    game_data.set_halt(false);
    Ok(Variant::Nil)
}

fn normalize_vfs_path(p: &str) -> String {
    // Scripts usually use forward slashes, but some ports may pass backslashes.
    // Normalize to forward slashes for VFS resolution.
    let p = p.replace('\\', "/");
    // Strip leading "./".
    let p = p.strip_prefix("./").unwrap_or(&p);
    p.to_string()
}

fn movie_path_candidates(path: &str) -> Vec<String> {
    let lower = path.to_ascii_lowercase();

    fn replace_ext(p: &str, new_ext: &str) -> String {
        if let Some(idx) = p.rfind('.') {
            let mut s = p.to_string();
            s.replace_range(idx.., &format!(".{new_ext}"));
            s
        } else {
            format!("{p}.{new_ext}")
        }
    }

    // Prefer native pipelines first, then fall back to `.mp4` as the final failback.
    // - WMV: `.wmv` / `.asf`
    // - MPEG-PS/TS / MPEG-1/2 elementary: `.mpg` / `.mpeg` / `.m2v` / `.ts` / `.ps` / `.vob` / `.dat`
    let is_native = lower.ends_with(".wmv")
        || lower.ends_with(".asf")
        || lower.ends_with(".mpg")
        || lower.ends_with(".mpeg")
        || lower.ends_with(".m2v")
        || lower.ends_with(".ts")
        || lower.ends_with(".ps")
        || lower.ends_with(".vob")
        || lower.ends_with(".dat");

    if is_native {
        let mp4 = replace_ext(path, "mp4");
        if mp4 != path {
            return vec![path.to_string(), mp4];
        }
        return vec![path.to_string()];
    }

    vec![path.to_string()]
}

pub struct Movie;
impl Syscaller for Movie {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        movie_play(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
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
