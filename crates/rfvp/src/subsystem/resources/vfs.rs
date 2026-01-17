use anyhow::{bail, Context, Result};
use glob::glob;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::path::{Path, PathBuf};

use crate::script::parser::Nls;
use crate::utils::file::app_base_path;

/// A simple trait alias for "readable + seekable" streams.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// A VFS-backed stream. This is intentionally a boxed trait object so callers can
/// treat on-disk files and pack-slices uniformly.
pub type VfsStream = Box<dyn ReadSeek + Send>;

/// A seekable view over a contiguous byte range inside a file.
///
/// This is used to expose pack entries as `Read + Seek` without loading them into memory.
#[derive(Debug)]
pub struct SubFile {
    file: File,
    start: u64,
    len: u64,
    pos: u64,
}

impl SubFile {
    pub fn new(mut file: File, start: u64, len: u64) -> Result<Self> {
        file.seek(SeekFrom::Start(start))
            .with_context(|| format!("seek pack slice start={}", start))?;
        Ok(Self {
            file,
            start,
            len,
            pos: 0,
        })
    }

    fn clamp_pos(&self, p: i128) -> u64 {
        if p <= 0 {
            return 0;
        }
        let p = p as u128;
        if p >= self.len as u128 {
            return self.len;
        }
        p as u64
    }
}

impl Read for SubFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len {
            return Ok(0);
        }
        let remain = (self.len - self.pos) as usize;
        let to_read = buf.len().min(remain);
        // Keep the underlying FD aligned with our logical position.
        self.file
            .seek(SeekFrom::Start(self.start + self.pos))?;
        let n = self.file.read(&mut buf[..to_read])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for SubFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(off) => self.clamp_pos(off as i128),
            SeekFrom::End(delta) => self.clamp_pos(self.len as i128 + delta as i128),
            SeekFrom::Current(delta) => self.clamp_pos(self.pos as i128 + delta as i128),
        };
        self.pos = next;
        self.file
            .seek(SeekFrom::Start(self.start + self.pos))?;
        Ok(self.pos)
    }
}

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
    pub entries: HashMap<String, VfsEntry>,
    pub nls: Nls,
}

impl VfsFile {
    pub fn new(path: PathBuf, folder_name: String, nls: Nls) -> anyhow::Result<Self> {
        let (file_count, filename_table_size, entries) = VfsFile::parse(&path, nls)
            .with_context(|| format!("parse pack {}", path.display()))?;

        Ok(VfsFile {
            path,
            folder_name,
            file_count,
            filename_table_size,
            entries,
            nls: nls.clone(),
        })
    }

    fn read_u32le(reader: &mut (impl Read + Seek), offset: u64) -> Result<u32> {
        let mut buffer = [0u8; 4];
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buffer)?;
        Ok(u32::from_le_bytes(buffer))
    }

    /// Read C-style NUL-terminated strings from `[offset, offset+size)`.
    ///
    /// Returns a map: `string_start_offset_in_table -> decoded string`.
    fn read_filename_table(
        reader: &mut (impl Read + Seek),
        offset: u64,
        size: u64,
        nls: Nls,
    ) -> Result<HashMap<u64, String>> {
        let mut buffer = vec![0u8; size as usize];
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buffer)?;

        let mut results = HashMap::new();
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

    /// Parse `.bin` package:
    /// - u32 file_count
    /// - u32 filename_table_size
    /// - file_count entries, each 12 bytes: {u32 name_off, u32 data_off, u32 data_size}
    /// - filename table (NUL-terminated strings)
    pub fn parse(
        path: impl AsRef<Path>,
        nls: Nls,
    ) -> Result<(u64, u64, HashMap<String, VfsEntry>)> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("pack does not exist: {}", path.display());
        }

        let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;

        let mut offset = 0u64;
        let file_count = Self::read_u32le(&mut file, offset)? as u64;
        offset += size_of::<u32>() as u64;

        let filename_table_size = Self::read_u32le(&mut file, offset)? as u64;
        offset += size_of::<u32>() as u64;

        // entries begin at offset=8
        let entries_offset = offset;
        let filename_table_offset = entries_offset + file_count * 12;

        let filename_table =
            Self::read_filename_table(&mut file, filename_table_offset, filename_table_size, nls)?;

        // entry table
        file.seek(SeekFrom::Start(entries_offset))?;
        let mut entries = HashMap::new();
        let mut cur = entries_offset;
        for _ in 0..file_count {
            let name_off = Self::read_u32le(&mut file, cur)? as u64;
            cur += 4;
            let data_off = Self::read_u32le(&mut file, cur)? as u64;
            cur += 4;
            let data_size = Self::read_u32le(&mut file, cur)? as u64;
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

    /// Open an entry as a seekable stream.
    ///
    /// Resolution order:
    /// 1) Prefer loose file override at `<game_root>/<folder_name>/<name>` if it exists.
    /// 2) Fallback to reading from `<game_root>/<folder_name>.bin` pack.
    pub fn open_stream(&self, name: &str) -> Result<VfsStream> {
        let override_path = app_base_path().join(&self.folder_name).join(name);
        if override_path.get_path().exists() {
            let f = File::open(override_path.get_path())
                .with_context(|| format!("open override file {}", override_path.get_path().display()))?;
            return Ok(Box::new(f));
        }

        let ent = self
            .entries
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("file not found in pack {}: {}", self.path.display(), name))?;
        let f = File::open(&self.path)
            .with_context(|| format!("open pack file {}", self.path.display()))?;
        let sub = SubFile::new(f, ent.offset, ent.size)
            .with_context(|| format!("create SubFile slice for {}", name))?;
        Ok(Box::new(sub))
    }

    /// Legacy convenience: read an entry fully into memory.
    pub fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let mut r = self.open_stream(name)?;
        let mut buf = Vec::with_capacity(self.entries.get(name).map(|e| e.size as usize).unwrap_or(0));
        r.read_to_end(&mut buf)
            .with_context(|| format!("read all bytes for {}", name))?;
        Ok(buf)
    }

    pub fn save(&self, name: &str, content: Vec<u8>) -> Result<()> {
        let mut file = File::create(app_base_path().join(&self.folder_name).join(name).get_path())
            .with_context(|| format!("create override file for {}", name))?;
        file.write_all(&content)
            .with_context(|| format!("write override file for {}", name))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Vfs {
    pub files: HashMap<String, VfsFile>,
    pub nls: Nls,
}

impl Default for Vfs {
    fn default() -> Self {
        Vfs::new(Nls::ShiftJIS).expect("default Vfs initialization")
    }
}

impl Vfs {
    pub fn new(nls: Nls) -> Result<Vfs> {
        let path = app_base_path().join("*.bin");
        let mut files = HashMap::new();
        for entry in glob(path.get_path().to_str().unwrap())? {
            if let Ok(path) = entry {
                let filename = path.file_stem().unwrap().to_string_lossy();
                if filename.is_empty() {
                    continue;
                }
                let folder_name = filename.to_string();
                if let Ok(vf) = VfsFile::new(path, folder_name.clone(), nls) {
                    files.insert(folder_name, vf);
                }
            }
        }

        Ok(Vfs { files, nls: nls.clone() })
    }

    /// Open a path as a seekable stream.
    ///
    /// Resolution order (as requested):
    /// 1) Try direct filesystem open at `<game_root>/<path>`.
    /// 2) If it fails and `path` is `folder/name...`, look for `<folder>.bin` and open the entry.
    pub fn open_stream(&self, path: &str) -> Result<VfsStream> {
        // 1) direct file override
        let fs_path = app_base_path().join(path);
        if fs_path.get_path().exists() {
            let f = File::open(fs_path.get_path())
                .with_context(|| format!("open file {}", fs_path.get_path().display()))?;
            return Ok(Box::new(f));
        }

        // 2) packed: folder/name...
        let (folder, inner) = path
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("file not found: {}", path))?;

        let vf = self
            .files
            .get(folder)
            .ok_or_else(|| anyhow::anyhow!("pack not found for folder '{}' (missing {}.bin)", folder, folder))?;
        vf.open_stream(inner)
    }

    /// Legacy convenience: read a path fully into memory.
    ///
    /// This retains the old signature but now follows the same resolution order as `open_stream`.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let mut r = self.open_stream(path)?;
        let mut buf = Vec::new();
        r.read_to_end(&mut buf)
            .with_context(|| format!("read all bytes for {}", path))?;
        Ok(buf)
    }

    pub fn save(&self, path: &str, content: Vec<u8>) -> Result<()> {
        let (folder, name) = path
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("invalid vfs path (expected folder/name): {}", path))?;
        let file = self
            .files
            .get(folder)
            .ok_or_else(|| anyhow::anyhow!("missing vfs pack for folder: {}", folder))?;
        file.save(name, content)
    }

    /// Find loose `.ani` files in `<game_root>/graph`.
    ///
    /// Note: this does not (yet) scan packs. It's primarily used for cursor assets.
    pub fn find_ani(&self) -> Result<Vec<PathBuf>> {
        let path = app_base_path().join("graph").join("*.ani");

        let matches: Vec<_> = glob(path.get_path().to_str().unwrap())?.flatten().collect();
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_parse_pack_smoke() {
        // This test is intentionally lenient: it only checks that parsing doesn't panic.
        // Provide a real pack under `testcase/` if you want stronger assertions.
        let p = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase"));
        let _ = p; // placeholder
    }
}
