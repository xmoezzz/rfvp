//! Save slots.
//!
//! Slot files are read and written through the host's [`RfvpStorage`] (the game directory on a
//! desktop, an SD card on a console); without one, saves last for the session. The save manager
//! has no clock or GPU readback of its own: the date comes from [`SaveManager::set_wall_time`]
//! and the thumbnail is handed in by the frontend.
//!
//! A slot file is a small header (date, title, scene, script text), the RGBA thumbnail, and the
//! encoded engine state as a chunk at the end.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use anyhow::{anyhow, bail, Result};

use crate::host_api::{RfvpStorage, WallTime};
use crate::script::parser::Nls;
use crate::subsystem::resources::thread_manager::ThreadManagerSnapshotV1;

const SLOT_COUNT: usize = 1000;
/// Directory of the slot files, relative to the game's directory.
const SAVE_DIR: &str = "save";
/// The most a slot header can take: the date, then three strings of up to 64 KiB each.
const HEADER_MAX_LEN: usize = 7 + 6 + 3 * 65535;
/// What is read first for a slot header; headers are short and the full bound is rarely needed.
const HEADER_FIRST_READ: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDataFunction {
    RefreshAll,
    TestSaveData,
    DeleteSaveData,
    CopySaveData,
    GetSaveTitle,
    GetSaveSceneTitle,
    GetScriptContent,
    GetYear,
    GetMonth,
    GetDay,
    GetDayOfWeek,
    GetHour,
    GetMinute,
    LoadSaveThumbToTexture,
}

impl TryFrom<i32> for SaveDataFunction {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self> {
        match value {
            0 => Ok(Self::RefreshAll),
            1 => Ok(Self::TestSaveData),
            2 => Ok(Self::DeleteSaveData),
            3 => Ok(Self::CopySaveData),
            4 => Ok(Self::GetSaveTitle),
            5 => Ok(Self::GetSaveSceneTitle),
            6 => Ok(Self::GetScriptContent),
            7 => Ok(Self::GetYear),
            8 => Ok(Self::GetMonth),
            9 => Ok(Self::GetDay),
            10 => Ok(Self::GetDayOfWeek),
            11 => Ok(Self::GetHour),
            12 => Ok(Self::GetMinute),
            13 => Ok(Self::LoadSaveThumbToTexture),
            _ => Err(anyhow!("unknown SaveData function id: {value}")),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SaveItem {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub day_of_week: u8,
    pub hour: u8,
    pub minute: u8,
    pub title: String,
    pub scene_title: String,
    pub script_content: String,
}

impl SaveItem {
    /// Where slot `slot` is written.
    pub fn get_save_path(slot: u32) -> String {
        format!("{SAVE_DIR}/rfvp_s{slot:03}.bin")
    }

    /// Where older builds kept the slot; read, never written.
    fn legacy_paths(slot: u32) -> [String; 2] {
        [
            format!("{SAVE_DIR}/s{slot:03}.bin"),
            format!("{SAVE_DIR}/s{slot}.bin"),
        ]
    }

    fn decode_bytes(nls: Nls, bytes: &[u8]) -> String {
        crate::text_codec::decode(nls, bytes).0
    }

    /// The header of a slot file. `buf` may be only the head of the file.
    pub fn load_from_mem(buf: &[u8], nls: Nls) -> Result<Self> {
        let mut cur = 0;
        let year = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap());
        let month = take(buf, &mut cur, 1)?[0];
        let day = take(buf, &mut cur, 1)?[0];
        let day_of_week = take(buf, &mut cur, 1)?[0];
        let hour = take(buf, &mut cur, 1)?[0];
        let minute = take(buf, &mut cur, 1)?[0];

        let mut text = |nls: Nls| -> Result<String> {
            let len = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap()) as usize;
            Ok(if len > 0 {
                Self::decode_bytes(nls, take(buf, &mut cur, len)?)
            } else {
                String::new()
            })
        };
        let title = text(nls.clone())?;
        let scene_title = text(nls.clone())?;
        let script_content = text(nls)?;

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

    /// The RGBA thumbnail of a slot file, which follows the header. `buf` may be only the head
    /// of the file.
    pub fn read_thumb_texture_from_mem(buf: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
        let mut cur = 0;
        take(buf, &mut cur, 2 + 5)?; // date
        for _ in 0..3 {
            let len = u16::from_le_bytes(take(buf, &mut cur, 2)?.try_into().unwrap()) as usize;
            take(buf, &mut cur, len)?;
        }
        #[cfg(feature = "psx")]
        {
            let packed = take(buf, &mut cur, psx_thumb::encoded_len(width, height))?;
            return Ok(psx_thumb::decode(packed, width, height));
        }
        #[cfg(not(feature = "psx"))]
        {
            let thumb_size = (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4);
            Ok(take(buf, &mut cur, thumb_size)?.to_vec())
        }
    }
}

fn take<'a>(buf: &'a [u8], cur: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = cur
        .checked_add(n)
        .ok_or_else(|| anyhow!("cursor overflow"))?;
    if end > buf.len() {
        return Err(anyhow!("invalid save data: too short"));
    }
    let out = &buf[*cur..end];
    *cur = end;
    Ok(out)
}

#[derive(Clone)]
struct StorageHandle(Arc<dyn RfvpStorage>);

impl fmt::Debug for StorageHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StorageHandle")
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
    /// Boxed: most of the 1000 slots are empty.
    slots: Vec<Option<Box<SaveItem>>>,
    pending_vm_snapshot: Option<ThreadManagerSnapshotV1>,
    /// Request to prepare an in-memory save payload (original engine: local_saved).
    need_save_prepare: bool,
    /// Prepared save bytes (header + thumbnail + optional state chunk) for SaveWrite.
    local_saved_bytes: Option<Box<[u8]>>,
    storage: Option<StorageHandle>,
    /// Slots found on the last `refresh_all_savedata`.
    present: Vec<bool>,
    /// Slots looked for in the storage and not found since (one bit each): scripts test a slot
    /// every frame, and a look costs path formatting and file searches.
    absent: [core::sync::atomic::AtomicU32; SLOT_COUNT.div_ceil(32)],
    wall_time: Option<WallTime>,
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
            slots: vec![None; SLOT_COUNT],
            pending_vm_snapshot: None,
            need_save_prepare: false,
            local_saved_bytes: None,
            storage: None,
            present: vec![false; SLOT_COUNT],
            absent: [const { core::sync::atomic::AtomicU32::new(0) }; SLOT_COUNT.div_ceil(32)],
            wall_time: None,
        }
    }

    /// Where slot files are kept. Without it saves only last for the session.
    pub fn set_storage(&mut self, storage: Option<Arc<dyn RfvpStorage>>) {
        self.storage = storage.map(StorageHandle);
        self.forget_absent();
    }

    pub fn storage(&self) -> Option<&Arc<dyn RfvpStorage>> {
        self.storage.as_ref().map(|handle| &handle.0)
    }

    /// The date and time stamped on the next save.
    pub fn set_wall_time(&mut self, wall_time: Option<WallTime>) {
        self.wall_time = wall_time;
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

    pub fn wants_vm_snapshot_capture(&self) -> bool {
        (self.need_save_prepare && self.local_saved_bytes.is_none())
            || (self.save_requested && !self.savedata_prepared && self.local_saved_bytes.is_none())
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

    /// The path a slot file exists at, if it does.
    fn existing_path(&self, slot: u32) -> Option<String> {
        let storage = self.storage()?;
        if slot as usize >= SLOT_COUNT {
            return None;
        }
        let primary = SaveItem::get_save_path(slot);
        if storage.exists(&primary) {
            return Some(primary);
        }
        SaveItem::legacy_paths(slot)
            .into_iter()
            .find(|path| storage.exists(path))
    }

    pub fn test_save_slot(&self, slot: u32) -> bool {
        if slot as usize >= SLOT_COUNT {
            return false;
        }
        if self.slots[slot as usize].is_some() || self.present[slot as usize] {
            return true;
        }
        use core::sync::atomic::Ordering::Relaxed;
        let (word, bit) = (slot as usize / 32, 1u32 << (slot % 32));
        if self.absent[word].load(Relaxed) & bit != 0 {
            return false;
        }
        let found = self.existing_path(slot).is_some();
        if !found {
            self.absent[word].fetch_or(bit, Relaxed);
        }
        found
    }

    fn forget_absent(&mut self) {
        for word in self.absent.iter_mut() {
            *word.get_mut() = 0;
        }
    }

    pub fn get_save_title(&self, slot: u32) -> String {
        self.item(slot)
            .map(|item| item.title.clone())
            .unwrap_or_default()
    }

    pub fn get_save_scene_title(&self, slot: u32) -> String {
        self.item(slot)
            .map(|item| item.scene_title.clone())
            .unwrap_or_default()
    }

    pub fn get_script_content(&self, slot: u32) -> String {
        self.item(slot)
            .map(|item| item.script_content.clone())
            .unwrap_or_default()
    }

    pub fn get_year(&self, slot: u32) -> u16 {
        self.item(slot).map_or(u16::MAX, |item| item.year)
    }

    pub fn get_month(&self, slot: u32) -> u8 {
        self.item(slot).map_or(u8::MAX, |item| item.month)
    }

    pub fn get_day(&self, slot: u32) -> u8 {
        self.item(slot).map_or(u8::MAX, |item| item.day)
    }

    pub fn get_day_of_week(&self, slot: u32) -> u8 {
        self.item(slot).map_or(u8::MAX, |item| item.day_of_week)
    }

    pub fn get_hour(&self, slot: u32) -> u8 {
        self.item(slot).map_or(u8::MAX, |item| item.hour)
    }

    pub fn get_minute(&self, slot: u32) -> u8 {
        self.item(slot).map_or(u8::MAX, |item| item.minute)
    }

    fn item(&self, slot: u32) -> Option<&SaveItem> {
        self.slots.get(slot as usize)?.as_deref()
    }

    pub fn get_save_thumb(&self, slot: u32, width: u32, height: u32) -> Result<Vec<u8>> {
        let storage = self
            .storage()
            .ok_or_else(|| anyhow!("no storage for save thumbnails"))?;
        let path = self
            .existing_path(slot)
            .ok_or_else(|| anyhow!("save slot {slot} does not exist"))?;
        let want = HEADER_MAX_LEN
            + (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4);
        let head = storage
            .read_prefix(&path, want)
            .map_err(|err| anyhow!("read {path}: {err:?}"))?;
        SaveItem::read_thumb_texture_from_mem(&head, width, height)
    }

    pub fn delete_savedata(&mut self, slot: u32) {
        if slot as usize >= SLOT_COUNT {
            return;
        }
        self.slots[slot as usize] = None;
        self.present[slot as usize] = false;
        if let (Some(storage), Some(path)) = (self.storage(), self.existing_path(slot)) {
            let _ = storage.remove(&path);
        }
    }

    pub fn copy_savedata(&mut self, src: u32, dst: u32) -> Result<()> {
        if src as usize >= SLOT_COUNT || dst as usize >= SLOT_COUNT {
            bail!("save slot out of range");
        }
        let Some(item) = self.slots[src as usize].clone() else {
            return Ok(());
        };
        if let (Some(storage), Some(from)) = (self.storage(), self.existing_path(src)) {
            let bytes = storage
                .read(&from)
                .map_err(|err| anyhow!("read {from}: {err:?}"))?;
            let to = SaveItem::get_save_path(dst);
            storage
                .write(&to, &bytes)
                .map_err(|err| anyhow!("write {to}: {err:?}"))?;
        }
        self.slots[dst as usize] = Some(item);
        self.present[dst as usize] = true;
        Ok(())
    }

    /// Forget what is cached about the slots, find out which ones exist and read their headers.
    pub fn refresh_all_savedata(&mut self, nls: Nls) -> Result<()> {
        for slot in self.slots.iter_mut() {
            *slot = None;
        }
        for present in self.present.iter_mut() {
            *present = false;
        }
        self.forget_absent();
        let Some(storage) = self.storage() else {
            return Ok(());
        };
        let names = match storage.list(SAVE_DIR) {
            Ok(names) => names,
            // No save directory yet: nothing has been saved.
            Err(_) => return Ok(()),
        };
        let mut found = Vec::new();
        for name in names {
            // `rfvp_s012.bin`, and the older `s012.bin` / `s12.bin`.
            let stem = name.strip_suffix(".bin").unwrap_or("");
            let digits = stem
                .strip_prefix("rfvp_s")
                .or_else(|| stem.strip_prefix('s'));
            if let Some(slot) = digits.and_then(|d| d.parse::<usize>().ok()) {
                if slot < SLOT_COUNT {
                    found.push(slot);
                }
            }
        }
        for slot in found {
            self.present[slot] = true;
            if let Err(err) = self.load_savedata(slot as u32, nls.clone()) {
                log::warn!("refresh_all_savedata: slot {slot}: {err:#}");
            }
        }
        Ok(())
    }

    /// Read the header of slot `slot` into the cache. A missing slot is not an error.
    pub fn load_savedata(&mut self, slot: u32, nls: Nls) -> Result<()> {
        if slot as usize >= SLOT_COUNT {
            return Ok(());
        }
        if self.slots[slot as usize].is_some() {
            return Ok(());
        }
        let Some(path) = self.existing_path(slot) else {
            return Ok(());
        };
        let Some(storage) = self.storage() else {
            return Ok(());
        };
        let read = |len| {
            storage
                .read_prefix(&path, len)
                .map_err(|err| anyhow!("read {path}: {err:?}"))
        };
        let head = read(HEADER_FIRST_READ)?;
        let item = match SaveItem::load_from_mem(&head, nls.clone()) {
            Ok(item) => item,
            // A header with long strings; read as much as one can take.
            Err(_) if head.len() == HEADER_FIRST_READ => {
                SaveItem::load_from_mem(&read(HEADER_MAX_LEN)?, nls)?
            }
            Err(err) => return Err(err),
        };
        self.slots[slot as usize] = Some(Box::new(item));
        Ok(())
    }

    pub fn load_save_buff(&mut self, slot: u32, nls: Nls, cache: &Vec<u8>) -> Result<()> {
        let save_item = SaveItem::load_from_mem(cache, nls)?;
        self.slots[slot as usize] = Some(Box::new(save_item));
        Ok(())
    }

    /// The whole slot file, for loading the saved state.
    pub fn read_slot_file(&self, slot: u32) -> Result<Vec<u8>> {
        let storage = self
            .storage()
            .ok_or_else(|| anyhow!("no storage for saves"))?;
        let path = self
            .existing_path(slot)
            .ok_or_else(|| anyhow!("save slot {slot} does not exist"))?;
        storage
            .read(&path)
            .map_err(|err| anyhow!("read {path}: {err:?}"))
    }

    /// If either a `SaveCreate(3, nil/int)` requested preparation of an in-memory payload
    /// (original engine: `local_saved`), or a `SaveWrite` requested a fallback capture,
    /// return (slot, thumb_w, thumb_h) so the renderer can capture the current frame.
    ///
    /// Special slot value `u32::MAX` indicates "prepare local_saved" (no file write).
    pub fn pending_save_capture(&self) -> Option<(u32, u32, u32)> {
        // Prepare local_saved (SaveCreate fnid=3) without binding to any slot.
        if self.need_save_prepare && self.local_saved_bytes.is_none() {
            let w = self.thumb_width.max(1);
            let h = self.thumb_height.max(1);
            return Some((u32::MAX, w, h));
        }

        // If we already have a prepared local_saved buffer, SaveWrite can commit without a capture.
        if self.save_requested && self.local_saved_bytes.is_some() {
            return None;
        }

        if !self.save_requested {
            return None;
        }
        if self.savedata_prepared {
            return None;
        }
        if self.current_save_slot >= SLOT_COUNT as u32 {
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

    /// Finalize preparation of `local_saved_bytes` from the captured thumbnail and optional state.
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
        if thumb_rgba.len() != expected && !(cfg!(feature = "psx") && thumb_rgba.is_empty()) {
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

    /// The header and thumbnail of a slot file; the PlayStation appends the state itself
    /// (`save_state::append_state_chunk_streamed`) and hands the result to
    /// [`Self::finish_prepare`] or [`Self::finish_write`].
    #[cfg(feature = "psx")]
    pub fn slot_file_head(&self, nls: Nls, thumb_rgba: &[u8]) -> Result<Vec<u8>> {
        self.build_save_file_bytes(nls, thumb_rgba)
    }

    /// `finalize_local_savedata_prepare` with the file already built.
    #[cfg(feature = "psx")]
    pub fn finish_prepare(&mut self, bytes: Vec<u8>) {
        self.local_saved_bytes = Some(bytes.into_boxed_slice());
        self.need_save_prepare = false;
    }

    /// `finalize_save_write` with the file already built.
    #[cfg(feature = "psx")]
    pub fn finish_write(&mut self, bytes: Vec<u8>, nls: Nls) -> Result<()> {
        if !self.save_requested || self.current_save_slot == u32::MAX {
            return Ok(());
        }
        self.write_current_slot(&bytes, nls)?;
        self.savedata_prepared = true;
        Ok(())
    }

    /// Write `bytes` to the file of the current slot and refresh the cache for it.
    fn write_current_slot(&mut self, bytes: &[u8], nls: Nls) -> Result<()> {
        let started = crate::perf::start();
        let slot = self.current_save_slot;
        if let Some(storage) = self.storage() {
            let path = SaveItem::get_save_path(slot);
            storage
                .write(&path, bytes)
                .map_err(|err| anyhow!("write {path}: {err:?}"))?;
        }
        if let Ok(item) = SaveItem::load_from_mem(bytes, nls) {
            self.slots[slot as usize] = Some(Box::new(item));
        }
        self.present[slot as usize] = true;
        crate::perf::event(format_args!("save write slot={} bytes={} total_us={}", slot, bytes.len(),
            crate::perf::start().saturating_sub(started)));
        Ok(())
    }

    /// If `SaveWrite(slot)` is pending and `local_saved_bytes` is ready, write it out.
    ///
    /// Returns `true` if a file was written.
    pub fn try_commit_local_savedata(&mut self, nls: Nls) -> Result<bool> {
        if !self.save_requested || self.savedata_prepared {
            return Ok(false);
        }
        if self.current_save_slot >= SLOT_COUNT as u32 {
            return Ok(false);
        }
        let Some(bytes) = self.local_saved_bytes.take() else {
            return Ok(false);
        };
        let result = self.write_current_slot(&bytes, nls);
        self.local_saved_bytes = Some(bytes);
        result?;
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
        if thumb_rgba.len() != expected && !(cfg!(feature = "psx") && thumb_rgba.is_empty()) {
            log::warn!(
                "finalize_save_write: thumbnail size mismatch: got={}, expected={}",
                thumb_rgba.len(),
                expected
            );
        }

        let mut bytes = self.build_save_file_bytes(nls.clone(), thumb_rgba)?;
        if let Some(snap) = state {
            crate::subsystem::save_state::append_state_chunk_v1(&mut bytes, snap)?;
        }
        self.write_current_slot(&bytes, nls)?;
        self.savedata_prepared = true;
        Ok(())
    }

    /// Called by the SaveWrite syscall once it has observed completion.
    pub fn consume_save_write_result(&mut self) {
        self.save_requested = false;
        self.savedata_prepared = false;
        self.current_save_slot = u32::MAX;
    }

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
        self.should_load = false;
        Some(self.load_slot)
    }

    /// Load a save slot into the current save fields.
    pub fn load_slot_into_current(&mut self, slot: u32, nls: Nls) -> Result<()> {
        let bytes = self.read_slot_file(slot)?;
        self.load_slot_into_current_from_bytes(slot, nls, &bytes)
    }

    pub fn load_slot_into_current_from_bytes(
        &mut self,
        slot: u32,
        nls: Nls,
        bytes: &[u8],
    ) -> Result<()> {
        let item = SaveItem::load_from_mem(bytes, nls)?;
        self.current_save_slot = slot;
        self.current_title = item.title.clone();
        self.current_scene_title = item.scene_title.clone();
        self.current_script_content = item.script_content.clone();
        if (slot as usize) < SLOT_COUNT {
            self.slots[slot as usize] = Some(Box::new(item));
        }
        Ok(())
    }

    fn build_save_file_bytes(&self, nls: Nls, thumb_rgba: &[u8]) -> Result<Vec<u8>> {
        fn enc(nls: Nls, s: &str) -> Vec<u8> {
            crate::text_codec::encode(nls, s)
        }

        // Save file prefix format:
        //   u16 year (LE), u8 month, u8 day, u8 day_of_week (Sunday=0), u8 hour, u8 minute,
        //   u16 title_len + title, u16 scene_len + scene, u16 script_len + script,
        //   RGBA8 thumbnail: 4 * thumb_w * thumb_h bytes
        let time = self.wall_time.unwrap_or(WallTime {
            year: 0,
            month: 0,
            day: 0,
            day_of_week: 0,
            hour: 0,
            minute: 0,
        });

        let texts = [
            enc(nls.clone(), &self.current_title),
            enc(nls.clone(), &self.current_scene_title),
            enc(nls, &self.current_script_content),
        ];

        let mut out: Vec<u8> = Vec::with_capacity(
            7 + 6
                + texts
                    .iter()
                    .map(|t| t.len().min(u16::MAX as usize))
                    .sum::<usize>()
                + thumb_rgba.len(),
        );
        out.extend_from_slice(&time.year.to_le_bytes());
        out.extend_from_slice(&[
            time.month,
            time.day,
            time.day_of_week,
            time.hour,
            time.minute,
        ]);
        for text in &texts {
            let len = text.len().min(u16::MAX as usize);
            out.extend_from_slice(&(len as u16).to_le_bytes());
            out.extend_from_slice(&text[..len]);
        }
        // The PlayStation saves to a memory card: its thumbnails are 16 colors.
        #[cfg(feature = "psx")]
        if thumb_rgba.is_empty() {
            let blank = psx_thumb::encoded_len(self.thumb_width.max(1), self.thumb_height.max(1));
            out.resize(out.len() + blank, 0);
        } else {
            out.extend_from_slice(&psx_thumb::encode(thumb_rgba));
        }
        #[cfg(not(feature = "psx"))]
        out.extend_from_slice(thumb_rgba);
        Ok(out)
    }
}

/// Save thumbnails on the PlayStation: 16 RGB555 colors (the most used, at 4 bits a channel),
/// then 4-bit indices, two a byte, low nibble first.
#[cfg(feature = "psx")]
mod psx_thumb {
    use alloc::vec::Vec;

    pub fn encoded_len(width: u32, height: u32) -> usize {
        32 + (width as usize * height as usize).div_ceil(2)
    }

    pub fn encode(rgba: &[u8]) -> Vec<u8> {
        let mut counts = alloc::vec![0u32; 4096];
        for p in rgba.chunks_exact(4) {
            counts[((p[0] as usize >> 4) << 8) | ((p[1] as usize >> 4) << 4) | (p[2] as usize >> 4)] += 1;
        }
        let mut order: Vec<usize> = (0..4096).filter(|&c| counts[c] > 0).collect();
        order.sort_by(|a, b| counts[*b].cmp(&counts[*a]));
        order.truncate(16);
        let palette: Vec<[i32; 3]> = order
            .iter()
            .map(|&c| [((c >> 8) as i32) * 17, (((c >> 4) & 15) as i32) * 17, ((c & 15) as i32) * 17])
            .collect();
        let mut out = Vec::with_capacity(32 + rgba.len() / 8);
        for i in 0..16 {
            let c = palette.get(i).copied().unwrap_or([0, 0, 0]);
            let v = (c[0] as u16 >> 3) | ((c[1] as u16 >> 3) << 5) | ((c[2] as u16 >> 3) << 10);
            out.extend_from_slice(&v.to_le_bytes());
        }
        let nearest = |p: &[u8]| -> u8 {
            let mut best = (i32::MAX, 0u8);
            for (i, c) in palette.iter().enumerate() {
                let d = (0..3).map(|k| (c[k] - p[k] as i32).pow(2)).sum::<i32>();
                if d < best.0 {
                    best = (d, i as u8);
                }
            }
            best.1
        };
        let indices: Vec<u8> = rgba.chunks_exact(4).map(nearest).collect();
        for pair in indices.chunks(2) {
            out.push(pair[0] | (pair.get(1).copied().unwrap_or(0) << 4));
        }
        out
    }

    pub fn decode(packed: &[u8], width: u32, height: u32) -> Vec<u8> {
        let e = |v: u16| ((v & 31) << 3 | (v & 31) >> 2) as u8;
        let count = width as usize * height as usize;
        let mut out = Vec::with_capacity(count * 4);
        for i in 0..count {
            let byte = packed.get(32 + i / 2).copied().unwrap_or(0);
            let index = if i & 1 == 0 { byte & 15 } else { byte >> 4 } as usize;
            let c = u16::from_le_bytes([packed[index * 2], packed[index * 2 + 1]]);
            out.extend_from_slice(&[e(c), e(c >> 5), e(c >> 10), 255]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_file_header_round_trips() {
        let mut manager = SaveManager::new();
        manager.set_wall_time(Some(WallTime {
            year: 2026,
            month: 10,
            day: 10,
            day_of_week: 6,
            hour: 21,
            minute: 5,
        }));
        manager.set_current_title("title".to_string());
        manager.set_current_scene_title("scene".to_string());
        manager.set_current_script_content("x".repeat(3000));
        manager.set_thumb_size(2, 2);
        // Opaque white round-trips exactly through both RGBA and the PS1's
        // lossy RGB555 palette. Arbitrary alpha bytes cannot survive that format.
        let thumb = vec![255u8; 4 * 2 * 2];
        let bytes = manager.build_save_file_bytes(Nls::UTF8, &thumb).unwrap();

        let item = SaveItem::load_from_mem(&bytes, Nls::UTF8).unwrap();
        assert_eq!((item.year, item.month, item.day, item.hour), (2026, 10, 10, 21));
        assert_eq!(item.title, "title");
        assert_eq!(item.script_content.len(), 3000);
        assert_eq!(
            SaveItem::read_thumb_texture_from_mem(&bytes, 2, 2).unwrap(),
            thumb
        );
        // A header longer than the first read is still found.
        assert!(bytes.len() > HEADER_FIRST_READ);
    }
}
