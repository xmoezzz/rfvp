use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use bincode::Options;

use crate::audio_player::{BgmPlayerSnapshotV1, SePlayerSnapshotV1};
use crate::subsystem::resources::motion_manager::MotionManagerSnapshotV1;
use crate::subsystem::world::GameData;

const SAVE_STATE_MAGIC: [u8; 4] = *b"RFVS";
const SAVE_STATE_FOOTER_LEN: usize = 8; // u32 payload_len + 4-byte magic
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

fn bincode_opts() -> impl bincode::Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
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

    let len_u32: u32 = payload
        .len()
        .try_into()
        .context("payload length overflow")?;

    out.extend_from_slice(&payload);
    out.extend_from_slice(&len_u32.to_le_bytes());
    out.extend_from_slice(&SAVE_STATE_MAGIC);

    Ok(())
}

pub fn try_decode_state_chunk_v1(file_bytes: &[u8]) -> Result<Option<SaveStateSnapshotV1>> {
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
            "SaveStateSnapshotV1 payload length {} exceeds max {}",
            payload_len,
            MAX_STATE_PAYLOAD_BYTES
        );
    }

    if file_bytes.len() < SAVE_STATE_FOOTER_LEN + payload_len {
        bail!(
            "SaveStateSnapshotV1 footer present but payload is truncated: file={} need={} payload_len={} footer={}",
            file_bytes.len(),
            SAVE_STATE_FOOTER_LEN + payload_len,
            payload_len,
            SAVE_STATE_FOOTER_LEN
        );
    }

    let payload_start = file_bytes.len() - SAVE_STATE_FOOTER_LEN - payload_len;
    let payload_end = payload_start + payload_len;
    let payload = &file_bytes[payload_start..payload_end];

    let snap: SaveStateSnapshotV1 = bincode_opts()
        .deserialize(payload)
        .context("deserialize SaveStateSnapshotV1")?;

    if snap.version != 1 {
        bail!("unsupported SaveStateSnapshot version: {}", snap.version);
    }

    Ok(Some(snap))
}
