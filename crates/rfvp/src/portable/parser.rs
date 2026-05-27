use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::mem::size_of;

use super::vm::{VmError, VmResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Nls {
    #[default]
    ShiftJis,
    Gbk,
    Utf8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Syscall {
    pub args: u8,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Parser {
    buffer: Vec<u8>,
    pub nls: Nls,
    pub sys_desc_offset: u32,
    pub entry_point: u32,
    pub non_volatile_global_count: u16,
    pub volatile_global_count: u16,
    pub custom_syscall_count: u16,
    game_mode: u8,
    game_mode_reserved: u8,
    game_title: String,
    pub syscall_count: u16,
    syscalls: Vec<Syscall>,
}

impl Parser {
    pub fn from_bytes(buffer: impl Into<Vec<u8>>, nls: Nls) -> VmResult<Self> {
        let mut parser = Self {
            buffer: buffer.into(),
            nls,
            sys_desc_offset: 0,
            entry_point: 0,
            non_volatile_global_count: 0,
            volatile_global_count: 0,
            custom_syscall_count: 0,
            game_mode: 0,
            game_mode_reserved: 0,
            game_title: String::new(),
            syscall_count: 0,
            syscalls: Vec::new(),
        };
        parser.parse()?;
        Ok(parser)
    }

    pub fn read_u8(&self, offset: usize) -> VmResult<u8> {
        self.buffer
            .get(offset)
            .copied()
            .ok_or_else(|| VmError::invalid_data("parser read_u8 out of bounds", offset))
    }

    pub fn read_i8(&self, offset: usize) -> VmResult<i8> {
        Ok(self.read_u8(offset)? as i8)
    }

    pub fn read_u16(&self, offset: usize) -> VmResult<u16> {
        let bytes = self.read_bytes(offset, 2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_i16(&self, offset: usize) -> VmResult<i16> {
        let bytes = self.read_bytes(offset, 2)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_u32(&self, offset: usize) -> VmResult<u32> {
        let bytes = self.read_bytes(offset, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_i32(&self, offset: usize) -> VmResult<i32> {
        let bytes = self.read_bytes(offset, 4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_f32(&self, offset: usize) -> VmResult<f32> {
        let bytes = self.read_bytes(offset, 4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_cstring(&self, offset: usize, len: usize) -> VmResult<String> {
        let bytes = self.read_bytes(offset, len)?;
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        decode_string(&bytes[..end], self.nls)
    }

    pub fn is_code_area(&self, addr: u32) -> bool {
        addr >= 4 && addr < self.sys_desc_offset
    }

    pub fn get_syscall(&self, id: u16) -> Option<&Syscall> {
        self.syscalls.get(id as usize)
    }

    pub fn get_title(&self) -> String {
        self.game_title.clone()
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
            _ => (640, 480),
        }
    }

    pub fn get_game_mode(&self) -> u8 {
        self.game_mode
    }

    pub fn get_game_mode_reserved(&self) -> u8 {
        self.game_mode_reserved
    }

    pub fn get_entry_point(&self) -> u32 {
        self.entry_point
    }

    pub fn get_sys_desc_offset(&self) -> u32 {
        self.sys_desc_offset
    }

    fn read_bytes(&self, offset: usize, len: usize) -> VmResult<&[u8]> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| VmError::invalid_data("parser offset overflow", offset))?;
        self.buffer
            .get(offset..end)
            .ok_or_else(|| VmError::invalid_data("parser read out of bounds", offset))
    }

    fn parse(&mut self) -> VmResult<()> {
        let mut off = 0usize;
        self.sys_desc_offset = self.read_u32(off)?;

        off = self.sys_desc_offset as usize;
        self.entry_point = self.read_u32(off)?;
        off += size_of::<u32>();

        self.non_volatile_global_count = self.read_u16(off)?;
        off += size_of::<u16>();

        self.volatile_global_count = self.read_u16(off)?;
        off += size_of::<u16>();

        self.game_mode = self.read_u8(off)?;
        off += size_of::<u8>();

        self.game_mode_reserved = self.read_u8(off)?;
        off += size_of::<u8>();

        let title_len = self.read_u8(off)?;
        off += size_of::<u8>();

        self.game_title = self.read_cstring(off, title_len as usize)?;
        off += title_len as usize;

        self.syscall_count = self.read_u16(off)?;
        off += size_of::<u16>();

        self.syscalls.clear();
        self.syscalls.reserve(self.syscall_count as usize);
        for _ in 0..self.syscall_count {
            let args = self.read_u8(off)?;
            off += size_of::<u8>();

            let name_len = self.read_u8(off)?;
            off += size_of::<u8>();

            let name = self.read_cstring(off, name_len as usize)?;
            off += name_len as usize;

            self.syscalls.push(Syscall { args, name });
        }

        self.custom_syscall_count = self.read_u16(off)?;
        Ok(())
    }
}

fn decode_string(bytes: &[u8], nls: Nls) -> VmResult<String> {
    match nls {
        Nls::Utf8 => core::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| VmError::invalid_data("invalid UTF-8 script string", 0)),
        Nls::ShiftJis | Nls::Gbk => Ok(bytes.iter().map(|b| *b as char).collect()),
    }
}
