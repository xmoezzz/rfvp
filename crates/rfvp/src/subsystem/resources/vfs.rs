use anyhow::{bail, Context, Result};
use glob::glob;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::path::{Path, PathBuf};

#[cfg(target_arch = "wasm32")]
use crate::wasm_app_path::{normalize_key as normalize_wasm_key, wasm_read_range, WasmAppPath, WasmFileRef, WasmRangeStream};

use crate::script::parser::Nls;
use crate::utils::file::app_base_path;

/// A simple trait alias for "readable + seekable" streams.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// A VFS-backed stream. This is intentionally a boxed trait object so callers can
/// treat on-disk files and pack-slices uniformly.
pub type VfsStream = Box<dyn ReadSeek + Send + Sync>;


/// A VFS stream that can be passed directly to Symphonia/Kira streaming audio.
pub struct VfsMediaSource {
    stream: VfsStream,
    byte_len: Option<u64>,
}

impl VfsMediaSource {
    pub fn new(stream: VfsStream, byte_len: Option<u64>) -> Self {
        Self { stream, byte_len }
    }
}

impl Read for VfsMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Seek for VfsMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.stream.seek(pos)
    }
}

impl symphonia_core::io::MediaSource for VfsMediaSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

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
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
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
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
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
    wasm_pack: Option<WasmFileRef>,
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
            wasm_pack: None,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new_wasm_pack(
        folder_name: String,
        path: String,
        file_ref: WasmFileRef,
        nls: Nls,
    ) -> anyhow::Result<Self> {
        let header = wasm_read_range(file_ref.id, 0, 8)
            .with_context(|| format!("read wasm pack header {path}"))?;
        let file_count = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let filename_table_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
        let metadata_len = 8u64
            .checked_add(file_count.checked_mul(12).ok_or_else(|| anyhow::anyhow!("pack entry table size overflow"))?)
            .and_then(|n| n.checked_add(filename_table_size))
            .ok_or_else(|| anyhow::anyhow!("pack metadata size overflow"))?;

        if metadata_len > file_ref.size {
            anyhow::bail!(
                "wasm pack metadata exceeds file size: path={} metadata_len={} file_size={}",
                path,
                metadata_len,
                file_ref.size
            );
        }

        let metadata_len_usize = usize::try_from(metadata_len)
            .map_err(|_| anyhow::anyhow!("wasm pack metadata too large: {metadata_len}"))?;
        let metadata = wasm_read_range(file_ref.id, 0, metadata_len_usize)
            .with_context(|| format!("read wasm pack metadata {path}"))?;
        let mut cursor = Cursor::new(metadata.as_slice());
        let (file_count, filename_table_size, entries) = VfsFile::parse_reader(&mut cursor, nls)
            .with_context(|| format!("parse wasm pack metadata {path}"))?;

        Ok(VfsFile {
            path: PathBuf::from(path),
            folder_name,
            file_count,
            filename_table_size,
            entries,
            nls: nls.clone(),
            wasm_pack: Some(file_ref),
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
    pub fn parse(path: impl AsRef<Path>, nls: Nls) -> Result<(u64, u64, HashMap<String, VfsEntry>)> {
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

        let entries_offset = offset;
        let filename_table_offset = entries_offset + file_count * 12;

        let filename_table =
            Self::read_filename_table(reader, filename_table_offset, filename_table_size, nls)?;

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

    /// Open an entry as a seekable stream and return its byte length when known.
    pub fn open_stream_with_len(&self, name: &str) -> Result<(VfsStream, Option<u64>)> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let override_path = app_base_path().join(&self.folder_name).join(name);
            if override_path.get_path().exists() {
                let f = File::open(override_path.get_path())
                    .with_context(|| format!("open override file {}", override_path.get_path().display()))?;
                let len = f.metadata().ok().map(|m| m.len());
                return Ok((Box::new(f), len));
            }
        }

        let ent = self
            .entries
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("file not found in pack {}: {}", self.path.display(), name))?;

        #[cfg(target_arch = "wasm32")]
        {
            let Some(file_ref) = self.wasm_pack.as_ref() else {
                anyhow::bail!("wasm pack source is missing for {}", self.path.display());
            };
            let sub = WasmRangeStream::new(file_ref, ent.offset, ent.size)
                .with_context(|| format!("create wasm pack slice for {}", name))?;
            return Ok((Box::new(sub), Some(ent.size)));
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let f = File::open(&self.path)
                .with_context(|| format!("open pack file {}", self.path.display()))?;
            let sub = SubFile::new(f, ent.offset, ent.size)
                .with_context(|| format!("create SubFile slice for {}", name))?;
            Ok((Box::new(sub), Some(ent.size)))
        }
    }

    /// Open an entry as a seekable stream.
    pub fn open_stream(&self, name: &str) -> Result<VfsStream> {
        self.open_stream_with_len(name).map(|(stream, _)| stream)
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
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (name, content);
            anyhow::bail!("saving VFS override files is not supported in wasm");
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut file = File::create(app_base_path().join(&self.folder_name).join(name).get_path())
                .with_context(|| format!("create override file for {}", name))?;
            file.write_all(&content)
                .with_context(|| format!("write override file for {}", name))?;
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct Vfs {
    pub files: HashMap<String, VfsFile>,
    pub nls: Nls,
    #[cfg(target_arch = "wasm32")]
    wasm_app_path: Option<WasmAppPath>,
}

impl Default for Vfs {
    fn default() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            return Vfs {
                files: HashMap::new(),
                nls: Nls::ShiftJIS,
                wasm_app_path: None,
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
            wasm_app_path: None,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn from_wasm_app_path(nls: Nls, app_path: WasmAppPath) -> Result<Vfs> {
        let mut files = HashMap::new();
        for (path, file_ref) in app_path.root_bin_files() {
            let folder_name = path
                .strip_suffix(".bin")
                .unwrap_or(&path)
                .to_ascii_lowercase();
            if folder_name.is_empty() {
                continue;
            }
            match VfsFile::new_wasm_pack(folder_name.clone(), path.clone(), file_ref, nls) {
                Ok(vf) => {
                    files.insert(folder_name, vf);
                }
                Err(e) => {
                    log::warn!("failed to parse wasm pack {}: {:#}", path, e);
                }
            }
        }

        Ok(Vfs {
            files,
            nls: nls.clone(),
            wasm_app_path: Some(app_path),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn first_hcb_bytes(&self) -> Result<Vec<u8>> {
        let Some(app_path) = self.wasm_app_path.as_ref() else {
            anyhow::bail!("wasm app path is not initialized");
        };
        app_path.first_root_hcb_bytes()
    }

    /// Open a path as a seekable stream and return its byte length when known.
    pub fn open_stream_with_len(&self, path: &str) -> Result<(VfsStream, Option<u64>)> {
        #[cfg(target_arch = "wasm32")]
        {
            let key = normalize_vfs_key(path);
            if let Some(app_path) = self.wasm_app_path.as_ref() {
                if let Some(file_ref) = app_path.lookup(&key) {
                    let stream = WasmRangeStream::new(file_ref, 0, file_ref.size)
                        .with_context(|| format!("open wasm loose file {}", key))?;
                    return Ok((Box::new(stream), Some(file_ref.size)));
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let fs_path = app_base_path().join(path);
            if fs_path.get_path().exists() {
                let f = File::open(fs_path.get_path())
                    .with_context(|| format!("open file {}", fs_path.get_path().display()))?;
                let len = f.metadata().ok().map(|m| m.len());
                return Ok((Box::new(f), len));
            }
        }

        let (folder, inner) = path
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("file not found: {}", path))?;

        let vf = self
            .files
            .get(&folder.to_ascii_lowercase())
            .ok_or_else(|| {
                #[cfg(target_arch = "wasm32")]
                {
                    if let Some(app_path) = self.wasm_app_path.as_ref() {
                        return anyhow::anyhow!(
                            "pack not found for folder '{}' (missing {}.bin); wasm root sample: {:?}",
                            folder,
                            folder,
                            app_path.known_root_files_sample()
                        );
                    }
                }
                anyhow::anyhow!("pack not found for folder '{}' (missing {}.bin)", folder, folder)
            })?;
        vf.open_stream_with_len(inner)
    }

    /// Open a path as a seekable stream.
    pub fn open_stream(&self, path: &str) -> Result<VfsStream> {
        self.open_stream_with_len(path).map(|(stream, _)| stream)
    }

    /// Open a path as a Symphonia-compatible media source for streaming audio.
    pub fn open_media_source(&self, path: &str) -> Result<VfsMediaSource> {
        let (stream, byte_len) = self.open_stream_with_len(path)?;
        Ok(VfsMediaSource::new(stream, byte_len))
    }

    /// Legacy convenience: read a path fully into memory.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let (mut r, byte_len) = self.open_stream_with_len(path)?;
        let mut buf = Vec::with_capacity(
            byte_len
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0),
        );
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
    normalize_wasm_key(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_parse_pack_smoke() {
        let p = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase"));
        let _ = p;
    }
}
