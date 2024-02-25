
#[derive(Clone)]
pub struct GraphBuffTexture {
    pub texture: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl GraphBuffTexture {
    pub fn new() -> Self {
        Self {
            texture: Vec::new(),
            width: 0,
            height: 0,
        }
    }
}

pub struct GraphBuff {
    pub textures: Vec<GraphBuffTexture>,
    pub r_value: u8,
    pub g_value: u8,
    pub b_value: u8,
    pub texture_ready: bool,
    pub texture_path: String,
    pub offset_x: u16,
    pub offset_y: u16,
}

impl GraphBuff {
    pub fn new() -> Self {
        Self {
            textures: vec![GraphBuffTexture::new(); 16],
            r_value: 0,
            g_value: 0,
            b_value: 0,
            texture_ready: false,
            texture_path: String::new(),
            offset_x: 0,
            offset_y: 0,
        }
    }
}