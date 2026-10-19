//! The backend of the `no_std` ports: audio becomes commands the core hands to the host's
//! [`RfvpAudio`](crate::host_api::RfvpAudio) once per frame.

use alloc::vec::Vec;

use anyhow::Result;

use super::{AudioBackend, SoundSource, Tween};
use crate::host_api::{
    AudioByteStream, AudioParams, AudioStreamDesc, AudioStreamId, EncodedAudioKind,
};
use crate::subsystem::resources::vfs::Vfs;
use crate::utils::stable_hash::StableHashSet;
// `std` is the crate's own shim in `no_std` builds.
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub enum AudioCommand {
    LoadEncoded {
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: Vec<u8>,
    },
    /// A long sound (music), read from its file as it plays: see [`STREAM_FROM_BYTES`].
    LoadStream {
        id: AudioStreamId,
        kind: EncodedAudioKind,
        stream: AudioByteStream,
    },
    CreateStream {
        id: AudioStreamId,
        desc: AudioStreamDesc,
    },
    SubmitI16 {
        id: AudioStreamId,
        samples: Vec<i16>,
    },
    SubmitF32 {
        id: AudioStreamId,
        samples: Vec<f32>,
    },
    Play {
        id: AudioStreamId,
        params: AudioParams,
        fade_in_ms: u32,
    },
    Stop {
        id: AudioStreamId,
        fade_ms: u32,
    },
    Pause {
        id: AudioStreamId,
    },
    Resume {
        id: AudioStreamId,
    },
    SetParams {
        id: AudioStreamId,
        params: AudioParams,
    },
    DestroyStream {
        id: AudioStreamId,
    },
    MasterVolume {
        volume: f32,
    },
}

/// Sounds at least this long (in bytes) are streamed rather than read whole: the music of some
/// games runs to megabytes, which a console with little memory cannot hold.
#[cfg(not(feature = "psx"))]
pub const STREAM_FROM_BYTES: u64 = 1024 * 1024;
/// On the PlayStation every sound goes to the host as a stream: it copies sounds to sound RAM
/// a piece at a time, which keeps them out of the 2 MiB main RAM.
#[cfg(feature = "psx")]
pub const STREAM_FROM_BYTES: u64 = 0;

#[derive(Default)]
struct State {
    commands: Vec<AudioCommand>,
    /// Streams started and not stopped since. The host does not report when a sound ends.
    playing: StableHashSet<u32>,
}

pub struct CommandBackend {
    state: Mutex<State>,
}

impl Default for CommandBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandBackend {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        let mut state = self.state.lock().ok().expect("audio command lock");
        f(&mut state)
    }
}

impl AudioBackend for CommandBackend {
    fn load(&self, id: AudioStreamId, source: &SoundSource, vfs: &Vfs) -> Result<()> {
        crate::perf::event(format_args!("audio source id={} path={:?}", id.0, source.name()));
        let kind = source.encoded_kind();
        if let SoundSource::Named(name) = source {
            let (stream, len) = vfs.open_stream_with_len(name)?;
            if len.is_some_and(|len| len >= STREAM_FROM_BYTES) {
                let stream = AudioByteStream::new(stream);
                self.with(|state| {
                    state
                        .commands
                        .push(AudioCommand::LoadStream { id, kind, stream })
                });
                return Ok(());
            }
        }
        let bytes = source.read(vfs)?;
        self.with(|state| {
            state
                .commands
                .push(AudioCommand::LoadEncoded { id, kind, bytes })
        });
        Ok(())
    }

    fn unload(&self, id: AudioStreamId) {
        self.stop(id, Tween::default());
    }

    fn play(&self, id: AudioStreamId, params: AudioParams, fade_in: Tween, _vfs: &Vfs) -> Result<()> {
        self.with(|state| {
            state.playing.insert(id.0);
            state.commands.push(AudioCommand::Play {
                id,
                params,
                fade_in_ms: fade_in.duration_ms(),
            });
        });
        Ok(())
    }

    fn set_params(&self, id: AudioStreamId, params: AudioParams, _tween: Tween) {
        self.with(|state| state.commands.push(AudioCommand::SetParams { id, params }));
    }

    fn stop(&self, id: AudioStreamId, fade_out: Tween) {
        self.with(|state| {
            state.playing.remove(&id.0);
            state.commands.push(AudioCommand::Stop {
                id,
                fade_ms: fade_out.duration_ms(),
            });
        });
    }

    fn is_playing(&self, id: AudioStreamId) -> bool {
        self.with(|state| state.playing.contains(&id.0))
    }

    fn set_master_volume(&self, volume: f32) {
        self.with(|state| state.commands.push(AudioCommand::MasterVolume { volume }));
    }

    fn drain_commands(&self, out: &mut Vec<AudioCommand>) {
        self.with(|state| out.extend(state.commands.drain(..)));
    }
}
