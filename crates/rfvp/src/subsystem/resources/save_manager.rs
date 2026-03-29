use anyhow::{bail, Result};
use chrono::{Datelike, Local, Timelike};
use std::mem::size_of;
use std::{io::Read, path::Path};

use crate::{script::parser::Nls, utils::file::app_base_path};
use crate::subsystem::resources::thread_manager::ThreadManagerSnapshotV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDataFunction {
    RefreshAll = 0,
    TestSaveData = 1,
    DeleteSaveData = 2,
    CopySaveData = 3,
    GetSaveTitle = 4,
    GetSaveSceneTitle = 5,
    GetScriptContent = 6,
    GetYear = 7,
    GetMonth = 8,
    GetDay = 9,
    GetDayOfWeek = 10,
    GetHour = 11,
    GetMinute = 12,
    LoadSaveThumbToTexture = 13,
}

impl TryFrom<i32> for SaveDataFunction {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> anyhow::Result<Self> {
        match value {
            0 => Ok(SaveDataFunction::RefreshAll),
            1 => Ok(SaveDataFunction::TestSaveData),
            2 => Ok(SaveDataFunction::DeleteSaveData),
            3 => Ok(SaveDataFunction::CopySaveData),
            4 => Ok(SaveDataFunction::GetSaveTitle),
            5 => Ok(SaveDataFunction::GetSaveSceneTitle),
            6 => Ok(SaveDataFunction::GetScriptContent),
            7 => Ok(SaveDataFunction::GetYear),
            8 => Ok(SaveDataFunction::GetMonth),
            9 => Ok(SaveDataFunction::GetDay),
            10 => Ok(SaveDataFunction::GetDayOfWeek),
            11 => Ok(SaveDataFunction::GetHour),
            12 => Ok(SaveDataFunction::GetMinute),
            13 => Ok(SaveDataFunction::LoadSaveThumbToTexture),
            _ => bail!("invalid save data function: {}", value),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SaveItem {
    year: u16,
    month: u8,
    day: u8,
    day_of_week: u8,
    hour: u8,
    minute: u8,
    title: String,
    scene_title: String,
    script_content: String,
}

impl SaveItem {
    pub fn get_save_path(slot: u32) -> std::path::PathBuf {
    // rfvp save files use "rfvp_s%03d.bin"; read path remains compatible with original/legacy names.
    app_base_path()
        .get_path()
        .join("save")
        .join(format!("rfvp_s{:03}.bin", slot))
}

fn legacy_save_path(slot: u32) -> std::path::PathBuf {
    // Legacy rfvp builds used "s{slot}.bin".
    app_base_path()
        .get_path()
        .join("save")
        .join(format!("s{}.bin", slot))
}

pub fn resolve_save_path_for_read(slot: u32) -> std::path::PathBuf {
    let p = Self::get_save_path(slot);
    if p.exists() {
        return p;
    }
    let legacy_padded = app_base_path()
        .get_path()
        .join("save")
        .join(format!("s{:03}.bin", slot));
    if legacy_padded.exists() {
        return legacy_padded;
    }
    Self::legacy_save_path(slot)
}

    fn decode_bytes(nls: Nls, bytes: &[u8]) -> String {
        match nls {
            Nls::ShiftJIS => encoding_rs::SHIFT_JIS.decode(bytes).0.to_string(),
            Nls::GBK => encoding_rs::GBK.decode(bytes).0.to_string(),
            Nls::UTF8 => encoding_rs::UTF_8.decode(bytes).0.to_string(),
        }
    }

    pub fn load_from_file(path: impl AsRef<std::path::Path>, nls: Nls) -> anyhow::Result<Self> {
        use std::io::Read;

        fn read_u16_le<R: Read>(r: &mut R) -> anyhow::Result<u16> {
            let mut b = [0u8; 2];
            r.read_exact(&mut b)?;
            Ok(u16::from_le_bytes(b))
        }

        let file = std::fs::File::open(path)?;
        let mut r = std::io::BufReader::new(file);

        let year = read_u16_le(&mut r)?;
        let mut b = [0u8; 5];
        r.read_exact(&mut b)?;
        let month = b[0];
        let day = b[1];
        let day_of_week = b[2];
        let hour = b[3];
        let minute = b[4];

        let title_len = read_u16_le(&mut r)? as usize;
        let title = if title_len > 0 {
            let mut buf = vec![0u8; title_len];
            r.read_exact(&mut buf)?;
            Self::decode_bytes(nls.clone(), &buf)
        } else {
            String::new()
        };

        let scene_len = read_u16_le(&mut r)? as usize;
        let scene_title = if scene_len > 0 {
            let mut buf = vec![0u8; scene_len];
            r.read_exact(&mut buf)?;
            Self::decode_bytes(nls.clone(), &buf)
        } else {
            String::new()
        };

        let script_len = read_u16_le(&mut r)? as usize;
        let script_content = if script_len > 0 {
            let mut buf = vec![0u8; script_len];
            r.read_exact(&mut buf)?;
            Self::decode_bytes(nls, &buf)
        } else {
            String::new()
        };

        Ok(SaveItem {
            year,
            month,
            day,
            day_of_week,
            hour,
            minute,
            title,
            scene_title,
            script_content,
        })
    }

    pub fn load_from_mem(buf: &[u8], nls: Nls) -> anyhow::Result<Self> {
        fn take<'a>(buf: &'a [u8], cur: &mut usize, n: usize) -> anyhow::Result<&'a [u8]> {
            let end = cur.checked_add(n).ok_or_else(|| anyhow::anyhow!("cursor overflow"))?;
            if end > buf.len() {
                return Err(anyhow::anyhow!("invalid save data: too short"));
            }
            let out = &buf[*cur..end];
            *cur = end;
            Ok(out)
        }

        let mut cur = 0;
        let year = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap());
        let month = take(buf, &mut cur, 1)?[0];
        let day = take(buf, &mut cur, 1)?[0];
        let day_of_week = take(buf, &mut cur, 1)?[0];
        let hour = take(buf, &mut cur, 1)?[0];
        let minute = take(buf, &mut cur, 1)?[0];

        let title_len = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap()) as usize;
        let title = if title_len > 0 {
            let bytes = take(buf, &mut cur, title_len)?;
            Self::decode_bytes(nls.clone(), bytes)
        } else {
            String::new()
        };

        let scene_len = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap()) as usize;
        let scene_title = if scene_len > 0 {
            let bytes = take(buf, &mut cur, scene_len)?;
            Self::decode_bytes(nls.clone(), bytes)
        } else {
            String::new()
        };

        let script_len = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap()) as usize;
        let script_content = if script_len > 0 {
            let bytes = take(buf, &mut cur, script_len)?;
            Self::decode_bytes(nls, bytes)
        } else {
            String::new()
        };

        Ok(SaveItem {
            year,
            month,
            day,
            day_of_week,
            hour,
            minute,
            title,
            scene_title,
            script_content,
        })
    }

    pub fn read_thumb_texture_from_file(
        slot: u32,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Vec<u8>> {
        use std::io::Read;

        fn read_u16_le<R: Read>(r: &mut R) -> anyhow::Result<u16> {
            let mut b = [0u8; 2];
            r.read_exact(&mut b)?;
            Ok(u16::from_le_bytes(b))
        }

        fn skip_bytes<R: Read>(r: &mut R, mut n: usize) -> anyhow::Result<()> {
            let mut buf = [0u8; 4096];
            while n > 0 {
                let take = n.min(buf.len());
                r.read_exact(&mut buf[..take])?;
                n -= take;
            }
            Ok(())
        }

        let path = Self::resolve_save_path_for_read(slot);
        let file = std::fs::File::open(path)?;
        let mut r = std::io::BufReader::new(file);

        // year(u16) + month/day/dow/hour/min (u8 x5)
        let _year = read_u16_le(&mut r)?;
        let mut tmp = [0u8; 5];
        r.read_exact(&mut tmp)?;

        // skip title/scene/script
        let title_len = read_u16_le(&mut r)? as usize;
        if title_len != 0 { skip_bytes(&mut r, title_len)?; }
        let scene_len = read_u16_le(&mut r)? as usize;
        if scene_len != 0 { skip_bytes(&mut r, scene_len)?; }
        let script_len = read_u16_le(&mut r)? as usize;
        if script_len != 0 { skip_bytes(&mut r, script_len)?; }

        let thumb_size = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        let mut thumb = vec![0u8; thumb_size];
        r.read_exact(&mut thumb)?;
        Ok(thumb)
    }

    pub fn read_thumb_texture_from_mem(
        buf: &[u8],
        width: u32,
        height: u32,
    ) -> anyhow::Result<Vec<u8>> {
        fn take<'a>(buf: &'a [u8], cur: &mut usize, n: usize) -> anyhow::Result<&'a [u8]> {
            let end = cur.checked_add(n).ok_or_else(|| anyhow::anyhow!("cursor overflow"))?;
            if end > buf.len() {
                return Err(anyhow::anyhow!("invalid save data: too short"));
            }
            let out = &buf[*cur..end];
            *cur = end;
            Ok(out)
        }

        let mut cur = 0;
        take(buf, &mut cur, 2)?; // year
        take(buf, &mut cur, 5)?; // month/day/dow/hour/min

        let title_len = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap()) as usize;
        take(buf, &mut cur, title_len)?;
        let scene_len = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap()) as usize;
        take(buf, &mut cur, scene_len)?;
        let script_len = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap()) as usize;
        take(buf, &mut cur, script_len)?;

        let thumb_size = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        let bytes = take(buf, &mut cur, thumb_size)?;
        Ok(bytes.to_vec())
    }

    pub fn read_thumb_texture(slot: u32, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
        Self::read_thumb_texture_from_file(slot, width, height)
    }
}

#[derive(Debug)]
pub struct SaveManager {
    thumb_width: u32,
    thumb_height: u32,
    current_scene_title: String,
    current_title: String,
    current_script_content: String,
    current_save_slot: u32,
    load_slot: u32,
    save_requested: bool,
    savedata_prepared: bool,
    should_load: bool,
    slots: Vec<Option<SaveItem>>,
    pending_vm_snapshot: Option<ThreadManagerSnapshotV1>,
    /// Request to prepare an in-memory save payload (original engine: local_saved).
    need_save_prepare: bool,
    /// Prepared save bytes (header + thumbnail + optional RFVS chunk) for SaveWrite.
    local_saved_bytes: Option<Box<[u8]>>,
}

impl Default for SaveManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveManager {
    pub fn new() -> Self {
        SaveManager {
            // Original engine default: 96x72 (can be overridden by SaveThumbSize).
            thumb_width: 96,
            thumb_height: 72,
            current_scene_title: String::new(),
            current_title: String::new(),
            current_script_content: String::new(),
            current_save_slot: u32::MAX,
            load_slot: u32::MAX,
            save_requested: false,
            savedata_prepared: false,
            should_load: false,
            slots: vec![None; 1000],
            pending_vm_snapshot: None,
            need_save_prepare: false,
            local_saved_bytes: None,
        }
    }

    pub fn set_thumb_size(&mut self, width: u32, height: u32) {
        self.thumb_width = width;
        self.thumb_height = height;
    }

    pub fn set_current_scene_title(&mut self, title: String) {
        self.current_scene_title = title;
    }

    pub fn set_current_title(&mut self, title: String) {
        self.current_title = title;
    }

    pub fn set_current_script_content(&mut self, content: String) {
        self.current_script_content = content;
    }

    pub fn set_current_save_slot(&mut self, slot: u32) {
        self.current_save_slot = slot;
    }

    pub fn set_savedata_requested(&mut self, requested: bool) {
        self.save_requested = requested;
    }

    pub fn set_savedata_prepared(&mut self, prepared: bool) {
        self.savedata_prepared = prepared;
    }

    pub fn set_should_load(&mut self, should_load: bool) {
        self.should_load = should_load;
    }

    pub fn get_thumb_width(&self) -> u32 {
        self.thumb_width
    }

    pub fn get_thumb_height(&self) -> u32 {
        self.thumb_height
    }

    pub fn get_current_scene_title(&self) -> &str {
        &self.current_scene_title
    }

    pub fn get_current_title(&self) -> &str {
        &self.current_title
    }

    pub fn get_current_script_content(&self) -> &str {
        &self.current_script_content
    }

    pub fn get_current_save_slot(&self) -> u32 {
        self.current_save_slot
    }

    pub fn is_save_requested(&self) -> bool {
        self.save_requested
    }

    pub fn is_savedata_prepared(&self) -> bool {
        self.savedata_prepared
    }

    pub fn is_should_load(&self) -> bool {
        self.should_load
    }


/// True if the VM runner should capture a coroutine snapshot for an upcoming save.
///
/// This includes both:
/// - regular `SaveWrite(slot)` path (when no local_saved is prepared), and
/// - `SaveCreate(3, nil/int)` local_saved preparation.
pub fn wants_vm_snapshot_capture(&self) -> bool {
    if self.need_save_prepare && self.local_saved_bytes.is_none() {
        return true;
    }
    if self.save_requested && !self.savedata_prepared && self.local_saved_bytes.is_none() {
        return true;
    }
    false
}



pub fn set_pending_vm_snapshot(&mut self, snap: ThreadManagerSnapshotV1) {
    self.pending_vm_snapshot = Some(snap);
}

pub fn has_pending_vm_snapshot(&self) -> bool {
    self.pending_vm_snapshot.is_some()
}

pub fn take_pending_vm_snapshot(&mut self) -> Option<ThreadManagerSnapshotV1> {
    self.pending_vm_snapshot.take()
}



    pub fn asynchronously_save(&mut self, slot: u32) {
        // mark the save status as dirty and perform the 'delayed' save
        self.current_save_slot = slot;
        self.save_requested = true;
    }

    pub fn test_save_slot(&self, slot: u32) -> bool {
        if slot >= 1000 {
            return false;
        }
        if let Some(Some(_)) = self.slots.get(slot as usize) {
            return true;
        }
        SaveItem::get_save_path(slot).exists()
    }

    pub fn get_save_title(&self, slot: u32) -> String {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.title.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    pub fn get_save_scene_title(&self, slot: u32) -> String {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.scene_title.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    pub fn get_script_content(&self, slot: u32) -> String {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.script_content.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    pub fn get_year(&self, slot: u32) -> u16 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.year
            } else {
                u16::MAX
            }
        } else {
            u16::MAX
        }
    }

    pub fn get_month(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.month
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_day(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.day
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_day_of_week(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.day_of_week
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_hour(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.hour
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_minute(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.minute
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_save_thumb(&self, slot: u32, width: u32, height: u32) -> Result<Vec<u8>> {
        SaveItem::read_thumb_texture(slot, width, height)
    }

    pub fn delete_savedata(&mut self, slot: u32) {
        if slot >= 1000 {
            return;
        }
        self.slots[slot as usize] = None;
        let path = SaveItem::resolve_save_path_for_read(slot);
        let _ = std::fs::remove_file(&path);
    }

    pub fn copy_savedata(&mut self, src: u32, dst: u32) -> Result<()> {
        if let Some(save_item) = self.slots.get(src as usize) {
            if let Some(save_item) = save_item {
                self.slots[dst as usize] = Some(save_item.clone());
                                let src_data = SaveItem::resolve_save_path_for_read(src);

                let dst_data = SaveItem::get_save_path(dst);

                let _ = std::fs::copy(src_data, dst_data)?;
            }
        }
        Ok(())
    }

    pub fn refresh_all_savedata(&mut self, nls: Nls) -> Result<()> {
        // Clear the slot table then scan existing save files.
        for slot in self.slots.iter_mut() {
            *slot = None;
        }

        let dir = app_base_path().get_path().join("save");
        if !dir.exists() {
            return Ok(());
        }

        let it = match std::fs::read_dir(&dir) {
            Ok(it) => it,
            Err(e) => {
                log::warn!("refresh_all_savedata: read_dir failed: {e:?}");
                return Ok(());
            }
        };

        for ent in it {
            let ent = match ent {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("refresh_all_savedata: read_dir entry error: {e:?}");
                    continue;
                }
            };
            let path = ent.path();
            if !path.is_file() {
                continue;
            }
            let fname = match path.file_name().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            if !fname.starts_with('s') || !fname.ends_with(".bin") {
                continue;
            }
            let num = &fname[1..fname.len().saturating_sub(4)];
            let slot_id: u32 = match num.parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if slot_id >= 1000 {
                continue;
            }

            match SaveItem::load_from_file(&path, nls.clone()) {
                Ok(item) => self.slots[slot_id as usize] = Some(item),
                Err(e) => log::warn!(
                    "refresh_all_savedata: failed to load slot {} from {}: {e:?}",
                    slot_id,
                    path.display()
                ),
            }
        }

        Ok(())
    }

    pub fn load_savedata(&mut self, slot: u32, nls: Nls) -> Result<()> {
                let path = SaveItem::resolve_save_path_for_read(slot);

        // Missing save slots are not fatal for callers that enumerate all slots.
        if !path.exists() {
            if slot < 1000 {
                self.slots[slot as usize] = None;
            }
            return Ok(());
        }

        let save_item = SaveItem::load_from_file(path, nls)?;
        if slot < 1000 {
            self.slots[slot as usize] = Some(save_item);
        }
        Ok(())
    }

    pub fn load_save_buff(&mut self, slot: u32, nls: Nls, cache: &Vec<u8>) -> Result<()> {
        let save_item = SaveItem::load_from_mem(cache, nls)?;
        self.slots[slot as usize] = Some(save_item);
        Ok(())
    }


    /// If either a `SaveCreate(3, nil/int)` requested preparation of an in-memory payload
    /// (original engine: `local_saved`), or a `SaveWrite` requested a fallback capture,
    /// return (slot, thumb_w, thumb_h) so the renderer can read back the current frame.
    ///
    /// Special slot value `u32::MAX` indicates "prepare local_saved" (no file write).
    pub fn pending_save_capture(&self) -> Option<(u32, u32, u32)> {
        // Prepare local_saved (SaveCreate fnid=3) without binding to any slot.
        if self.need_save_prepare && self.local_saved_bytes.is_none() {
            let w = self.thumb_width.max(1);
            let h = self.thumb_height.max(1);
            return Some((u32::MAX, w, h));
        }

        // If we already have a prepared local_saved buffer, SaveWrite can commit without a GPU readback.
        if self.save_requested && self.local_saved_bytes.is_some() {
            return None;
        }

        if !self.save_requested {
            return None;
        }
        if self.savedata_prepared {
            return None;
        }
        if self.current_save_slot >= 1000 {
            return None;
        }
        let w = self.thumb_width.max(1);
        let h = self.thumb_height.max(1);
        Some((self.current_save_slot, w, h))
    }

        
    /// Request preparing an in-memory save payload (original engine: `local_saved`).
    ///
    /// This should be triggered when the save/load UI is opened (scripts call `SaveCreate(3, nil)`),
    /// so subsequent `SaveWrite(slot)` can commit without capturing the menu overlay.
    pub fn request_prepare_local_savedata(&mut self) {
        self.need_save_prepare = true;
        self.local_saved_bytes = None;
    }

    /// True if an in-memory save payload has been prepared (local_saved).
    pub fn has_local_saved(&self) -> bool {
        self.local_saved_bytes.is_some()
    }

    /// Finalize preparation of `local_saved_bytes` from the captured thumbnail and optional RFVS state.
    pub fn finalize_local_savedata_prepare(
        &mut self,
        nls: Nls,
        thumb_w: u32,
        thumb_h: u32,
        thumb_rgba: &[u8],
        state: Option<&crate::subsystem::save_state::SaveStateSnapshotV1>,
    ) -> Result<()> {
        let expected = (thumb_w as usize)
            .saturating_mul(thumb_h as usize)
            .saturating_mul(4);
        if thumb_rgba.len() != expected {
            log::warn!(
                "finalize_local_savedata_prepare: thumbnail size mismatch: got={}, expected={}",
                thumb_rgba.len(),
                expected
            );
        }

        let mut bytes = self.build_save_file_bytes(nls, thumb_rgba)?;
        if let Some(snap) = state {
            crate::subsystem::save_state::append_state_chunk_v1(&mut bytes, snap)?;
        }

        self.local_saved_bytes = Some(bytes.into_boxed_slice());
        self.need_save_prepare = false;
        Ok(())
    }

    /// If `SaveWrite(slot)` is pending and `local_saved_bytes` is ready, write it to disk.
    ///
    /// Returns `true` if a file was written.
    pub fn try_commit_local_savedata(&mut self, nls: Nls) -> Result<bool> {
        if !self.save_requested || self.savedata_prepared {
            return Ok(false);
        }
        if self.current_save_slot >= 1000 {
            return Ok(false);
        }
        let Some(bytes) = self.local_saved_bytes.as_ref() else {
            return Ok(false);
        };

        let path = SaveItem::get_save_path(self.current_save_slot);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes)?;

        // Refresh slot cache.
        if self.current_save_slot < 1000 {
            if let Ok(item) = SaveItem::load_from_mem(bytes, nls) {
                self.slots[self.current_save_slot as usize] = Some(item);
            }
        }

        self.savedata_prepared = true;
        Ok(true)
    }

    /// Finalize a pending save by writing out the save slot file.
    ///
    /// The caller is expected to provide an RGBA8 thumbnail of size (thumb_w, thumb_h).
    pub fn finalize_save_write(
        &mut self,
        nls: Nls,
        thumb_w: u32,
        thumb_h: u32,
        thumb_rgba: &[u8],
        state: Option<&crate::subsystem::save_state::SaveStateSnapshotV1>,
    ) -> Result<()> {
        if !self.save_requested || self.current_save_slot == u32::MAX {
            return Ok(());
        }
        let expected = (thumb_w as usize)
            .saturating_mul(thumb_h as usize)
            .saturating_mul(4);
        if thumb_rgba.len() != expected {
            log::warn!(
                "finalize_save_write: thumbnail size mismatch: got={}, expected={}",
                thumb_rgba.len(),
                expected
            );
        }

        let mut bytes = self.build_save_file_bytes(nls, thumb_rgba)?;
        if let Some(snap) = state {
            crate::subsystem::save_state::append_state_chunk_v1(&mut bytes, snap)?;
        }
        let path = SaveItem::get_save_path(self.current_save_slot);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &bytes)?;

        // Refresh slot cache.
        if self.current_save_slot < 1000 {
            if let Ok(item) = SaveItem::load_from_mem(&bytes, nls) {
                self.slots[self.current_save_slot as usize] = Some(item);
            }
        }

        self.savedata_prepared = true;
        Ok(())
    }

    /// Called by the SaveWrite syscall once it has observed completion.
    pub fn consume_save_write_result(&mut self) {
        self.save_requested = false;
        self.savedata_prepared = false;
        self.current_save_slot = u32::MAX;    }

    /// Request a load of a save slot (deferred to the engine loop).
    pub fn request_load(&mut self, slot: u32) {
        self.should_load = true;
        self.load_slot = slot;
    }

    /// Take a pending load request (slot), clearing the flag.
    pub fn take_load_request(&mut self) -> Option<u32> {
        if !self.should_load {
            return None;
        }
        self.should_load = false;        Some(self.load_slot)
    }

    /// Load a save slot into the current save fields.
    ///
    /// Note: the actual VM/state restoration is expected to be driven by scripts
    /// using the loaded fields (title/scene/script content). This mirrors the original
    /// engine behavior where the save payload is interpreted at a higher layer.
    pub fn load_slot_into_current(&mut self, slot: u32, nls: Nls) -> Result<()> {
        let path = SaveItem::resolve_save_path_for_read(slot);
        let bytes = std::fs::read(&path)?;
        self.load_slot_into_current_from_bytes(slot, nls, &bytes)
    }

    pub fn load_slot_into_current_from_bytes(&mut self, slot: u32, nls: Nls, bytes: &[u8]) -> Result<()> {
        let item = SaveItem::load_from_mem(bytes, nls)?;
        self.current_save_slot = slot;
        self.current_title = item.title.clone();
        self.current_scene_title = item.scene_title.clone();
        self.current_script_content = item.script_content.clone();
        if slot < 1000 {
            self.slots[slot as usize] = Some(item);
        }
        Ok(())
    }

    fn build_save_file_bytes(&self, nls: Nls, thumb_rgba: &[u8]) -> Result<Vec<u8>> {
        use chrono::Datelike;
        use chrono::Timelike;
        use encoding_rs::Encoding;

        fn enc(nls: Nls, s: &str) -> Vec<u8> {
            match nls {
                Nls::ShiftJIS => {
                    let (cow, _, _) = encoding_rs::SHIFT_JIS.encode(s);
                    cow.into_owned()
                }
                Nls::UTF8 => s.as_bytes().to_vec(),
                Nls::GBK => {
                    let enc = Encoding::for_label(b"gbk").unwrap_or(encoding_rs::UTF_8);
                    let (cow, _, _) = enc.encode(s);
                    cow.into_owned()
                }
            }
        }

        // -----------------------------
        // Save file prefix format (IDA):
        //   offset 0:  u16 year (LE)
        //   offset 2:  u8  month
        //   offset 3:  u8  day
        //   offset 4:  u8  day_of_week (0..6, Sunday=0)
        //   offset 5:  u8  hour
        //   offset 6:  u8  minute
        //   offset 7:  u16 title_len (LE) + title bytes
        //              u16 scene_len (LE) + scene bytes
        //              u16 script_len (LE) + script bytes
        //              RGBA8 thumbnail: 4 * thumb_w * thumb_h bytes
        // -----------------------------
        let now = chrono::Local::now();
        let year: u16 = (now.year().clamp(0, 65535)) as u16;
        let month: u8 = now.month().clamp(0, 255) as u8;
        let day: u8 = now.day().clamp(0, 255) as u8;
        let day_of_week: u8 = now.weekday().num_days_from_sunday() as u8;
        let hour: u8 = now.hour().clamp(0, 255) as u8;
        let minute: u8 = now.minute().clamp(0, 255) as u8;

        let title_b = enc(nls.clone(), &self.current_title);
        let scene_b = enc(nls.clone(), &self.current_scene_title);
        let script_b = enc(nls, &self.current_script_content);

        let title_len: u16 = title_b.len().min(u16::MAX as usize) as u16;
        let scene_len: u16 = scene_b.len().min(u16::MAX as usize) as u16;
        let script_len: u16 = script_b.len().min(u16::MAX as usize) as u16;

        let mut out: Vec<u8> = Vec::with_capacity(
            7
                + 2 + (title_len as usize)
                + 2 + (scene_len as usize)
                + 2 + (script_len as usize)
                + thumb_rgba.len(),
        );

        out.extend_from_slice(&year.to_le_bytes());
        out.push(month);
        out.push(day);
        out.push(day_of_week);
        out.push(hour);
        out.push(minute);

        out.extend_from_slice(&title_len.to_le_bytes());
        out.extend_from_slice(&title_b[..(title_len as usize)]);

        out.extend_from_slice(&scene_len.to_le_bytes());
        out.extend_from_slice(&scene_b[..(scene_len as usize)]);

        out.extend_from_slice(&script_len.to_le_bytes());
        out.extend_from_slice(&script_b[..(script_len as usize)]);

        out.extend_from_slice(thumb_rgba);

        Ok(out)
    }

    pub fn finish_save_write_from_thumb(&mut self, slot: u32, nls: Nls, thumb_rgba: &[u8]) -> Result<()> {
        // Deprecated entrypoint kept for compatibility with older call sites.
        // Prefer `finalize_save_write()` which validates state and refreshes caches.
        self.current_save_slot = slot;
        self.save_requested = true;
        self.savedata_prepared = false;
        self.finalize_save_write(nls, self.thumb_width.max(1), self.thumb_height.max(1), thumb_rgba, None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_save_item() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/s282.bin"));

        let save_item = SaveItem::load_from_file(filepath, Nls::ShiftJIS).unwrap();
        log::debug!("{:?}", save_item);
    }
}