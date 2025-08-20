//! Glue together `shin-core` and `kira` to provide an API to play NXA audio files.

mod data;
mod handle;
mod manager;
mod resampler;
mod sound;

pub use data::AudioData;
pub use handle::AudioHandle;
use kira::track::TrackId;
pub use manager::AudioManager;
pub use rfvp_core::format::audio::AudioFile;
use rfvp_core::{
    time::Tween,
    types::{Pan, Volume},
};

pub struct AudioSettings {
    pub track: TrackId,
    pub fade_in: Tween,
    pub loop_start: Option<u32>,
    pub volume: Volume,
    pub pan: Pan,
    // TODO: support play speed (needs research)
}
