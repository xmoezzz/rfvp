
#[derive(Debug, Clone)]
pub struct PartsItem {
    prim_id: u16,
    // uint16_t unknown1;
    // uint16_t type_;
    // uint16_t width;
    // uint16_t height;
    // uint16_t offset_x;
    // uint16_t offset_y;
    // uint32_t unknown2;
    // uint32_t entry_count;
    // uint32_t unknown3;
    // uint32_t unknown4;
    r_value: u8,
    g_value: u8,
    b_value: u8,
    running: bool,
}

impl PartsItem {
    pub fn new() -> Self {
        Self {
            prim_id: 0,
            r_value: 0,
            g_value: 0,
            b_value: 0,
            running: false,
        }
    }
}

#[derive(Debug)]
pub struct PartsManager {
    pub parts: Vec<PartsItem>,
}

impl PartsManager {
    pub fn new() -> Self {
        Self {
            parts: vec![PartsItem::new(); 64],
        }
    }

    pub fn load_parts(&mut self, id: u16, file_name: &str) {
        
    }
}

impl Default for PartsManager {
    fn default() -> Self {
        Self::new()
    }
}

