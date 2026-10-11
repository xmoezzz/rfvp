#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::script::global::GLOBAL;
use crate::script::Variant;
use crate::subsystem::resources::flag_manager::FlagManager;
use crate::subsystem::snapshot_codec;
use crate::subsystem::world::GameData;

const MAX_GLOBAL_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSaveDataV1 {
    pub version: u16,

    // HCB first-region count. The original engine uses this as the base
    // offset of the global/system region; normal save slots serialize the
    // first region, while the global save serializes the second region.
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
        let (non_volatile_global_count, volatile_global_count, volatile_globals) = {
            let g = GLOBAL.lock().unwrap();
            (
                g.non_volatile_count(),
                g.volatile_count(),
                g.snapshot_volatile_globals(),
            )
        };

        GlobalSaveDataV1 {
            version: 1,
            non_volatile_global_count,
            volatile_global_count,
            volatile_globals,
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
            // The PlayStation's map grows as text is read (see TextManager::new).
            if self.readed_text.len() == expected || cfg!(feature = "psx") {
                game_data.motion_manager.text_manager.readed_text = self.readed_text.clone();
            } else {
                log::warn!(
                    "GlobalSaveDataV1: readed_text length mismatch: got={}, expected={} (ignoring)",
                    self.readed_text.len(),
                    expected
                );
            }
        }

        {
            let mut g = GLOBAL.lock().unwrap();
            g.restore_volatile_globals(
                self.non_volatile_global_count,
                self.volatile_global_count,
                &self.volatile_globals,
            );
        }

        // Non-critical engine state.
        game_data
            .save_manager
            .set_thumb_size(self.thumb_width, self.thumb_height);
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

        // The first HCB global region is deliberately not restored here.
        // The original engine restores that region from normal save slots.
    }
}

/// Where the global save is kept, relative to the game's directory.
pub const GLOBAL_SAVE_STORAGE_PATH: &str = "save/rfvp_global.bin";

/// The global save as it is written to disk: the encoded state followed by its footer.
pub fn encode_global_savedata_v1(game_data: &GameData) -> Result<Vec<u8>> {
    let snap = GlobalSaveDataV1::capture(game_data);
    let payload = snapshot_codec::encode(&snap).context("serialize GlobalSaveDataV1")?;
    let mut out = Vec::with_capacity(payload.len() + snapshot_codec::FOOTER_LEN);
    snapshot_codec::append_chunk(
        &mut out,
        &payload,
        snapshot_codec::GLOBAL_MAGIC,
        MAX_GLOBAL_PAYLOAD_BYTES,
    )
    .context("GlobalSaveDataV1")?;
    Ok(out)
}

pub fn save_global_savedata_v1(game_data: &GameData) -> Result<()> {
    let Some(storage) = game_data.save_manager.storage() else {
        return Ok(());
    };
    let out = encode_global_savedata_v1(game_data)?;
    storage
        .write(GLOBAL_SAVE_STORAGE_PATH, &out)
        .map_err(|err| anyhow::anyhow!("write {GLOBAL_SAVE_STORAGE_PATH}: {err:?}"))
}

pub fn try_load_global_savedata_v1(game_data: &mut GameData) -> Result<bool> {
    let Some(storage) = game_data.save_manager.storage().cloned() else {
        return Ok(false);
    };
    if !storage.exists(GLOBAL_SAVE_STORAGE_PATH) {
        return Ok(false);
    }
    let bytes = storage
        .read(GLOBAL_SAVE_STORAGE_PATH)
        .map_err(|err| anyhow::anyhow!("read {GLOBAL_SAVE_STORAGE_PATH}: {err:?}"))?;
    apply_global_savedata_bytes(game_data, &bytes)
}

fn apply_global_savedata_bytes(game_data: &mut GameData, bytes: &[u8]) -> Result<bool> {
    let Some(snap) = try_decode_global_savedata_v1(bytes)? else {
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
    let Some(payload) = snapshot_codec::find_chunk(
        file_bytes,
        snapshot_codec::GLOBAL_MAGIC,
        MAX_GLOBAL_PAYLOAD_BYTES,
    )
    .context("GlobalSaveDataV1")?
    else {
        return Ok(None);
    };
    let snap: GlobalSaveDataV1 =
        snapshot_codec::decode(payload).context("deserialize GlobalSaveDataV1")?;
    Ok(Some(snap))
}
