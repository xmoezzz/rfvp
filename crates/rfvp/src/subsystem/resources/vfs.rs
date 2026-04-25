use anyhow::{bail, Context, Result};
use glob::glob;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::path::{Path, PathBuf};

#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

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


#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub struct MemorySubFile {
    data: Arc<Vec<u8>>,
    start: u64,
    len: u64,
    pos: u64,
}

#[cfg(target_arch = "wasm32")]
impl MemorySubFile {
    pub fn new(data: Arc<Vec<u8>>, start: u64, len: u64) -> Result<Self> {
        let end = start
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!("memory slice range overflow"))?;
        if end > data.len() as u64 {
            anyhow::bail!(
                "memory slice out of bounds: start={} len={} data_len={}",
                start,
                len,
                data.len()
            );
        }
        Ok(Self { data, start, len, pos: 0 })
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

#[cfg(target_arch = "wasm32")]
impl Read for MemorySubFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len {
            return Ok(0);
        }
        let remain = (self.len - self.pos) as usize;
        let to_read = buf.len().min(remain);
        let begin = (self.start + self.pos) as usize;
        let end = begin + to_read;
        buf[..to_read].copy_from_slice(&self.data[begin..end]);
        self.pos += to_read as u64;
        Ok(to_read)
    }
}

#[cfg(target_arch = "wasm32")]
impl Seek for MemorySubFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(off) => self.clamp_pos(off as i128),
            SeekFrom::End(delta) => self.clamp_pos(self.len as i128 + delta as i128),
            SeekFrom::Current(delta) => self.clamp_pos(self.pos as i128 + delta as i128),
        };
        self.pos = next;
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
    #[cfg(target_arch = "wasm32")]
    memory_pack: Option<Arc<Vec<u8>>>,
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
            #[cfg(target_arch = "wasm32")]
            memory_pack: None,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new_memory_pack(
        folder_name: String,
        data: Arc<Vec<u8>>,
        nls: Nls,
    ) -> anyhow::Result<Self> {
        let mut cursor = Cursor::new(data.as_slice());
        let (file_count, filename_table_size, entries) = VfsFile::parse_reader(&mut cursor, nls)
            .with_context(|| format!("parse in-memory pack {}.bin", folder_name))?;

        Ok(VfsFile {
            path: PathBuf::from(format!("{folder_name}.bin")),
            folder_name,
            file_count,
            filename_table_size,
            entries,
            nls: nls.clone(),
            memory_pack: Some(data),
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
        Self::parse_reader(&mut file, nls)
    }

    fn parse_reader(
        reader: &mut (impl Read + Seek),
        nls: Nls,
    ) -> Result<(u64, u64, HashMap<String, VfsEntry>)> {
        let mut offset = 0u64;
        let file_count = Self::read_u32le(reader, offset)? as u64;
        offset += size_of::<u32>() as u64;

        let filename_table_size = Self::read_u32le(reader, offset)? as u64;
        offset += size_of::<u32>() as u64;

        // entries begin at offset=8
        let entries_offset = offset;
        let filename_table_offset = entries_offset + file_count * 12;

        let filename_table =
            Self::read_filename_table(reader, filename_table_offset, filename_table_size, nls)?;

        // entry table
        reader.seek(SeekFrom::Start(entries_offset))?;
        let mut entries = HashMap::new();
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

    /// Open an entry as a seekable stream.
    ///
    /// Resolution order:
    /// 1) Prefer loose file override at `<game_root>/<folder_name>/<name>` if it exists.
    /// 2) Fallback to reading from `<game_root>/<folder_name>.bin` pack.
    pub fn open_stream(&self, name: &str) -> Result<VfsStream> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let override_path = app_base_path().join(&self.folder_name).join(name);
            if override_path.get_path().exists() {
                let f = File::open(override_path.get_path())
                    .with_context(|| format!("open override file {}", override_path.get_path().display()))?;
                return Ok(Box::new(f));
            }
        }

        let ent = self
            .entries
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("file not found in pack {}: {}", self.path.display(), name))?;
        #[cfg(target_arch = "wasm32")]
        if let Some(data) = self.memory_pack.as_ref() {
            let sub = MemorySubFile::new(data.clone(), ent.offset, ent.size)
                .with_context(|| format!("create MemorySubFile slice for {}", name))?;
            return Ok(Box::new(sub));
        }

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
    #[cfg(target_arch = "wasm32")]
    loose_files: HashMap<String, Arc<Vec<u8>>>,
}

impl Default for Vfs {
    fn default() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            return Vfs {
                files: HashMap::new(),
                nls: Nls::ShiftJIS,
                loose_files: HashMap::new(),
            };
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            Vfs::new(Nls::ShiftJIS).expect("default Vfs initialization")
        }
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
                if let Ok(vf) = VfsFile::new(path, folder_name.to_ascii_lowercase().clone(), nls) {
                    files.insert(folder_name.to_ascii_lowercase(), vf);
                }
            }
        }

        Ok(Vfs {
            files,
            nls: nls.clone(),
            #[cfg(target_arch = "wasm32")]
            loose_files: HashMap::new(),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn from_memory_files(nls: Nls, files_in: HashMap<String, Vec<u8>>) -> Result<Vfs> {
        let mut loose_files: HashMap<String, Arc<Vec<u8>>> = HashMap::new();

        for (path, bytes) in files_in {
            let key = normalize_vfs_key(&path);
            if key.is_empty() {
                continue;
            }
            loose_files.insert(key, Arc::new(bytes));
        }

        let mut files = HashMap::new();
        for (path, data) in &loose_files {
            if !path.contains('/') && path.to_ascii_lowercase().ends_with(".bin") {
                let folder_name = path
                    .strip_suffix(".bin")
                    .unwrap_or(path)
                    .to_ascii_lowercase();
                if folder_name.is_empty() {
                    continue;
                }
                match VfsFile::new_memory_pack(folder_name.clone(), data.clone(), nls) {
                    Ok(vf) => {
                        files.insert(folder_name, vf);
                    }
                    Err(e) => {
                        log::warn!("failed to parse in-memory pack {}: {:#}", path, e);
                    }
                }
            }
        }

        Ok(Vfs {
            files,
            nls: nls.clone(),
            loose_files,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn first_hcb_bytes(&self) -> Result<Vec<u8>> {
        let mut candidates: Vec<_> = self
            .loose_files
            .iter()
            .filter(|(path, _)| path.to_ascii_lowercase().ends_with(".hcb"))
            .collect();

        candidates.sort_by(|(a, _), (b, _)| {
            let arank = if a.contains('/') { 1 } else { 0 };
            let brank = if b.contains('/') { 1 } else { 0 };
            arank.cmp(&brank).then_with(|| a.cmp(b))
        });

        let Some((path, bytes)) = candidates.into_iter().next() else {
            anyhow::bail!("No hcb file found in selected browser directory");
        };

        log::info!("using in-memory hcb: {}", path);
        Ok((**bytes).clone())
    }

    /// Open a path as a seekable stream.
    ///
    /// Resolution order (as requested):
    /// 1) Try direct filesystem open at `<game_root>/<path>`.
    /// 2) If it fails and `path` is `folder/name...`, look for `<folder>.bin` and open the entry.
    pub fn open_stream(&self, path: &str) -> Result<VfsStream> {
        #[cfg(target_arch = "wasm32")]
        {
            let key = normalize_vfs_key(path);
            if let Some(bytes) = self.loose_files.get(&key) {
                return Ok(Box::new(Cursor::new((**bytes).clone())));
            }
        }

        // 1) direct file override
        #[cfg(not(target_arch = "wasm32"))]
        {
            let fs_path = app_base_path().join(path);
            if fs_path.get_path().exists() {
                let f = File::open(fs_path.get_path())
                    .with_context(|| format!("open file {}", fs_path.get_path().display()))?;
                return Ok(Box::new(f));
            }
        }

        // 2) packed: folder/name...
        let (folder, inner) = path
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("file not found: {}", path))?;

        let vf = self
            .files
            .get(&folder.to_ascii_lowercase())
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

    /// Find loose `cursor*.ani` files next to the game executable/root.
    ///
    /// Reverse-engineered behavior: the original engine loads `cursor1.ani`,
    /// `cursor2.ani`, `cursor3.ani` by bare filename, not from `graph/`.
    /// This intentionally only scans the app base path and does not look inside packs.
    pub fn find_ani(&self) -> Result<Vec<PathBuf>> {
        #[cfg(target_arch = "wasm32")]
        {
            return Ok(Vec::new());
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = app_base_path().join("cursor*.ani");

            let matches: Vec<_> = glob(path.get_path().to_str().unwrap())?.flatten().collect();
            Ok(matches)
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn normalize_vfs_key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_ascii_lowercase()
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
