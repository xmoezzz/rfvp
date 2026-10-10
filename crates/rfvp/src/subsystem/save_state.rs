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

        bgm_player
            .apply_snapshot_v1(&self.audio.bgm, vfs)
            .context("apply BgmPlayerSnapshotV1")?;

        se_player
            .apply_snapshot_v1(&self.audio.se, vfs)
            .context("apply SePlayerSnapshotV1")?;

        {
            let mut g = GLOBAL.lock().unwrap();
            g.restore_non_volatile(&self.globals_non_volatile);
        }

        tm.apply_snapshot_v1(&self.vm);
        Ok(())
    }
}

pub fn append_state_chunk_v1(out: &mut Vec<u8>, snap: &SaveStateSnapshotV1) -> Result<()> {
    let payload = snapshot_codec::encode(snap).context("serialize SaveStateSnapshotV1")?;
    snapshot_codec::append_chunk(
        out,
        &payload,
        snapshot_codec::STATE_MAGIC,
        MAX_STATE_PAYLOAD_BYTES,
    )
    .context("SaveStateSnapshotV1")
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
