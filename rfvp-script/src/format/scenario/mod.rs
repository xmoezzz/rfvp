pub mod instructions;
pub mod variant;

use std::{collections::HashMap, io::Cursor, str::FromStr};

use anyhow::{bail, Result};
use binrw::{BinRead, BinWrite};
use bytes::Bytes;


#[derive(Debug, Clone, Default)]
pub enum Nls {
    #[default]
    ShiftJIS = 0,
    GBK = 1,
    UTF8 = 2,
}


impl FromStr for Nls {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "sjis" => Ok(Nls::ShiftJIS),
            "gbk" => Ok(Nls::GBK),
            "utf8" => Ok(Nls::UTF8),
            _ => Err(anyhow::anyhow!("unknown NLS")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Syscall {
    /// how many arguments the syscall takes from the stack
    pub args: u8,
    /// name of the syscall
    pub name: String,
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct Scenario {
    raw_data: Bytes,
    pub nls: Nls,
    pub sys_desc_offset: u32,
    /// entry point (offset) of the script
    pub entry_point: u32,
    pub non_volatile_global_count: u16,
    pub volatile_global_count: u16,
    // register a script function as syscall, never use?
    pub custom_syscall_count: u16,
    /// Game resolution for the window mode
    game_mode: u16,
    game_title: String,
    pub syscall_count: u16,
    pub syscalls: HashMap<usize, Syscall>,
}

impl Scenario {
    pub fn new(data: Bytes, nls: Option<Nls>) -> Result<Self> {
        let mut scenario = Scenario {
            raw_data: data,
            nls: nls.unwrap_or(Nls::ShiftJIS),
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

        scenario.parser()?;

        Ok(scenario)
    }

    #[inline]
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    /// safely read a u8 from the buffer
    pub fn read_u8(&self, offset: usize) -> Result<u8> {
        if offset >= self.raw().len() {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        Ok(self.raw()[offset])
    }

    /// safely read a little-endian u16 from the buffer
    pub fn read_u16(&self, offset: usize) -> Result<u16> {
        if offset + 1 >= self.raw().len() {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        Ok(u16::from_le_bytes([self.raw()[offset], self.raw()[offset + 1]]))
    }

    /// safely read a little-endian u32 from the buffer
    pub fn read_u32(&self, offset: usize) -> Result<u32> {
        if offset + 3 >= self.raw().len() {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        Ok(u32::from_le_bytes([
            self.raw()[offset],
            self.raw()[offset + 1],
            self.raw()[offset + 2],
            self.raw()[offset + 3],
        ]))
    }

    /// safely read a little-endian i8 from the buffer
    pub fn read_i8(&self, offset: usize) -> Result<i8> {
        if offset >= self.raw().len() {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        Ok(self.raw()[offset] as i8)
    }

    /// safely read a little-endian i16 from the buffer
    pub fn read_i16(&self, offset: usize) -> Result<i16> {
        if offset + 1 >= self.raw().len() {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        Ok(i16::from_le_bytes([self.raw()[offset], self.raw()[offset + 1]]))
    }

    /// safely read a little-endian i32 from the buffer
    pub fn read_i32(&self, offset: usize) -> Result<i32> {
        if offset + 3 >= self.raw().len() {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        Ok(i32::from_le_bytes([
            self.raw()[offset],
            self.raw()[offset + 1],
            self.raw()[offset + 2],
            self.raw()[offset + 3],
        ]))
    }

    /// safely read a little-endian f32 from the buffer
    pub fn read_f32(&self, offset: usize) -> Result<f32> {
        if offset + 3 >= self.raw().len() {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        Ok(f32::from_le_bytes([
            self.raw()[offset],
            self.raw()[offset + 1],
            self.raw()[offset + 2],
            self.raw()[offset + 3],
        ]))
    }

    /// safe read a c-style string from the buffer with string length
    /// (with null terminator)
    /// then convert it to a UTF-8 string due to the NLS
    pub fn read_cstring(&self, offset: usize, len: usize) -> Result<String> {
        if offset + len >= self.raw().len() {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        let mut string = Vec::new();
        for i in 0..len {
            if self.raw()[offset + i] == 0 {
                break;
            }
            string.push(self.raw()[offset + i]);
        }

        if string.ends_with(&[0]) {
            string.pop();
        }
        
        let s = match self.nls {
            Nls::ShiftJIS => {
                let (s, _, e) = encoding_rs::SHIFT_JIS.decode(&string);
                if e {
                    log::error!("failed to decode string as ShiftJIS");
                }
                s
            }
            Nls::GBK => {
                let (s, _, e) = encoding_rs::GBK.decode(&string);
                if e {
                    log::error!("failed to decode string as GBK");
                }
                s
            }
            Nls::UTF8 => {
                let (s, _, e) = encoding_rs::UTF_8.decode(&string);
                if e {
                    log::error!("failed to decode string as UTF-8");
                }
                s
            }
        };

        Ok(s.to_string())
    }

    fn parser(&mut self) -> Result<()> {
        let mut off = 0usize;
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

        let title_len = self.read_u8(off)?;
        off += size_of::<u8>();

        self.game_title = self.read_cstring(off, title_len as usize)?;
        off += title_len as usize;

        self.syscall_count = self.read_u16(off)?;
        off += size_of::<u16>();

        for i in 0..self.syscall_count {
            let args = self.read_u8(off)?;
            off += size_of::<u8>();

            let name_len = self.read_u8(off)?;
            off += size_of::<u8>();

            let name = self.read_cstring(off, name_len as usize)?;
            off += name_len as usize;

            self.syscalls.insert(i as usize, Syscall { args, name });
        }

        self.custom_syscall_count = self.read_u16(off)?;
        if self.custom_syscall_count > 0 {
            log::warn!("custom syscall count: {}", self.custom_syscall_count);
        }

        Ok(())
    }

    pub fn get_syscall_name(&self, id: u16) -> Option<&str> {
        self.syscalls.get(&(id as usize)).map(|s| s.name.as_str())
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

    pub fn get_title(&self) -> String {
        self.game_title.clone()
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
                log::error!("unknown resolution: {}, use 640x480 as defualt", self.game_mode);
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

    // the upper bound of the code area
    pub fn get_sys_desc_offset(&self) -> u32 {
        self.sys_desc_offset
    }
}


