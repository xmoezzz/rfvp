use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use anyhow::Result;

use super::graph_buff::{GraphBuff, GraphBuffSnapshotV1};
use super::vfs::Vfs;

#[derive(Debug, Clone)]
pub struct GaijiItem {
    pub code: char,
    pub size: u8,
    pub texture: GraphBuff,
}

impl GaijiItem {
    pub fn new(code: char, size: u8, texture: GraphBuff) -> Self {
        Self {
            code,
            size,
            texture,
        }
    }

    pub fn set_code(&mut self, code: char) {
        self.code = code;
    }

    pub fn set_size(&mut self, size: u8) {
        self.size = size;
    }

    pub fn get_code(&self) -> char {
        self.code
    }

    pub fn get_size(&self) -> u8 {
        self.size
    }

    pub fn get_texture(&self) -> &GraphBuff {
        &self.texture
    }
}

pub struct GaijiManager {
    item: HashMap<char, HashMap<u8, GaijiItem>>
}

impl Default for GaijiManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GaijiManager {
    pub fn new() -> Self {
        Self {
            item: HashMap::new()
        }
    }

    pub fn set_gaiji(&mut self, code: char, size: u8, texture: GraphBuff) {
        let item = GaijiItem::new(code, size, texture);
        self.item.entry(code).or_insert_with(HashMap::new);
        if let Some(entry) = self.item.get_mut(&code) {
            entry.insert(size, item);
        }
    }

    /// Lookup a gaiji mapping for the given trigger character and size slot.
    pub fn get(&self, code: char, size: u8) -> Option<&GaijiItem> {
        self.item.get(&code)?.get(&size)
    }

    /// Convenience helper to access the mapped texture, if any.
    pub fn get_texture(&self, code: char, size: u8) -> Option<&GraphBuff> {
        self.get(code, size).map(|it| it.get_texture())
    }

}

// ----------------------------
// Save/Load snapshots
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaijiEntrySnapshotV1 {
    pub code: char,
    pub size: u8,
    pub texture: GraphBuffSnapshotV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaijiManagerSnapshotV1 {
    pub entries: Vec<GaijiEntrySnapshotV1>,
}

impl GaijiManager {
    pub fn capture_snapshot_v1(&self) -> GaijiManagerSnapshotV1 {
        let mut entries = Vec::new();
        for (code, size_map) in &self.item {
            for (size, item) in size_map {
                // Gaiji textures are always 1bpp glyph images, so we keep load_kind.
                entries.push(GaijiEntrySnapshotV1 {
                    code: *code,
                    size: *size,
                    texture: item.texture.capture_snapshot_with_id(0),
                });
            }
        }
        GaijiManagerSnapshotV1 { entries }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &GaijiManagerSnapshotV1, vfs: &Vfs) -> Result<()> {
        self.item.clear();
        for e in &snap.entries {
            let mut gb = GraphBuff::new();
            gb.apply_snapshot_v1(&e.texture, vfs)?;
            self.set_gaiji(e.code, e.size, gb);
        }
        Ok(())
    }
}