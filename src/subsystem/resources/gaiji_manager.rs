use std::collections::HashMap;
use super::graph_buff::GraphBuff;

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
}