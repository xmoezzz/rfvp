use anyhow::{Context, Result};

use std::path::PathBuf;
use std::fs;
use std::io;
use std::path::Component;
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;

use crate::script::Variant;
use crate::subsystem::resources::videoplayer::MovieMode;
use crate::subsystem::world::GameData;
use crate::utils::file::app_base_path;

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

    // Movie file resolution:
    // - For `.wmv`/`.asf`: prefer the original extension (native WMV pipeline), then fall back to `.mp4`.
    // - For `.mpg`/`.mpeg`: keep historical behavior, map to `.mp4`.
    // - Otherwise: use the original path.
    let candidates = movie_path_candidates(path);

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

    // `VideoPlayerManager::start()` requires a real filesystem path.
    // Movie paths coming from scripts are often VFS logical paths.
    // We resolve each candidate as:
    // 1) `<game_root>/<candidate>` if it exists (0-copy)
    // 2) persistent cache file if previously extracted
    // 3) extract once from VFS/pack into persistent cache, then open
    let mut started = false;
    for cand in candidates {
        let cand = normalize_vfs_path(&cand);
        let real_path = match resolve_movie_real_path(game_data, &cand) {
            Ok(p) => p,
            Err(e) => {
                log::debug!("Movie: resolve failed for {} (orig {}): {e:?}", cand, path);
                continue;
            }
        };

        match game_data.video_manager.start(
            &real_path,
            mode,
            w,
            h,
            &mut game_data.motion_manager,
            audio_manager.clone(),
        ) {
            Ok(()) => {
                started = true;
                break;
            }
            Err(e) => {
                log::debug!(
                    "Movie: start failed for {} (orig {}): {e:?}",
                    real_path.display(),
                    path
                );
                continue;
            }
        }
    }

    if !started {
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


fn normalize_vfs_path(p: &str) -> String {
    // Scripts usually use forward slashes, but some ports may pass backslashes.
    // Normalize to forward slashes for VFS resolution.
    let p = p.replace('\\', "/");
    // Strip leading "./".
    let p = p.strip_prefix("./").unwrap_or(&p);
    p.to_string()
}

fn safe_rel_path_from_vfs(path: &str) -> anyhow::Result<PathBuf> {
    // Prevent directory traversal when writing cache files.
    let mut out = PathBuf::new();
    for c in PathBuf::from(path).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            // Reject absolute paths, prefixes, and parent traversal.
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                anyhow::bail!("invalid vfs path for cache: {path}");
            }
        }
    }
    if out.as_os_str().is_empty() {
        anyhow::bail!("empty vfs path for cache: {path}");
    }
    Ok(out)
}

fn video_cache_root_dir() -> PathBuf {
    // Preferred: `<game_root>/.rfvp_cache/video`.
    // Fallback: OS-specific cache dir (writable on mobile).
    let preferred = app_base_path().join(".rfvp_cache").join("video").get_path().clone();
    if fs::create_dir_all(&preferred).is_ok() {
        return preferred;
    }

    let fallback = ProjectDirs::from("com", "xmoezzz", "rfvp")
        .map(|d| d.cache_dir().join("video"))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            .join(".rfvp_cache")
            .join("video"));
    let _ = fs::create_dir_all(&fallback);
    fallback
}

fn resolve_movie_real_path(game_data: &GameData, mapped: &str) -> anyhow::Result<PathBuf> {
    // 1) Direct filesystem path under game root.
    let fs_path = app_base_path().join(mapped).get_path().clone();
    if fs_path.exists() {
        return Ok(fs_path);
    }

    // 2) Persistent cache.
    let cache_root = video_cache_root_dir();
    let rel = safe_rel_path_from_vfs(mapped)?;
    let cache_path = cache_root.join(rel);

    if cache_path.exists() {
        return Ok(cache_path);
    }

    // 3) Extract once from VFS into the persistent cache.
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create cache dir {}", parent.display()))?;
    }

    let pid = std::process::id();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let tmp_name = match cache_path.file_name().and_then(|s| s.to_str()) {
        Some(name) => format!("{name}.part_{pid}_{now}"),
        None => format!("movie.part_{pid}_{now}"),
    };
    let tmp_path = cache_path.with_file_name(tmp_name);

    {
        let mut src = game_data.vfs.open_stream(mapped)
            .with_context(|| format!("vfs.open_stream({mapped})"))?;
        let mut dst = fs::File::create(&tmp_path)
            .with_context(|| format!("create cache tmp {}", tmp_path.display()))?;
        io::copy(&mut src, &mut dst)
            .with_context(|| format!("copy movie bytes to {}", tmp_path.display()))?;
        // Best-effort: ensure bytes are on disk before rename.
        let _ = dst.sync_all();
    }

    match fs::rename(&tmp_path, &cache_path) {
        Ok(()) => {}
        Err(e) if cache_path.exists() => {
            // Another thread/process may have populated the cache.
            let _ = fs::remove_file(&tmp_path);
            log::debug!("Movie cache race: {} already exists ({e:?})", cache_path.display());
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            return Err(anyhow::anyhow!("rename cache tmp {} -> {}: {e}", tmp_path.display(), cache_path.display()));
        }
    }

    Ok(cache_path)
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

    // Keep historical behavior for mpg/mpeg (always remap to mp4).
    if lower.ends_with(".mpg") || lower.ends_with(".mpeg") {
        return vec![replace_ext(path, "mp4")];
    }

    // Prefer native WMV pipeline, then fall back to mp4.
    if lower.ends_with(".wmv") || lower.ends_with(".asf") {
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
