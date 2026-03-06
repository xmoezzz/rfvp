use anyhow::{bail, Context, Result};
use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::audio_player::{BgmPlayerSnapshotV1, SePlayerSnapshotV1};
use crate::script::Variant;
use crate::script::global::GLOBAL;
use crate::subsystem::resources::motion_manager::MotionManagerSnapshotV1;
use crate::subsystem::resources::thread_manager::{ThreadManager, ThreadManagerSnapshotV1};
use crate::subsystem::world::GameData;

const SAVE_STATE_MAGIC: [u8; 4] = *b"RFVS";
const SAVE_STATE_FOOTER_LEN: usize = 8; // u32 payload_len + 4-byte magic
const MAX_STATE_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

fn bincode_opts() -> impl bincode::Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
}

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
}

impl SaveStateSnapshotV1 {
    pub fn capture(game_data: &GameData) -> Self {
        SaveStateSnapshotV1 {
            version: 1,
            motion: game_data.motion_manager.capture_snapshot_v1(),
            audio: AudioSnapshotV1 {
                bgm: game_data.bgm_player_ref().capture_snapshot_v1(),
                se: game_data.se_player_ref().capture_snapshot_v1(),
            },
        }
    }

    pub fn apply(&self, game_data: &mut GameData) -> Result<()> {
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

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveStateSnapshotV2 {
    pub version: u16,
    pub motion: MotionManagerSnapshotV1,
    pub audio: AudioSnapshotV1,
    /// Non-volatile globals (indices 0..non_volatile_count).
    pub globals_non_volatile: Vec<Variant>,
    /// Full VM coroutine state.
    pub vm: ThreadManagerSnapshotV1,
}

impl SaveStateSnapshotV2 {
    pub fn capture(game_data: &mut GameData) -> Self {
        let globals_non_volatile = GLOBAL.lock().unwrap().snapshot_non_volatile();

        let vm = match game_data.save_manager.take_pending_vm_snapshot() {
            Some(s) => s,
            None => {
                // If this happens, the save will still have header+thumbnail, but cannot resume execution.
                log::warn!("SaveStateSnapshotV2: missing VM snapshot; save will not be resumable");
                ThreadManager::new().capture_snapshot_v1()
            }
        };

        SaveStateSnapshotV2 {
            version: 2,
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
        // Restore graphics/audio subsystems first.
        SaveStateSnapshotV1 {
            version: 1,
            motion: self.motion.clone(),
            audio: self.audio.clone(),
        }
        .apply(game_data)
        .context("apply motion/audio")?;

        // Restore non-volatile globals.
        {
            let mut g = GLOBAL.lock().unwrap();
            g.restore_non_volatile(&self.globals_non_volatile);
        }

        // Restore VM coroutine state.
        tm.apply_snapshot_v1(&self.vm);

        Ok(())
    }
}

pub fn append_state_chunk_v1(out: &mut Vec<u8>, snap: &SaveStateSnapshotV1) -> Result<()> {
    let payload = bincode_opts()
        .serialize(snap)
        .context("serialize SaveStateSnapshotV1")?;

    if payload.len() > MAX_STATE_PAYLOAD_BYTES {
        bail!(
            "SaveStateSnapshotV1 payload too large: {} bytes (max {})",
            payload.len(),
            MAX_STATE_PAYLOAD_BYTES
        );
    }

    let len_u32: u32 = payload.len().try_into().context("payload length overflow")?;
    out.extend_from_slice(&payload);
    out.extend_from_slice(&len_u32.to_le_bytes());
    out.extend_from_slice(&SAVE_STATE_MAGIC);
    Ok(())
}

pub fn append_state_chunk_v2(out: &mut Vec<u8>, snap: &SaveStateSnapshotV2) -> Result<()> {
    let payload = bincode_opts()
        .serialize(snap)
        .context("serialize SaveStateSnapshotV2")?;

    if payload.len() > MAX_STATE_PAYLOAD_BYTES {
        bail!(
            "SaveStateSnapshotV2 payload too large: {} bytes (max {})",
            payload.len(),
            MAX_STATE_PAYLOAD_BYTES
        );
    }

    let len_u32: u32 = payload.len().try_into().context("payload length overflow")?;
    out.extend_from_slice(&payload);
    out.extend_from_slice(&len_u32.to_le_bytes());
    out.extend_from_slice(&SAVE_STATE_MAGIC);
    Ok(())
}

#[derive(Debug, Clone)]
pub enum DecodedSaveState {
    V2(SaveStateSnapshotV2),
    V1(SaveStateSnapshotV1),
}

/// Decode the optional RFVS chunk from a save slot file.
///
/// This function prefers V2 (newer) and falls back to V1.
pub fn try_decode_state_chunk(file_bytes: &[u8]) -> Result<Option<DecodedSaveState>> {
    if file_bytes.len() < SAVE_STATE_FOOTER_LEN {
        return Ok(None);
    }

    let magic_pos = file_bytes.len() - 4;
    if file_bytes[magic_pos..] != SAVE_STATE_MAGIC {
        return Ok(None);
    }

    let len_pos = file_bytes.len() - SAVE_STATE_FOOTER_LEN;
    let payload_len = u32::from_le_bytes([
        file_bytes[len_pos],
        file_bytes[len_pos + 1],
        file_bytes[len_pos + 2],
        file_bytes[len_pos + 3],
    ]) as usize;

    if payload_len > MAX_STATE_PAYLOAD_BYTES {
        bail!(
            "SaveState footer present but payload length {} exceeds max {}",
            payload_len,
            MAX_STATE_PAYLOAD_BYTES
        );
    }

    if file_bytes.len() < SAVE_STATE_FOOTER_LEN + payload_len {
        bail!(
            "SaveState footer present but payload is truncated: file={} need={} payload_len={} footer={}",
            file_bytes.len(),
            SAVE_STATE_FOOTER_LEN + payload_len,
            payload_len,
            SAVE_STATE_FOOTER_LEN
        );
    }

    let payload_start = file_bytes.len() - SAVE_STATE_FOOTER_LEN - payload_len;
    let payload_end = payload_start + payload_len;
    let payload = &file_bytes[payload_start..payload_end];

    if let Ok(snap2) = bincode_opts().deserialize::<SaveStateSnapshotV2>(payload) {
        if snap2.version != 2 {
            bail!("unsupported SaveStateSnapshotV2 version: {}", snap2.version);
        }
        return Ok(Some(DecodedSaveState::V2(snap2)));
    }

    let snap1: SaveStateSnapshotV1 = bincode_opts()
        .deserialize(payload)
        .context("deserialize SaveStateSnapshotV1")?;
    if snap1.version != 1 {
        bail!("unsupported SaveStateSnapshotV1 version: {}", snap1.version);
    }
    Ok(Some(DecodedSaveState::V1(snap1)))
}

// Compatibility shim: older call sites expect V1. V2 is downcast to motion/audio only.
pub fn try_decode_state_chunk_v1(file_bytes: &[u8]) -> Result<Option<SaveStateSnapshotV1>> {
    match try_decode_state_chunk(file_bytes)? {
        Some(DecodedSaveState::V1(v1)) => Ok(Some(v1)),
        Some(DecodedSaveState::V2(v2)) => Ok(Some(SaveStateSnapshotV1 {
            version: 1,
            motion: v2.motion,
            audio: v2.audio,
        })),
        None => Ok(None),
    }
}
