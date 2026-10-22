#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::audio_player::{BgmPlayerSnapshotV1, SePlayerSnapshotV1};
use crate::script::global::GLOBAL;
use crate::script::Variant;
use crate::subsystem::resources::motion_manager::MotionManagerSnapshotV1;
use crate::subsystem::snapshot_codec;
use crate::subsystem::resources::thread_manager::{ThreadManager, ThreadManagerSnapshotV1};
use crate::subsystem::world::GameData;

const MAX_STATE_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSnapshotV1 {
    pub bgm: BgmPlayerSnapshotV1,
    pub se: SePlayerSnapshotV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveStateSnapshotV1 {
    pub version: u16,
    pub motion: MotionManagerSnapshotV1,
    pub audio: AudioSnapshotV1,
    /// Reverse-engineered slot save persists non-volatile globals together with the scene snapshot.
    pub globals_non_volatile: Vec<Variant>,
    /// Full VM coroutine state needed for resumable saves.
    pub vm: ThreadManagerSnapshotV1,
}

impl SaveStateSnapshotV1 {
    pub fn capture(game_data: &mut GameData) -> Self {
        let globals_non_volatile = GLOBAL.lock().unwrap().snapshot_non_volatile();

        let vm = match game_data.save_manager.take_pending_vm_snapshot() {
            Some(s) => s,
            None => {
                log::warn!("SaveStateSnapshotV1: missing VM snapshot; save will not be resumable");
                ThreadManager::new().capture_snapshot_v1()
            }
        };

        SaveStateSnapshotV1 {
            version: 1,
            motion: game_data.motion_manager.capture_snapshot_v1(),
            audio: AudioSnapshotV1 {
                bgm: game_data.bgm_player_ref().capture_snapshot_v1(),
                se: game_data.se_player_ref().capture_snapshot_v1(),
            },
            globals_non_volatile,
            vm,
        }
    }

    pub fn apply(&self, game_data: &mut GameData, tm: &mut ThreadManager) -> Result<()> {
        let started = crate::perf::start();
        let GameData {
            motion_manager,
            vfs,
            bgm_player,
            se_player,
            ..
        } = game_data;

        motion_manager
            .apply_snapshot_v1(&self.motion, vfs)
            .context("apply MotionManagerSnapshotV1")?;
        crate::perf::event(format_args!("save load graphics graphs={} prims={} us={}",
            self.motion.textures.len(), self.motion.prim_manager.prims.len(),
            crate::perf::start().saturating_sub(started)));

        let audio_started = crate::perf::start();
        bgm_player
            .apply_snapshot_v1(&self.audio.bgm, vfs)
            .context("apply BgmPlayerSnapshotV1")?;

        se_player
            .apply_snapshot_v1(&self.audio.se, vfs)
            .context("apply SePlayerSnapshotV1")?;
        crate::perf::event(format_args!("save load audio_state us={} (backend command execution may follow)",
            crate::perf::start().saturating_sub(audio_started)));

        {
            let mut g = GLOBAL.lock().unwrap();
            g.restore_non_volatile(&self.globals_non_volatile);
        }

        tm.apply_snapshot_v1(&self.vm);
        crate::perf::event(format_args!("save load apply total_us={}", crate::perf::start().saturating_sub(started)));
        Ok(())
    }
}

pub fn append_state_chunk_v1(out: &mut Vec<u8>, snap: &SaveStateSnapshotV1) -> Result<()> {
    #[cfg(feature = "no_std")]
    {
        let started = crate::perf::start();
        // Append directly to the file buffer. Postcard emits the same V1 payload;
        // avoid a second complete payload allocation and copy into the slot file.
        let start = out.len();
        let mut bytes = postcard::to_extend(snap, core::mem::take(out))
            .context("serialize SaveStateSnapshotV1")?;
        let length = bytes.len() - start;
        if length > MAX_STATE_PAYLOAD_BYTES {
            bytes.truncate(start);
            *out = bytes;
            bail!("SaveStateSnapshotV1 payload too large: {}", length);
        }
        let length = u32::try_from(length).context("SaveStateSnapshotV1 length overflow")?;
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&snapshot_codec::STATE_MAGIC);
        *out = bytes;
        crate::perf::event(format_args!("save encode state bytes={} us={}", length,
            crate::perf::start().saturating_sub(started)));
        Ok(())
    }
    #[cfg(not(feature = "no_std"))]
    {
    let payload = snapshot_codec::encode(snap).context("serialize SaveStateSnapshotV1")?;
    snapshot_codec::append_chunk(
        out,
        &payload,
        snapshot_codec::STATE_MAGIC,
        MAX_STATE_PAYLOAD_BYTES,
    )
    .context("SaveStateSnapshotV1")
    }
}

/// `append_state_chunk_v1(out, &SaveStateSnapshotV1::capture(game_data))` without the
/// snapshot: each part is captured, appended and dropped in turn, so the PlayStation never holds
/// the whole state twice. postcard writes a struct as its fields one after another, so the bytes
/// are the same.
#[cfg(feature = "no_std")]
pub fn append_state_chunk_streamed(out: &mut Vec<u8>, game_data: &mut GameData) -> Result<()> {
    fn put<T: Serialize>(out: &mut Vec<u8>, value: &T) -> Result<()> {
        let taken = core::mem::take(out);
        *out = postcard::to_extend(value, taken).context("serialize SaveStateSnapshotV1")?;
        Ok(())
    }
    let start = out.len();
    put(out, &1u16)?;
    put(out, &game_data.motion_manager.capture_snapshot_v1())?;
    put(
        out,
        &AudioSnapshotV1 {
            bgm: game_data.bgm_player_ref().capture_snapshot_v1(),
            se: game_data.se_player_ref().capture_snapshot_v1(),
        },
    )?;
    put(out, &GLOBAL.lock().unwrap().non_volatile_view())?;
    let vm = match game_data.save_manager.take_pending_vm_snapshot() {
        Some(s) => s,
        None => {
            log::warn!("SaveStateSnapshotV1: missing VM snapshot; save will not be resumable");
            ThreadManager::new().capture_snapshot_v1()
        }
    };
    put(out, &vm)?;
    drop(vm);
    let len = u32::try_from(out.len() - start).context("SaveStateSnapshotV1 too large")?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&snapshot_codec::STATE_MAGIC);
    Ok(())
}

pub fn try_decode_state_chunk_v1(file_bytes: &[u8]) -> Result<Option<SaveStateSnapshotV1>> {
    let Some(payload) = snapshot_codec::find_chunk(
        file_bytes,
        snapshot_codec::STATE_MAGIC,
        MAX_STATE_PAYLOAD_BYTES,
    )
    .context("SaveStateSnapshotV1")?
    else {
        return Ok(None);
    };
    let snap: SaveStateSnapshotV1 =
        snapshot_codec::decode(payload).context("deserialize SaveStateSnapshotV1")?;
    if snap.version != 1 {
        bail!("unsupported SaveStateSnapshotV1 version: {}", snap.version);
    }
    Ok(Some(snap))
}

#[cfg(all(test, not(feature = "no_std")))]
mod tests {
    use super::*;

    /// Slot files written by the desktop build must keep loading. Point `RFVP_TEST_SAVE` at a
    /// real `rfvp_sNNN.bin` to check one; without it the test does nothing.
    #[test]
    fn a_desktop_slot_file_still_decodes() {
        let Ok(path) = std::env::var("RFVP_TEST_SAVE") else {
            return;
        };
        let bytes = std::fs::read(&path).expect("read the save");
        let snap = try_decode_state_chunk_v1(&bytes)
            .expect("decode")
            .expect("the save has no state chunk");
        assert_eq!(snap.version, 1);
        println!("{path}: {} graphs, {} contexts", snap.motion.textures.len(), snap.vm.contexts.len());

        // Writing it back out and reading that again gives the same state.
        let mut again = Vec::new();
        append_state_chunk_v1(&mut again, &snap).expect("encode");
        let back = try_decode_state_chunk_v1(&again).expect("decode again").expect("no chunk");
        assert_eq!(back.motion.textures.len(), snap.motion.textures.len());
        assert_eq!(back.vm.contexts.len(), snap.vm.contexts.len());
    }
}
