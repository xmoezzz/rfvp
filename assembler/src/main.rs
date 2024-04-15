use anyhow::{bail, Result};
use clap::Parser as ClapParser;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod inst;

#[derive(Debug, Serialize, Deserialize)]
pub struct Function {
    address: u32,
    args_count: u8,
    locals_count: u8,
    insts: Vec<Inst>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Inst {
    address: u32,
    mnemonic: String,
    operands: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FVPProject {
    config_file: PathBuf,
    disassembly_file: PathBuf,
}

impl FVPProject {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let config_file = PathBuf::from(path.as_ref());
        let config_str = std::fs::read_to_string(config_file)?;
        let config: FVPProject = toml::from_str(&config_str)?;
        Ok(config)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyscallEntry {
    id: u32,
    name: String,
    args_count: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    entry_point: u32,
    non_volatile_global_count: u16,
    volatile_global_count: u16,
    game_mode: u16,
    game_title: String,
    syscalls: Vec<SyscallEntry>,
    custom_syscall_count: u16,
}

impl ProjectConfig {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let config_file = PathBuf::from(path.as_ref());
        let config_str = std::fs::read_to_string(config_file)?;
        let config: ProjectConfig = serde_yaml::from_str(&config_str)?;
        Ok(config)
    }

    pub fn put_u8(&self, value: u8, buffer: &mut Vec<u8>) {
        buffer.push(value);
    }

    pub fn put_u16_le(&self, value: u16, buffer: &mut Vec<u8>) {
        buffer.push((value & 0xff) as u8);
        buffer.push(((value >> 8) & 0xff) as u8);
    }

    pub fn put_u32_le(&self, value: u32, buffer: &mut Vec<u8>) {
        buffer.push((value & 0xff) as u8);
        buffer.push(((value >> 8) & 0xff) as u8);
        buffer.push(((value >> 16) & 0xff) as u8);
        buffer.push(((value >> 24) & 0xff) as u8);
    }

    pub fn serialize_to_binary(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        self.put_u32_le(self.entry_point, &mut data);

        Ok(data)
    }
}

pub struct Assembler {
    project: FVPProject,
    config: ProjectConfig,
    functions: Vec<Function>,
}

fn main() {
    println!("Hello, world!");
}
