use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::mem::size_of;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;
use serde::{Serialize, Deserialize};

use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum Nls {
    #[default]
    ShiftJIS = 0,
    GBK = 1,
    UTF8 = 2,
}

impl FromStr for Nls {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "sjis" => Ok(Nls::ShiftJIS),
            "gbk" => Ok(Nls::GBK),
            "utf8" => Ok(Nls::UTF8),
            _ => Err(anyhow!("unknown NLS")),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Syscall {
    /// How many arguments the syscall takes from the stack.
    pub args: u8,
    /// Name of the syscall.
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Parser {
    #[serde(skip)]
    pub buffer: Rc<Vec<u8>>,
    pub nls: Nls,
    pub sys_desc_offset: u32,
    /// Entry point (offset) of the script.
    pub entry_point: u32,
    pub non_volatile_global_count: u16,
    pub volatile_global_count: u16,
    /// Register a script function as syscall (usually unused).
    pub custom_syscall_count: u16,
    /// Game resolution id.
    pub game_mode: u16,
    pub game_title: String,
    pub syscall_count: u16,
    pub syscalls: HashMap<usize, Syscall>,
}

impl Parser {
    pub fn new(path: impl AsRef<Path>, nls: Nls) -> Result<Self> {
        let mut rdr = File::open(path)?;
        let mut buffer = Vec::new();
        rdr.read_to_end(&mut buffer)?;

        let mut parser = Parser {
            buffer: Rc::new(buffer),
            nls,
            sys_desc_offset: 0,
            entry_point: 0,
            non_volatile_global_count: 0,
            volatile_global_count: 0,
            custom_syscall_count: 0,
            game_mode: 0,
            game_title: String::new(),
            syscall_count: 0,
            syscalls: HashMap::new(),
        };

        parser.parse_header()?;
        Ok(parser)
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn read_u8(&self, offset: usize) -> Result<u8> {
        self.buffer.get(offset).copied().ok_or_else(|| anyhow!("offset out of bounds"))
    }

    pub fn read_i8(&self, offset: usize) -> Result<i8> {
        Ok(self.read_u8(offset)? as i8)
    }

    pub fn read_u16(&self, offset: usize) -> Result<u16> {
        if offset + 1 >= self.buffer.len() {
            return Err(anyhow!("offset out of bounds"));
        }
        Ok(u16::from_le_bytes([self.buffer[offset], self.buffer[offset + 1]]))
    }

    pub fn read_i16(&self, offset: usize) -> Result<i16> {
        Ok(self.read_u16(offset)? as i16)
    }

    pub fn read_u32(&self, offset: usize) -> Result<u32> {
        if offset + 3 >= self.buffer.len() {
            return Err(anyhow!("offset out of bounds"));
        }
        Ok(u32::from_le_bytes([
            self.buffer[offset],
            self.buffer[offset + 1],
            self.buffer[offset + 2],
            self.buffer[offset + 3],
        ]))
    }

    pub fn read_i32(&self, offset: usize) -> Result<i32> {
        Ok(self.read_u32(offset)? as i32)
    }

    pub fn read_f32(&self, offset: usize) -> Result<f32> {
        if offset + 3 >= self.buffer.len() {
            return Err(anyhow!("offset out of bounds"));
        }
        Ok(f32::from_le_bytes([
            self.buffer[offset],
            self.buffer[offset + 1],
            self.buffer[offset + 2],
            self.buffer[offset + 3],
        ]))
    }

    /// Read a C-style string with a maximum length `len` (may contain an early NUL).
    /// Then decode it into UTF-8 according to the configured NLS.
    pub fn read_cstring(&self, offset: usize, len: usize) -> Result<String> {
        if offset + len > self.buffer.len() {
            return Err(anyhow!("offset out of bounds"));
        }
        let mut raw = Vec::new();
        for i in 0..len {
            let b = self.buffer[offset + i];
            if b == 0 {
                break;
            }
            raw.push(b);
        }

        let decoded = match self.nls {
            Nls::ShiftJIS => {
                let (s, _, had_err) = encoding_rs::SHIFT_JIS.decode(&raw);
                if had_err {
                    log::warn!("ShiftJIS decode error");
                }
                s
            }
            Nls::GBK => {
                let (s, _, had_err) = encoding_rs::GBK.decode(&raw);
                if had_err {
                    log::warn!("GBK decode error");
                }
                s
            }
            Nls::UTF8 => {
                let (s, _, had_err) = encoding_rs::UTF_8.decode(&raw);
                if had_err {
                    log::warn!("UTF-8 decode error");
                }
                s
            }
        };

        Ok(decoded.to_string())
    }

    fn parse_header(&mut self) -> Result<()> {
        let mut off: usize = 0;
        self.sys_desc_offset = self.read_u32(off)?;

        off = self.sys_desc_offset as usize;
        self.entry_point = self.read_u32(off)?;
        off += size_of::<u32>();

        self.non_volatile_global_count = self.read_u16(off)?;
        off += size_of::<u16>();

        self.volatile_global_count = self.read_u16(off)?;
        off += size_of::<u16>();

        self.game_mode = self.read_u16(off)?;
        off += size_of::<u16>();

        let title_len = self.read_u8(off)? as usize;
        off += size_of::<u8>();

        self.game_title = self.read_cstring(off, title_len)?;
        off += title_len;

        self.syscall_count = self.read_u16(off)?;
        off += size_of::<u16>();

        for i in 0..self.syscall_count {
            let args = self.read_u8(off)?;
            off += size_of::<u8>();

            let name_len = self.read_u8(off)? as usize;
            off += size_of::<u8>();

            let name = self.read_cstring(off, name_len)?;
            off += name_len;

            self.syscalls.insert(i as usize, Syscall { args, name });
        }

        self.custom_syscall_count = self.read_u16(off)?;
        if self.custom_syscall_count > 0 {
            log::warn!("custom syscall count: {}", self.custom_syscall_count);
        }

        Ok(())
    }

    pub fn is_code_area(&self, addr: u32) -> bool {
        addr >= 4 && addr < self.sys_desc_offset
    }

    pub fn get_syscall(&self, id: u16) -> Option<&Syscall> {
        self.syscalls.get(&(id as usize))
    }

    pub fn get_all_syscalls(&self) -> &HashMap<usize, Syscall> {
        &self.syscalls
    }

    pub fn export_yaml(&self, path: impl AsRef<Path>) -> Result<()> {
        let s = serde_yml::to_string(self)?;
        std::fs::write(path, s)?;
        Ok(())
    }
}

impl Parser {
    pub fn get_title(&self) -> &str {
        &self.game_title
    }

    pub fn get_non_volatile_global_count(&self) -> u16 {
        self.non_volatile_global_count
    }

    pub fn get_volatile_global_count(&self) -> u16 {
        self.volatile_global_count
    }

    pub fn get_screen_size(&self) -> (u32, u32) {
        match self.game_mode {
            0 => (640, 480),
            1 => (800, 600),
            2 => (1024, 768),
            3 => (1280, 960),
            4 => (1600, 1200),
            5 => (640, 480),
            6 => (1024, 576),
            7 => (1024, 640),
            8 => (1280, 720),
            9 => (1280, 800),
            10 => (1440, 810),
            11 => (1440, 900),
            12 => (1680, 945),
            13 => (1680, 1050),
            14 => (1920, 1080),
            15 => (1920, 1200),
            _ => {
                log::warn!("unknown resolution: {}, defaulting to 640x480", self.game_mode);
                (640, 480)
            }
        }
    }

    pub fn get_game_mode(&self) -> u16 {
        self.game_mode
    }

    pub fn get_entry_point(&self) -> u32 {
        self.entry_point
    }

    pub fn get_custom_syscall_count(&self) -> u16 {
        self.custom_syscall_count
    }

    pub fn get_sys_desc_offset(&self) -> u32 {
        self.sys_desc_offset
    }
}
