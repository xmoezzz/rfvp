use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct LuaxConfig {
    pub nls: Option<Nls>,
    pub custom_syscall_count: Option<u16>,
    pub game_mode: Option<u8>,
    pub game_mode_reserved: Option<u8>,
    pub game_title: Option<String>,
    pub syscall_count: Option<u16>,
    pub globals: Vec<ConfigGlobal>,
    pub entry: Option<String>,
    pub syscalls: Vec<ConfigSyscall>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nls {
    ShiftJIS,
    Gbk,
    Utf8,
}

impl Default for Nls {
    fn default() -> Self {
        Self::ShiftJIS
    }
}

impl<'de> Deserialize<'de> for Nls {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        match raw.as_str() {
            "ShiftJIS" | "SJIS" | "sjis" => Ok(Self::ShiftJIS),
            "GBK" | "gbk" => Ok(Self::Gbk),
            "UTF8" | "utf8" | "UTF-8" | "utf-8" => Ok(Self::Utf8),
            _ => Err(serde::de::Error::custom(format!("unsupported nls '{}': expected ShiftJIS, GBK, or UTF8", raw))),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConfigGlobal {
    pub name: String,
    pub volatile: bool,
    pub doc: String,
    pub ty: String,
}

#[derive(Debug, Clone)]
pub struct ConfigSyscall {
    pub id: u32,
    pub name: String,
    pub doc: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawYamlSyscall {
    pub args: u16,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawYamlProject {
    pub nls: Nls,
    pub custom_syscall_count: u16,
    pub game_mode: u8,
    pub game_mode_reserved: u8,
    pub game_title: String,
    pub syscall_count: u16,
    pub syscalls: BTreeMap<u32, RawYamlSyscall>,
}

impl LuaxConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read project yaml: {}", path.display()))?;
        let raw = serde_yaml::from_str::<RawYamlProject>(&text)
            .with_context(|| format!("failed to parse project yaml: {}", path.display()))?;

        if raw.game_title.trim().is_empty() {
            return Err(anyhow!("project yaml field 'game_title' must not be empty"));
        }
        if raw.syscall_count as usize != raw.syscalls.len() {
            return Err(anyhow!(
                "project yaml field 'syscall_count' is {}, but 'syscalls' contains {} entries",
                raw.syscall_count,
                raw.syscalls.len()
            ));
        }

        for expected_id in 0..raw.syscall_count as u32 {
            if !raw.syscalls.contains_key(&expected_id) {
                return Err(anyhow!(
                    "project yaml 'syscalls' is missing required entry {}",
                    expected_id
                ));
            }
        }

        let mut syscalls = Vec::with_capacity(raw.syscalls.len());
        for (id, syscall) in raw.syscalls {
            if syscall.name.trim().is_empty() {
                return Err(anyhow!("project yaml syscall {} has empty name", id));
            }
            let params = (0..syscall.args)
                .map(|index| format!("arg{}", index + 1))
                .collect::<Vec<_>>();
            let detail = if params.is_empty() {
                format!("syscall #{}", id)
            } else {
                format!("syscall #{}({})", id, params.join(", "))
            };
            syscalls.push(ConfigSyscall {
                id,
                name: syscall.name,
                doc: detail,
                params,
            });
        }
        syscalls.sort_by_key(|syscall| syscall.id);

        Ok(Self {
            nls: Some(raw.nls),
            custom_syscall_count: Some(raw.custom_syscall_count),
            game_mode: Some(raw.game_mode),
            game_mode_reserved: Some(raw.game_mode_reserved),
            game_title: Some(raw.game_title),
            syscall_count: Some(raw.syscall_count),
            globals: Vec::new(),
            entry: None,
            syscalls,
        })
    }

    pub fn global_map(&self) -> HashMap<String, ConfigGlobal> {
        self.globals
            .iter()
            .cloned()
            .map(|g| (g.name.clone(), g))
            .collect()
    }

    pub fn syscall_map(&self) -> HashMap<String, ConfigSyscall> {
        self.syscalls
            .iter()
            .cloned()
            .map(|s| (s.name.clone(), s))
            .collect()
    }
}
