use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ColorItem {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl ColorItem {
    pub fn new() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    pub fn set_r(&mut self, r: u8) {
        self.r = r;
    }

    pub fn set_g(&mut self, g: u8) {
        self.g = g;
    }

    pub fn set_b(&mut self, b: u8) {
        self.b = b;
    }

    pub fn set_a(&mut self, a: u8) {
        self.a = a;
    }

    pub fn get_r(&self) -> u8 {
        self.r
    }

    pub fn get_g(&self) -> u8 {
        self.g
    }

    pub fn get_b(&self) -> u8 {
        self.b
    }

    pub fn get_a(&self) -> u8 {
        self.a
    }
}


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ColorManager {
    // 256 entries in total
    colors: Vec<ColorItem>
}

impl ColorManager {
    pub fn new() -> Self {
        Self {
            colors: vec![ColorItem::new(); 256],
        }
    }

    pub fn get_entry_mut(&mut self, id: u8) -> &mut ColorItem {
        &mut self.colors[id as usize]
    }
    
    pub fn get_entry(&self, id: u8) -> &ColorItem {
        &self.colors[id as usize]
    }
}

impl Default for ColorManager {
    fn default() -> Self {
        Self::new()
    }
}


