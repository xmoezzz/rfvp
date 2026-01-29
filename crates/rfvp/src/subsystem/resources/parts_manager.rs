use super::texture::{NvsgTexture, TextureType};
use anyhow::Result;
use image::DynamicImage;
use serde::{Deserialize, Serialize};

use super::vfs::Vfs;

#[derive(Debug, Clone)]
pub struct PartsItem {
    prim_id: u16,
    r_value: u8,
    g_value: u8,
    b_value: u8,
    running: bool,
    texture: NvsgTexture,
    texture_name: String,
    loaded: bool,
}

impl PartsItem {
    pub fn new() -> Self {
        Self {
            prim_id: 0,
            r_value: 0,
            g_value: 0,
            b_value: 0,
            running: false,
            texture: NvsgTexture::new(""),
            texture_name: String::new(),
            loaded: false,
        }
    }

    pub fn load_texture(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        self.texture
            .read_texture(&buff, |typ| typ == TextureType::Multi32Bit)?;

        self.texture.set_name(file_name);
        self.texture_name = file_name.to_string();
        self.r_value = 100;
        self.g_value = 100;
        self.b_value = 100;
        // `PartsLoad` resets the pause flag in the original engine.
        self.running = false;
        self.loaded = true;

        Ok(())
    }

    pub fn get_loaded(&self) -> bool {
        self.loaded
    }

    /// Release decoded pixel data while keeping header metadata and the stored name.
    ///
    /// This matches the original engine behavior for `PartsLoad(id, nil)`.
    pub fn unload_texture_keep_name(&mut self) {
        self.texture.clear_slices_keep_header();
        self.loaded = false;
        self.running = false;
    }

    pub fn set_color_tone(&mut self, r: u8, g: u8, b: u8) {
        // The original engine only updates RGB when the parts buffer exists.
        if !self.loaded {
            return;
        }
        for index in 0..self.texture.get_entry_count() as usize {
            let _ = self.texture
                .texture_color_tone_32(index, r as i32, g as i32, b as i32);
        }

        self.r_value = r;
        self.g_value = g;
        self.b_value = b;
    }

    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    pub fn get_texture_count(&self) -> u32 {
        self.texture.get_entry_count()
    }

    pub fn get_texture(&self, index: usize) -> Result<DynamicImage> {
        self.texture.get_texture(index)
    }

    pub fn get_prim_id(&self) -> u16 {
        self.prim_id
    }

    pub fn get_width(&self) -> u16 {
        self.texture.get_width()
    }

    pub fn get_height(&self) -> u16 {
        self.texture.get_height()
    }

    pub fn get_offset_x(&self) -> u16 {
        self.texture.get_offset_x()
    }

    pub fn get_offset_y(&self) -> u16 {
        self.texture.get_offset_y()
    }

    /// Parts offsets are treated as signed in the original engine.
    ///
    /// Some assets intentionally place the overlay partially outside the destination graph.
    /// The copy path must therefore support negative offsets via clipping.
    pub fn get_offset_x_i16(&self) -> i16 {
        self.texture.get_offset_x() as i16
    }

    pub fn get_offset_y_i16(&self) -> i16 {
        self.texture.get_offset_y() as i16
    }

    pub fn get_running(&self) -> bool {
        self.running
    }
}

#[derive(Debug, Clone)]
pub struct PartsMotion {
    running: bool,
    parts_id: u8,
    entry_id: u8,
    id: u8,
    elapsed: u32,
    duration: u32,
}

impl PartsMotion {
    pub fn new() -> Self {
        Self {
            running: false,
            parts_id: 0,
            entry_id: 0,
            id: 0,
            elapsed: 0,
            duration: 0,
        }
    }

    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    pub fn set_parts_id(&mut self, parts_id: u8) {
        self.parts_id = parts_id;
    }

    pub fn set_entry_id(&mut self, entry_id: u8) {
        self.entry_id = entry_id;
    }

    pub fn set_id(&mut self, id: u8) {
        self.id = id;
    }

    pub fn set_elapsed(&mut self, elapsed: u32) {
        self.elapsed = elapsed;
    }

    pub fn set_duration(&mut self, duration: u32) {
        self.duration = duration;
    }

    pub fn get_running(&self) -> bool {
        self.running
    }

    pub fn get_parts_id(&self) -> u8 {
        self.parts_id
    }

    pub fn get_entry_id(&self) -> u8 {
        self.entry_id
    }

    pub fn get_id(&self) -> u8 {
        self.id
    }

    pub fn get_elapsed(&self) -> u32 {
        self.elapsed
    }

    pub fn get_duration(&self) -> u32 {
        self.duration
    }
}

#[derive(Debug)]
pub struct PartsManager {
    parts: Vec<PartsItem>,
    parts_motions: Vec<PartsMotion>,
    allocation_pool: Vec<u8>,
    pub current_id: u8,
}

impl PartsManager {
    pub fn new() -> Self {
        let allocation_pool: Vec<u8> = (0..8).collect();

        Self {
            parts: vec![PartsItem::new(); 64],
            parts_motions: vec![PartsMotion::new(); 8],
            allocation_pool,
            current_id: 0,
        }
    }

    pub fn load_parts(&mut self, id: u16, file_name: &str, buff: Vec<u8>) -> Result<()> {
        self.parts[id as usize].load_texture(file_name, buff)?;
        Ok(())
    }

    /// Release decoded pixel data while keeping header metadata and the stored name.
    ///
    /// This matches the original engine behavior for `PartsLoad(id, nil)`.
    pub fn unload_parts_keep_name(&mut self, id: u8) {
        if (id as usize) < self.parts.len() {
            self.parts[id as usize].unload_texture_keep_name();
        }
    }

    pub fn set_rgb(&mut self, id: u16, r: u8, g: u8, b: u8) {
        self.parts[id as usize].set_color_tone(r, g, b);
    }

    /// Stop (and recycle) any running PartsMotion for this parts id.
    ///
    /// The original engine uses a small fixed-size pool for PartsMotion slots.
    /// We model it as a stack of free slot IDs (`allocation_pool`) plus a handle count (`current_id`).
    fn unload_motion_for_parts(&mut self, parts_id: u8) {
        for i in 0..self.parts_motions.len() {
            if self.parts_motions[i].get_running() && self.parts_motions[i].get_parts_id() == parts_id {
                let slot_id = self.parts_motions[i].get_id();
                self.parts_motions[i].set_running(false);
                // Recycle slot id.
                if self.current_id > 0 {
                    self.current_id -= 1;
                    self.allocation_pool[self.current_id as usize] = slot_id;
                }
                return;
            }
        }
    }

    /// Back-compat helper (older call sites used it as "free the motion slot").
    ///
    /// Note: this does **not** allocate; allocation is done by `set_motion`.
    pub fn next_free_id(&mut self, parts_id: u8) -> Option<u8> {
        self.unload_motion_for_parts(parts_id);
        None
    }

    pub fn get(&self, id: u8) -> &PartsItem {
        &self.parts[id as usize]
    }

    pub fn get_mut(&mut self, id: u8) -> &mut PartsItem {
        &mut self.parts[id as usize]
    }

    pub fn set_motion(&mut self, parts_id: u8, entry_id: u8, time: u32) -> Result<()> {
        // Only one motion per parts_id.
        self.unload_motion_for_parts(parts_id);

        // Pool exhausted.
        if self.current_id as usize >= self.allocation_pool.len() {
            return Ok(());
        }

        // Allocate a motion slot from the free stack.
        let slot_id = self.allocation_pool[self.current_id as usize];
        self.current_id += 1;

        let parts_motion = &mut self.parts_motions[slot_id as usize];
        parts_motion.set_id(slot_id);
        parts_motion.set_running(true);
        parts_motion.set_parts_id(parts_id);
        parts_motion.set_entry_id(entry_id);
        parts_motion.set_duration(time);
        parts_motion.set_elapsed(0);

        Ok(())
    }

    pub fn test_motion(&self, parts_id: u8) -> bool {
        self.parts_motions
            .iter()
            .any(|m| m.get_running() && m.get_parts_id() == parts_id)
    }

    pub fn stop_motion(&mut self, parts_id: u8) -> Result<()> {
        self.unload_motion_for_parts(parts_id);

        Ok(())
    }

    /// Advance all running parts motions and emit completed ones.
    ///
    /// The caller is responsible for applying the completed entry to the destination graph.
    pub fn tick_motions(&mut self, elapsed_ms: u32, completed: &mut Vec<(u8, u8)>) {
        if elapsed_ms == 0 {
            return;
        }

        for i in 0..self.parts_motions.len() {
            if !self.parts_motions[i].get_running() {
                continue;
            }

            let parts_id = self.parts_motions[i].get_parts_id();

            // PartsMotionPause toggles this flag per parts_id.
            // Here we interpret it as a "paused" switch.
            if self.parts[parts_id as usize].get_running() {
                continue;
            }

            let elapsed = self.parts_motions[i].get_elapsed().saturating_add(elapsed_ms);
            self.parts_motions[i].set_elapsed(elapsed);

            let duration = self.parts_motions[i].get_duration();
            if duration != 0 && elapsed >= duration {
                let entry_id = self.parts_motions[i].get_entry_id();
                let slot_id = self.parts_motions[i].get_id();

                self.parts_motions[i].set_running(false);

                // Recycle slot id.
                if self.current_id > 0 {
                    self.current_id -= 1;
                    self.allocation_pool[self.current_id as usize] = slot_id;
                }

                completed.push((parts_id, entry_id));
            }
        }
    }

    pub fn assign_prim_id(&mut self, parts_id: u8, prim_id: u16) {
        self.parts[parts_id as usize].prim_id = prim_id;
    }
}

impl Default for PartsManager {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------
// Save/Load snapshots
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartsItemSnapshotV1 {
    pub prim_id: u16,
    pub r_value: u8,
    pub g_value: u8,
    pub b_value: u8,
    pub running: bool,
    pub texture_name: String,
    pub loaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartsMotionSnapshotV1 {
    pub running: bool,
    pub parts_id: u8,
    pub entry_id: u8,
    pub id: u8,
    pub elapsed: u32,
    pub duration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartsManagerSnapshotV1 {
    pub parts: Vec<PartsItemSnapshotV1>,
    pub parts_motions: Vec<PartsMotionSnapshotV1>,
    pub allocation_pool: Vec<u8>,
    pub current_id: u8,
}

impl PartsItem {
    fn capture_snapshot_v1(&self) -> PartsItemSnapshotV1 {
        PartsItemSnapshotV1 {
            prim_id: self.prim_id,
            r_value: self.r_value,
            g_value: self.g_value,
            b_value: self.b_value,
            running: self.running,
            texture_name: self.texture_name.clone(),
            loaded: self.loaded,
        }
    }

    fn apply_snapshot_v1(&mut self, snap: &PartsItemSnapshotV1, vfs: &Vfs) -> Result<()> {
        self.prim_id = snap.prim_id;
        self.r_value = snap.r_value;
        self.g_value = snap.g_value;
        self.b_value = snap.b_value;
        self.running = snap.running;

        self.texture_name = snap.texture_name.clone();
        self.loaded = false;
        self.texture = NvsgTexture::new("");

        if snap.loaded && !snap.texture_name.is_empty() {
            let bytes = vfs.read_file(&snap.texture_name)?;
            self.load_texture(&snap.texture_name, bytes)?;
            self.set_color_tone(snap.r_value, snap.g_value, snap.b_value);
        }
        self.loaded = snap.loaded;
        Ok(())
    }
}

impl PartsMotion {
    fn capture_snapshot_v1(&self) -> PartsMotionSnapshotV1 {
        PartsMotionSnapshotV1 {
            running: self.running,
            parts_id: self.parts_id,
            entry_id: self.entry_id,
            id: self.id,
            elapsed: self.elapsed,
            duration: self.duration,
        }
    }

    fn apply_snapshot_v1(&mut self, snap: &PartsMotionSnapshotV1) {
        self.running = snap.running;
        self.parts_id = snap.parts_id;
        self.entry_id = snap.entry_id;
        self.id = snap.id;
        self.elapsed = snap.elapsed;
        self.duration = snap.duration;
    }
}

impl PartsManager {
    pub fn capture_snapshot_v1(&self) -> PartsManagerSnapshotV1 {
        PartsManagerSnapshotV1 {
            parts: self.parts.iter().map(|p| p.capture_snapshot_v1()).collect(),
            parts_motions: self
                .parts_motions
                .iter()
                .map(|m| m.capture_snapshot_v1())
                .collect(),
            allocation_pool: self.allocation_pool.clone(),
            current_id: self.current_id,
        }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &PartsManagerSnapshotV1, vfs: &Vfs) -> Result<()> {
        if self.parts.len() != snap.parts.len() {
            self.parts = vec![PartsItem::new(); snap.parts.len().max(64)];
        }
        if self.parts_motions.len() != snap.parts_motions.len() {
            self.parts_motions = vec![PartsMotion::new(); snap.parts_motions.len().max(8)];
        }

        let n = self.parts.len().min(snap.parts.len());
        for i in 0..n {
            self.parts[i].apply_snapshot_v1(&snap.parts[i], vfs)?;
        }

        let m = self.parts_motions.len().min(snap.parts_motions.len());
        for i in 0..m {
            self.parts_motions[i].apply_snapshot_v1(&snap.parts_motions[i]);
        }

        self.allocation_pool = snap.allocation_pool.clone();
        self.current_id = snap.current_id;
        Ok(())
    }
}
