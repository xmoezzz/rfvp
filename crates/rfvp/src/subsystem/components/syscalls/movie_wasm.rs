use anyhow::{Context, Result};
use std::io::Read;
use std::path::PathBuf;

use crate::script::Variant;
use crate::subsystem::resources::videoplayer::MovieMode;
use crate::subsystem::world::GameData;

use super::Syscaller;

pub fn movie_play(game_data: &mut GameData, path: &Variant, flag: &Variant) -> Result<Variant> {
    let path = match path {
        Variant::String(path) | Variant::ConstString(path, _) => path.as_str(),
        _ => return Ok(Variant::Nil),
    };

    let is_layer_effect = flag.is_nil();
    let mode = if is_layer_effect {
        MovieMode::LayerNoAudio
    } else {
        MovieMode::ModalWithAudio
    };

    let (w, h) = (game_data.get_width() as u32, game_data.get_height() as u32);
    let audio_manager = if matches!(mode, MovieMode::ModalWithAudio) {
        Some(game_data.audio_manager())
    } else {
        None
    };

    for cand in movie_path_candidates(path) {
        let cand = normalize_vfs_path(&cand);
        let bytes = match read_movie_bytes(game_data, &cand) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::debug!("wasm Movie: resolve/read failed for {} (orig {}): {e:?}", cand, path);
                continue;
            }
        };

        let start_result = {
            let (video_manager, motion_manager) = (&mut game_data.video_manager, &mut game_data.motion_manager);
            video_manager.start_from_bytes(
                &cand,
                bytes,
                mode,
                w,
                h,
                motion_manager,
                audio_manager.clone(),
            )
        };

        match start_result {
            Ok(()) => {
                if matches!(mode, MovieMode::ModalWithAudio) {
                    game_data.set_halt(true);
                    game_data.thread_wrapper.should_break();
                }
                return Ok(Variant::True);
            }
            Err(e) => {
                log::debug!("wasm Movie: start failed for {} (orig {}): {e:?}", cand, path);
                continue;
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
    game_data.set_halt(false);
    Ok(Variant::Nil)
}

fn read_movie_bytes(game_data: &GameData, mapped: &str) -> Result<Vec<u8>> {
    let mut src = game_data
        .vfs
        .open_stream(mapped)
        .with_context(|| format!("wasm vfs.open_stream({mapped})"))?;
    let mut bytes = Vec::new();
    src.read_to_end(&mut bytes)
        .with_context(|| format!("wasm read movie bytes {mapped}"))?;
    Ok(bytes)
}

fn normalize_vfs_path(p: &str) -> String {
    let p = p.replace('\\', "/");
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

#[allow(dead_code)]
fn safe_rel_path_from_vfs(path: &str) -> anyhow::Result<PathBuf> {
    let mut out = PathBuf::new();
    for c in PathBuf::from(path).components() {
        match c {
            std::path::Component::Normal(s) => out.push(s),
            std::path::Component::CurDir => {}
            std::path::Component::RootDir
            | std::path::Component::Prefix(_)
            | std::path::Component::ParentDir => anyhow::bail!("invalid vfs path: {path}"),
        }
    }
    if out.as_os_str().is_empty() {
        anyhow::bail!("empty vfs path");
    }
    Ok(out)
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
