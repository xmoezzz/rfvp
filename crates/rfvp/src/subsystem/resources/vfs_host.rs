use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::mem::size_of;

use anyhow::{anyhow, bail, Result};

use crate::script::parser::Nls;
use crate::utils::stable_hash::StableHashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

pub type VfsStream = Box<dyn ReadSeek + Send + Sync>;

#[derive(Debug, Clone)]
pub struct VfsEntry {
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct VfsFile {
    pub path: PathBuf,
    pub folder_name: String,
    pub file_count: u64,
    pub filename_table_size: u64,
    pub entries: StableHashMap<String, VfsEntry>,
    pub nls: Nls,
    pack_bytes: Vec<u8>,
    overrides: StableHashMap<String, Vec<u8>>,
}

impl VfsFile {
    pub fn from_pack_bytes(folder_name: String, pack_bytes: Vec<u8>, nls: Nls) -> Result<Self> {
        let mut cursor = Cursor::new(pack_bytes.clone());
        let (file_count, filename_table_size, entries) = Self::parse_reader(&mut cursor, nls)?;
        Ok(Self {
            path: PathBuf::from(format!("{folder_name}.bin").as_str()),
            folder_name,
            file_count,
            filename_table_size,
            entries,
            nls,
            pack_bytes,
            overrides: StableHashMap::default(),
        })
    }

    fn read_u32le(reader: &mut (impl Read + Seek), offset: u64) -> Result<u32> {
        let mut buffer = [0u8; 4];
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buffer)?;
        Ok(u32::from_le_bytes(buffer))
    }

    fn read_filename_table(
        reader: &mut (impl Read + Seek),
        offset: u64,
        size: u64,
        nls: Nls,
    ) -> Result<StableHashMap<u64, String>> {
        let size = usize::try_from(size).map_err(|_| anyhow!("filename table too large"))?;
        let mut buffer = Vec::new();
        buffer.resize(size, 0);
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buffer)?;

        let mut results = StableHashMap::default();
        let mut start = 0usize;
        for (i, &b) in buffer.iter().enumerate() {
            if b == 0 {
                let s = match nls {
                    Nls::ShiftJIS => {
                        let (s, _, _) = encoding_rs::SHIFT_JIS.decode(&buffer[start..i]);
                        s.to_string()
                    }
                    Nls::GBK => {
                        let (s, _, _) = encoding_rs::GBK.decode(&buffer[start..i]);
                        s.to_string()
                    }
                    Nls::UTF8 => String::from_utf8_lossy(&buffer[start..i]).into_owned(),
                };
                results.insert(start as u64, s);
                start = i + 1;
            }
        }
        Ok(results)
    }

    fn parse_reader(
        reader: &mut (impl Read + Seek),
        nls: Nls,
    ) -> Result<(u64, u64, StableHashMap<String, VfsEntry>)> {
        let mut offset = 0u64;
        let file_count = Self::read_u32le(reader, offset)? as u64;
        offset += size_of::<u32>() as u64;

        let filename_table_size = Self::read_u32le(reader, offset)? as u64;
        offset += size_of::<u32>() as u64;

        let entries_offset = offset;
        let filename_table_offset = entries_offset
            .checked_add(
                file_count
                    .checked_mul(12)
                    .ok_or_else(|| anyhow!("pack entry table overflow"))?,
            )
            .ok_or_else(|| anyhow!("pack metadata overflow"))?;
        let filename_table =
            Self::read_filename_table(reader, filename_table_offset, filename_table_size, nls)?;

        reader.seek(SeekFrom::Start(entries_offset))?;
        let mut entries = StableHashMap::default();
        let mut cur = entries_offset;
        for _ in 0..file_count {
            let name_off = Self::read_u32le(reader, cur)? as u64;
            cur += 4;
            let data_off = Self::read_u32le(reader, cur)? as u64;
            cur += 4;
            let data_size = Self::read_u32le(reader, cur)? as u64;
            cur += 4;

            if let Some(name) = filename_table.get(&name_off) {
                entries.insert(
                    name.clone(),
                    VfsEntry {
                        offset: data_off,
                        size: data_size,
                    },
                );
            }
        }

        Ok((file_count, filename_table_size, entries))
    }

    pub fn add_override(&mut self, name: &str, bytes: Vec<u8>) {
        self.overrides.insert(name.to_string(), bytes);
    }

    pub fn open_stream_with_len(&self, name: &str) -> Result<(VfsStream, Option<u64>)> {
        if let Some(bytes) = self.overrides.get(name) {
            return Ok((
                Box::new(Cursor::new(bytes.clone())),
                Some(bytes.len() as u64),
            ));
        }

        let ent = self
            .entries
            .get(name)
            .ok_or_else(|| anyhow!("file not found in host pack {}: {}", self.folder_name, name))?;
        let start = usize::try_from(ent.offset).map_err(|_| anyhow!("pack offset too large"))?;
        let size = usize::try_from(ent.size).map_err(|_| anyhow!("pack entry too large"))?;
        let end = start
            .checked_add(size)
            .ok_or_else(|| anyhow!("pack entry overflow"))?;
        if end > self.pack_bytes.len() {
            bail!(
                "pack entry out of range: {}/{} offset={} size={} pack_size={}",
                self.folder_name,
                name,
                ent.offset,
                ent.size,
                self.pack_bytes.len()
            );
        }
        Ok((
            Box::new(Cursor::new(self.pack_bytes[start..end].to_vec())),
            Some(ent.size),
        ))
    }

    pub fn open_stream(&self, name: &str) -> Result<VfsStream> {
        self.open_stream_with_len(name).map(|(stream, _)| stream)
    }

    pub fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let (mut stream, len) = self.open_stream_with_len(name)?;
        let mut out = Vec::with_capacity(len.and_then(|n| usize::try_from(n).ok()).unwrap_or(0));
        stream.read_to_end(&mut out)?;
        Ok(out)
    }

    pub fn save(&mut self, name: &str, content: Vec<u8>) -> Result<()> {
        self.overrides.insert(name.to_string(), content);
        Ok(())
    }
}

#[derive(Debug)]
pub struct Vfs {
    pub files: StableHashMap<String, VfsFile>,
    loose_files: StableHashMap<String, Vec<u8>>,
    pub nls: Nls,
}

impl Default for Vfs {
    fn default() -> Self {
        Self {
            files: StableHashMap::default(),
            loose_files: StableHashMap::default(),
            nls: Nls::ShiftJIS,
        }
    }
}

impl Vfs {
    pub fn new(nls: Nls) -> Result<Self> {
        Ok(Self {
            nls,
            ..Self::default()
        })
    }

    pub fn add_loose_file(&mut self, path: &str, bytes: Vec<u8>) {
        self.loose_files.insert(normalize_vfs_key(path), bytes);
    }

    pub fn add_pack_bytes(&mut self, folder_name: &str, bytes: Vec<u8>) -> Result<()> {
        let folder = folder_name
            .strip_suffix(".bin")
            .unwrap_or(folder_name)
            .to_ascii_lowercase();
        let file = VfsFile::from_pack_bytes(folder.clone(), bytes, self.nls)?;
        self.files.insert(folder, file);
        Ok(())
    }

    pub fn open_stream_with_len(&self, path: &str) -> Result<(VfsStream, Option<u64>)> {
        let key = normalize_vfs_key(path);
        if let Some(bytes) = self.loose_files.get(&key) {
            return Ok((
                Box::new(Cursor::new(bytes.clone())),
                Some(bytes.len() as u64),
            ));
        }

        let (folder, inner) = key
            .split_once('/')
            .ok_or_else(|| anyhow!("file not found: {}", path))?;
        let file = self
            .files
            .get(&folder.to_ascii_lowercase())
            .ok_or_else(|| anyhow!("pack not found for folder '{}'", folder))?;
        file.open_stream_with_len(inner)
    }

    pub fn open_stream(&self, path: &str) -> Result<VfsStream> {
        self.open_stream_with_len(path).map(|(stream, _)| stream)
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let (mut stream, len) = self.open_stream_with_len(path)?;
        let mut out = Vec::with_capacity(len.and_then(|n| usize::try_from(n).ok()).unwrap_or(0));
        stream.read_to_end(&mut out)?;
        Ok(out)
    }

    pub fn save(&mut self, path: &str, content: Vec<u8>) -> Result<()> {
        let key = normalize_vfs_key(path);
        if let Some((folder, inner)) = key.split_once('/') {
            if let Some(file) = self.files.get_mut(folder) {
                file.save(inner, content)?;
                return Ok(());
            }
        }
        self.loose_files.insert(key, content);
        Ok(())
    }

    pub fn find_ani(&self) -> Result<Vec<PathBuf>> {
        Ok(self
            .loose_files
            .keys()
            .filter(|key| {
                let lower = key.to_ascii_lowercase();
                lower.starts_with("cursor") && lower.ends_with(".ani")
            })
            .map(|key| PathBuf::from(key.as_str()))
            .collect())
    }
}

fn normalize_vfs_key(path: &str) -> String {
    path.trim_start_matches("./")
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}
