use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use anyhow::Result;

use super::graph_buff::{GraphBuff, GraphBuffSnapshotV1};
use super::vfs::Vfs;

#[derive(Debug, Clone)]
pub struct GaijiItem {
    pub key: String,
    pub size: u8,
    pub texture: GraphBuff,
}

impl GaijiItem {
    pub fn new(key: String, size: u8, texture: GraphBuff) -> Self {
        Self {
            key,
            size,
            texture,
        }
    }

    pub fn set_key(&mut self, key: String) {
        self.key = key;
    }

    pub fn set_size(&mut self, size: u8) {
        self.size = size;
    }

    pub fn get_key(&self) -> &str {
        &self.key
    }

    pub fn get_size(&self) -> u8 {
        self.size
    }

    pub fn get_texture(&self) -> &GraphBuff {
        &self.texture
    }
}

pub struct GaijiManager {
    item: HashMap<String, BTreeMap<u8, GaijiItem>>
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

    pub fn set_gaiji(&mut self, key: String, size: u8, texture: GraphBuff) {
        let item = GaijiItem::new(key.clone(), size, texture);
        self.item.entry(key).or_insert_with(BTreeMap::new).insert(size, item);
    }

    pub fn get_exact(&self, key: &str, size: u8) -> Option<&GaijiItem> {
        self.item.get(key)?.get(&size)
    }

    pub fn get_nearest(&self, key: &str, size: u8) -> Option<&GaijiItem> {
        let versions = self.item.get(key)?;
        let mut best: Option<&GaijiItem> = None;
        let mut best_delta = u16::MAX;
        for (entry_size, item) in versions.iter() {
            let delta = (*entry_size as i16 - size as i16).unsigned_abs();
            if delta < best_delta {
                best_delta = delta;
                best = Some(item);
            }
        }
        best
    }

    pub fn get(&self, key: &str, size: u8) -> Option<&GaijiItem> {
        self.get_exact(key, size).or_else(|| self.get_nearest(key, size))
    }

    pub fn get_texture(&self, key: &str, size: u8) -> Option<&GraphBuff> {
        self.get(key, size).map(|it| it.get_texture())
    }

}

// ----------------------------
// Save/Load snapshots
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaijiEntrySnapshotV1 {
    pub key: String,
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
        for (key, size_map) in &self.item {
            for (size, item) in size_map {
                entries.push(GaijiEntrySnapshotV1 {
                    key: key.clone(),
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
            self.set_gaiji(e.key.clone(), e.size, gb);
        }
        Ok(())
    }
}