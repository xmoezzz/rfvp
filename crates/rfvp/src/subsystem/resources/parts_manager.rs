use super::texture::{NvsgTexture, TextureType};
use anyhow::Result;
use image::DynamicImage;

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
        self.loaded = true;

        Ok(())
    }

    pub fn set_color_tone(&mut self, r: u8, g: u8, b: u8) {
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
