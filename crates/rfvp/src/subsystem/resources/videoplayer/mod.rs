//! Movies (`MoviePlay`, `MovieState`, `MovieStop`).
//!
//! The manager here is the same everywhere: it puts the movie on its own layer above the game,
//! uploads each frame as it becomes due and notices the end. Decoding is per platform:
//!
//! - [`threaded`]: desktops and phones, decoding on threads from a file, with sound;
//! - [`inline`]: UEFI, the browser and builds without audio, decoding on the engine's thread
//!   from bytes read through the VFS;
//! - the `no_std` ports have no decoder yet, so a movie does not start and the script carries on.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::Result;

use super::motion_manager::MotionManager;
use super::prim::PrimType;
use super::vfs::Vfs;
use crate::rfvp_audio::AudioManager;

#[cfg(all(
    not(feature = "no_std"),
    any(feature = "native-video", feature = "uefi-native-video"),
    any(
        target_os = "uefi",
        target_arch = "wasm32",
        not(feature = "audio")
    )
))]
mod inline;
#[cfg(all(
    not(feature = "no_std"),
    not(target_os = "uefi"),
    not(target_arch = "wasm32"),
    feature = "native-video",
    feature = "audio"
))]
mod threaded;

/// GraphBuff slot reserved for Movie textures.
///
/// Text buffers use 4064..4095, so we pick 4063.
pub const MOVIE_GRAPH_ID: u16 = 4063;

/// Reserved primitive ids for the Movie layer.
/// These are internal slots; public scripts generally operate on 1..=4095.
pub const MOVIE_GROUP_PRIM_ID: i16 = 4095;
pub const MOVIE_SPRT_PRIM_ID: i16 = 4094;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieMode {
    /// Normal playback: video + audio while freezing other engine actions.
    ModalWithAudio,
    /// Render as a layer: video only, engine continues.
    LayerNoAudio,
}

/// A decoded frame, RGBA.
pub struct MovieFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A movie being decoded.
pub trait MovieSource: Send + Sync {
    /// The frame to show now, if a new one is due.
    fn next_frame(&mut self) -> Result<Option<MovieFrame>>;

    fn is_finished(&mut self) -> Result<bool>;

    /// Stop decoding and the soundtrack.
    fn stop(self: Box<Self>);
}

/// What a decoder needs to open a movie.
pub struct MovieRequest<'a> {
    /// The movie's path in the game's files.
    pub name: &'a str,
    pub mode: MovieMode,
    pub screen_w: u32,
    pub screen_h: u32,
    pub vfs: &'a Vfs,
    /// For the soundtrack; `None` plays the movie silently.
    pub audio: Option<Arc<AudioManager>>,
}

#[cfg(all(
    not(feature = "no_std"),
    not(target_os = "uefi"),
    not(target_arch = "wasm32"),
    feature = "native-video",
    feature = "audio"
))]
use threaded::open;

#[cfg(all(
    not(feature = "no_std"),
    any(feature = "native-video", feature = "uefi-native-video"),
    any(
        target_os = "uefi",
        target_arch = "wasm32",
        not(feature = "audio")
    )
))]
use inline::open;

#[cfg(any(
    feature = "no_std",
    not(any(feature = "native-video", feature = "uefi-native-video"))
))]
fn open(request: &MovieRequest<'_>) -> Result<Box<dyn MovieSource>> {
    anyhow::bail!("no movie decoder in this build: {}", request.name)
}

/// A Movie manager that wires the syscall to the render tree.
#[derive(Default)]
pub struct VideoPlayerManager {
    playing: bool,
    modal: bool,
    source: Option<Box<dyn MovieSource>>,
}

impl core::fmt::Debug for VideoPlayerManager {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VideoPlayerManager")
            .field("playing", &self.playing)
            .field("modal", &self.modal)
            .field("loaded", &self.source.is_some())
            .finish()
    }
}

impl VideoPlayerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Whether a movie graph/resources are currently loaded.
    ///
    /// This matches the original engine's `is_loaded` flag used by `MovieState(1)`.
    pub fn is_loaded(&self) -> bool {
        self.source.is_some()
    }

    pub fn is_modal_active(&self) -> bool {
        self.playing && self.modal
    }

    /// Start a movie, replacing the one playing.
    pub fn start(&mut self, request: MovieRequest<'_>, motion: &mut MotionManager) -> Result<()> {
        if self.playing {
            self.stop(motion);
        }

        let source = open(&request)?;
        self.playing = true;
        self.modal = matches!(request.mode, MovieMode::ModalWithAudio);
        self.source = Some(source);
        self.ensure_layer(motion, request.screen_w as i16, request.screen_h as i16);

        // Prime the first frame.
        self.tick(motion)
    }

    /// Show the frame that is due, and stop at the end. Runs every frame, also while the VM is
    /// halted for a modal movie.
    pub fn tick(&mut self, motion: &mut MotionManager) -> Result<()> {
        if !self.playing {
            return Ok(());
        }
        let Some(source) = self.source.as_mut() else {
            self.playing = false;
            self.modal = false;
            return Ok(());
        };

        if let Some(frame) = source.next_frame()? {
            motion.load_texture_from_buff(MOVIE_GRAPH_ID, frame.rgba, frame.width, frame.height)?;
            motion.refresh_prims(MOVIE_GRAPH_ID);
        }
        if source.is_finished()? {
            self.stop(motion);
        }
        Ok(())
    }

    pub fn stop(&mut self, motion: &mut MotionManager) {
        self.playing = false;
        self.modal = false;

        // Hide and unlink the movie layer primitives.
        // Keep the GraphBuff intact; it can be overwritten on next start.
        let pm = &mut motion.prim_manager;
        pm.prim_set_draw(MOVIE_SPRT_PRIM_ID as i32, 0);
        pm.prim_set_draw(MOVIE_GROUP_PRIM_ID as i32, 0);
        pm.unlink_prim(MOVIE_SPRT_PRIM_ID);
        pm.unlink_prim(MOVIE_GROUP_PRIM_ID);

        if let Some(source) = self.source.take() {
            source.stop();
        }
    }

    fn ensure_layer(&self, motion: &mut MotionManager, screen_w: i16, screen_h: i16) {
        let pm = &mut motion.prim_manager;

        // Match the original engine draw order: the movie layer belongs to the root=0 prim tree
        // (drawn in the "root0" pass), not the custom/overlay root.
        let root = 0i32;

        // Group container.
        pm.prim_init_with_type(MOVIE_GROUP_PRIM_ID, PrimType::PrimTypeGroup);
        pm.prim_set_pos(MOVIE_GROUP_PRIM_ID as i32, 0, 0);
        pm.prim_set_z(MOVIE_GROUP_PRIM_ID as i32, 32767);
        pm.prim_set_draw(MOVIE_GROUP_PRIM_ID as i32, 1);

        // Sprite: bind texture_id = -2 (movie).
        pm.prim_init_with_type(MOVIE_SPRT_PRIM_ID, PrimType::PrimTypeSprt);
        pm.prim_set_texture_id(MOVIE_SPRT_PRIM_ID as i32, -2);
        pm.prim_set_uv(MOVIE_SPRT_PRIM_ID as i32, 0, 0);
        pm.prim_set_pos(MOVIE_SPRT_PRIM_ID as i32, 0, 0);
        pm.prim_set_size(MOVIE_SPRT_PRIM_ID as i32, screen_w as i32, screen_h as i32);
        pm.prim_set_alpha(MOVIE_SPRT_PRIM_ID as i32, 255);
        pm.prim_set_blend(MOVIE_SPRT_PRIM_ID as i32, 0);
        pm.prim_set_z(MOVIE_SPRT_PRIM_ID as i32, 32767);
        pm.prim_set_draw(MOVIE_SPRT_PRIM_ID as i32, 1);

        // Attach sprite to movie group, then group to root (append-to-end semantics).
        pm.set_prim_group_in(MOVIE_GROUP_PRIM_ID as i32, MOVIE_SPRT_PRIM_ID as i32);
        pm.set_prim_group_in(root, MOVIE_GROUP_PRIM_ID as i32);
    }
}
