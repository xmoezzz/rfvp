use anyhow::{bail, Context, Result};
use bincode::Options;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::script::global::GLOBAL;
use crate::script::Variant;
use crate::subsystem::resources::flag_manager::FlagManager;
use crate::subsystem::world::GameData;
use crate::utils::file::app_base_path;

const GLOBAL_SAVE_MAGIC: [u8; 4] = *b"RFVG";
const GLOBAL_SAVE_FOOTER_LEN: usize = 8; // u32 payload_len + 4-byte magic
const MAX_GLOBAL_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSaveDataV1 {
    pub version: u16,

    pub non_volatile_global_count: u16,
    pub volatile_global_count: u16,
    pub volatile_globals: Vec<Variant>,

    pub flags: FlagManager,
    pub readed_text: Vec<u32>,

    pub thumb_width: u32,
    pub thumb_height: u32,

    pub current_cursor_index: u32,

    pub render_flag: i32,
    pub is_first_frame: bool,
    pub close_immediate: bool,

    pub system_fontface_id: i32,
    pub current_font_name: String,
}

impl GlobalSaveDataV1 {
    pub fn capture(game_data: &GameData) -> Self {
        let (non_volatile_global_count, _volatile_global_count) = {
            let g = GLOBAL.lock().unwrap();
            (g.non_volatile_count(), g.volatile_count())
        };

        GlobalSaveDataV1 {
            version: 1,
            non_volatile_global_count,
            // Keep the V1 binary layout/fields for compatibility, but do not persist
            // volatile globals. The original global save should not carry ephemeral globals.
            volatile_global_count: 0,
            volatile_globals: Vec::new(),
            flags: game_data.flag_manager.clone(),
            readed_text: game_data.motion_manager.text_manager.readed_text.clone(),
            thumb_width: game_data.save_manager.get_thumb_width(),
            thumb_height: game_data.save_manager.get_thumb_height(),
            current_cursor_index: game_data.get_current_cursor_index(),
            render_flag: game_data.get_render_flag(),
            is_first_frame: game_data.get_is_first_frame(),
            close_immediate: game_data.get_close_immediate(),
            system_fontface_id: game_data.fontface_manager.get_system_fontface_id(),
            current_font_name: game_data
                .fontface_manager
                .get_current_font_name()
                .to_string(),
        }
    }

    pub fn apply(&self, game_data: &mut GameData) {
        // Flags and read-text bitmap are the most important script-visible global state.
        game_data.flag_manager = self.flags.clone();

        if !self.readed_text.is_empty() {
            // Keep the bitmap length compatible. Our TextManager expects exactly 0x800000/32 u32s.
            let expected = 0x800000usize / 32;
            if self.readed_text.len() == expected {
                game_data.motion_manager.text_manager.readed_text = self.readed_text.clone();
            } else {
                log::warn!(
                    "GlobalSaveDataV1: readed_text length mismatch: got={}, expected={} (ignoring)",
                    self.readed_text.len(),
                    expected
                );
            }
        }

        // Non-critical engine state.
        game_data.save_manager.set_thumb_size(self.thumb_width, self.thumb_height);
        game_data.set_current_cursor_index(self.current_cursor_index);
        game_data.set_render_flag_local(self.render_flag);
        game_data.set_is_first_frame(self.is_first_frame);
        game_data.set_close_immediate(self.close_immediate);

        game_data
            .fontface_manager
            .set_system_fontface_id(self.system_fontface_id);
        game_data
            .fontface_manager
            .set_current_font_name(&self.current_font_name);

        // Do not restore volatile globals from global save. Keep the V1 fields only so
        // existing files remain decodable; volatile globals are intentionally treated as
        // non-persistent runtime state.
        let _ = (
            self.non_volatile_global_count,
            self.volatile_global_count,
            &self.volatile_globals,
        );
    }
}

fn bincode_opts() -> impl bincode::Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
}

pub fn global_savedata_path() -> PathBuf {
    app_base_path()
        .get_path()
        .join("save")
        .join("rfvp_global.bin")
}

pub fn save_global_savedata_v1(game_data: &GameData) -> Result<()> {
    let snap = GlobalSaveDataV1::capture(game_data);
    let payload = bincode_opts()
        .serialize(&snap)
        .context("serialize GlobalSaveDataV1")?;

    if payload.len() > MAX_GLOBAL_PAYLOAD_BYTES {
        bail!(
            "GlobalSaveDataV1 payload too large: {} bytes (max {})",
            payload.len(),
            MAX_GLOBAL_PAYLOAD_BYTES
        );
    }

    let len_u32: u32 = payload
        .len()
        .try_into()
        .context("payload length overflow")?;

    let mut out = Vec::with_capacity(payload.len() + GLOBAL_SAVE_FOOTER_LEN);
    out.extend_from_slice(&payload);
    out.extend_from_slice(&len_u32.to_le_bytes());
    out.extend_from_slice(&GLOBAL_SAVE_MAGIC);

    let path = global_savedata_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create_dir_all {}", parent.display()))?;
    }
    std::fs::write(&path, &out).with_context(|| format!("write {}", path.display()))?;

    Ok(())
}

pub fn try_load_global_savedata_v1(game_data: &mut GameData) -> Result<bool> {
    let path = global_savedata_path();
    if !path.exists() {
        return Ok(false);
    }

    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let Some(snap) = try_decode_global_savedata_v1(&bytes)? else {
        return Ok(false);
    };

    if snap.version != 1 {
        log::warn!("GlobalSaveData: unsupported version: {}", snap.version);
        return Ok(false);
    }

    snap.apply(game_data);
    Ok(true)
}

pub fn try_decode_global_savedata_v1(file_bytes: &[u8]) -> Result<Option<GlobalSaveDataV1>> {
    if file_bytes.len() < GLOBAL_SAVE_FOOTER_LEN {
        return Ok(None);
    }

    let magic_pos = file_bytes.len() - 4;
    if file_bytes[magic_pos..] != GLOBAL_SAVE_MAGIC {
        return Ok(None);
    }

    let len_pos = file_bytes.len() - GLOBAL_SAVE_FOOTER_LEN;
    let payload_len = u32::from_le_bytes([
        file_bytes[len_pos],
        file_bytes[len_pos + 1],
        file_bytes[len_pos + 2],
        file_bytes[len_pos + 3],
    ]) as usize;

    if payload_len > MAX_GLOBAL_PAYLOAD_BYTES {
        bail!(
            "GlobalSaveDataV1 payload length {} exceeds max {}",
            payload_len,
            MAX_GLOBAL_PAYLOAD_BYTES
        );
    }

    if file_bytes.len() < GLOBAL_SAVE_FOOTER_LEN + payload_len {
        bail!(
            "GlobalSaveDataV1 footer present but payload is truncated: file={} need={} payload_len={} footer={}",
            file_bytes.len(),
            GLOBAL_SAVE_FOOTER_LEN + payload_len,
            payload_len,
            GLOBAL_SAVE_FOOTER_LEN
        );
    }

    let payload_start = file_bytes.len() - GLOBAL_SAVE_FOOTER_LEN - payload_len;
    let payload_end = payload_start + payload_len;
    let payload = &file_bytes[payload_start..payload_end];

    let snap: GlobalSaveDataV1 = bincode_opts()
        .deserialize(payload)
        .context("deserialize GlobalSaveDataV1")?;

    Ok(Some(snap))
}
