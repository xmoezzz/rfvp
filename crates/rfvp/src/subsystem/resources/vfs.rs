//! The game's files: `.bin` packs plus loose files in the game directory.
//!
//! One implementation for every platform. A host hands over the game directory as an
//! [`RfvpStorage`] (and packs as [`PackSource`]s when it opens them itself); everything here is
//! the same whether that directory is a desktop folder, an SD card, a UEFI volume or files picked
//! in a browser.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use anyhow::{anyhow, bail, Result};

use crate::host_api::{RfvpError, RfvpFile, RfvpResult, RfvpStorage};
use crate::script::parser::Nls;
use crate::utils::stable_hash::StableHashMap;
// `std` is the crate's own shim in `no_std` builds.
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Mutex;

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

pub type VfsStream = Box<dyn ReadSeek + Send + Sync>;

#[derive(Debug, Clone)]
pub struct VfsEntry {
    pub offset: u64,
    pub size: u64,
}

/// Random access to the bytes of a pack. Packs are often over a gigabyte, so hosts hand the VFS
/// a file to read from on demand instead of loading the whole pack into memory.
pub trait PackSource: Send + Sync {
    fn len(&self) -> u64;

    /// Read up to `buf.len()` bytes starting at `offset`; returns 0 at the end of the pack.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;
}

/// A pack that is entirely in memory.
struct MemoryPack(Vec<u8>);

impl PackSource for MemoryPack {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let Ok(start) = usize::try_from(offset) else {
            return Ok(0);
        };
        if start >= self.0.len() {
            return Ok(0);
        }
        let n = buf.len().min(self.0.len() - start);
        buf[..n].copy_from_slice(&self.0[start..start + n]);
        Ok(n)
    }
}

/// A pack read through a host file handle.
pub struct HostPackFile<F: RfvpFile> {
    file: Mutex<F>,
    len: u64,
}

impl<F: RfvpFile> HostPackFile<F> {
    pub fn new(mut file: F) -> RfvpResult<Self> {
        let len = file.len()?;
        Ok(Self {
            file: Mutex::new(file),
            len,
        })
    }
}

// SAFETY: the handle is only touched while holding the mutex, so it is never used from two
// threads at once. Host file handles are plain OS handles that may be used from any thread.
unsafe impl<F: RfvpFile> Send for HostPackFile<F> {}
unsafe impl<F: RfvpFile> Sync for HostPackFile<F> {}

impl<F: RfvpFile> PackSource for HostPackFile<F> {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("pack file lock poisoned"))?;
        file.read_at(offset, buf)
            .map_err(|err| anyhow!("pack read failed at offset {offset}: {err:?}"))
    }
}

#[derive(Clone)]
struct PackHandle(Arc<dyn PackSource>);

impl fmt::Debug for PackHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PackHandle(len={})", self.0.len())
    }
}

/// `Read + Seek` over one entry of a pack, reading from the pack source as it goes.
struct PackSlice {
    source: Arc<dyn PackSource>,
    base: u64,
    len: u64,
    pos: u64,
}

impl Read for PackSlice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let want = buf.len().min((self.len - self.pos) as usize);
        let n = self
            .source
            .read_at(self.base + self.pos, &mut buf[..want])
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for PackSlice {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let len = self.len as i128;
        let next = match pos {
            SeekFrom::Start(pos) => pos as i128,
            SeekFrom::End(delta) => len + delta as i128,
            SeekFrom::Current(delta) => self.pos as i128 + delta as i128,
        }
        .clamp(0, len) as u64;
        self.pos = next;
        Ok(next)
    }
}

#[derive(Debug, Clone)]
pub struct VfsFile {
    pub path: PathBuf,
    pub folder_name: String,
    pub file_count: u64,
    pub filename_table_size: u64,
    #[cfg(not(feature = "psx"))]
    pub entries: StableHashMap<String, VfsEntry>,
    pub nls: Nls,
    pack: PackHandle,
    /// Entry names by their lowercase form. Scripts do not match the case of the names in the
    /// packs (`title_bg1_a` for `title_bg1_A`); the original engine, on Windows, ignores case.
    #[cfg(not(feature = "psx"))]
    lowercase: StableHashMap<String, String>,
    /// The PlayStation keeps no names: entries sorted by a hash of the lowercase name (12 bytes
    /// each instead of two strings and two map slots; a game has thousands).
    #[cfg(feature = "psx")]
    index: Vec<(u32, u32, u32)>,
}

/// FNV-1a of a name's lowercase form.
#[cfg(feature = "psx")]
fn name_hash(name: &str) -> u32 {
    let mut h = 0x811c_9dc5u32;
    for b in name.bytes() {
        h ^= b.to_ascii_lowercase() as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

impl VfsFile {
    pub fn from_pack_source(
        folder_name: String,
        source: Arc<dyn PackSource>,
        nls: Nls,
    ) -> Result<Self> {
        #[cfg(feature = "psx")]
        {
            let (file_count, filename_table_size, index) = Self::parse_index(source.as_ref(), nls)?;
            Ok(Self {
                path: PathBuf::from(format!("{folder_name}.bin").as_str()),
                folder_name,
                file_count,
                filename_table_size,
                nls,
                pack: PackHandle(source),
                index,
            })
        }
        #[cfg(not(feature = "psx"))]
        {
            let (file_count, filename_table_size, entries) =
                Self::parse_source(source.as_ref(), nls)?;
            let lowercase = entries
                .keys()
                .map(|name| (name.to_ascii_lowercase(), name.clone()))
                .collect();
            Ok(Self {
                path: PathBuf::from(format!("{folder_name}.bin").as_str()),
                folder_name,
                file_count,
                filename_table_size,
                entries,
                nls,
                pack: PackHandle(source),
                lowercase,
            })
        }
    }

    fn read_exact_at(source: &dyn PackSource, mut offset: u64, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            let n = source.read_at(offset, buf)?;
            if n == 0 {
                bail!("unexpected end of pack at offset {offset}");
            }
            offset += n as u64;
            buf = buf.split_at_mut(n).1;
        }
        Ok(())
    }

    fn read_filename_table(
        source: &dyn PackSource,
        offset: u64,
        size: u64,
        nls: Nls,
    ) -> Result<StableHashMap<u64, String>> {
        let size = usize::try_from(size).map_err(|_| anyhow!("filename table too large"))?;
        let mut buffer = Vec::new();
        buffer.resize(size, 0);
        Self::read_exact_at(source, offset, &mut buffer)?;

        let mut results = StableHashMap::default();
        let mut start = 0usize;
        for (i, &b) in buffer.iter().enumerate() {
            if b == 0 {
                let s = crate::text_codec::decode(nls.clone(), &buffer[start..i]).0;
                results.insert(start as u64, s);
                start = i + 1;
            }
        }
        Ok(results)
    }

    /// The PlayStation's index: each entry by the hash of its name, made straight from the
    /// name table (no strings kept).
    #[cfg(feature = "psx")]
    fn parse_index(source: &dyn PackSource, nls: Nls) -> Result<(u64, u64, Vec<(u32, u32, u32)>)> {
        let mut header = [0u8; 8];
        Self::read_exact_at(source, 0, &mut header)?;
        let count = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let names_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let mut table = alloc::vec![0u8; count * 12];
        Self::read_exact_at(source, 8, &mut table)?;
        let mut names = alloc::vec![0u8; names_len];
        Self::read_exact_at(source, 8 + count as u64 * 12, &mut names)?;
        let mut index = Vec::with_capacity(count);
        for chunk in table.chunks_exact(12) {
            let name_off = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as usize;
            let offset = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            let size = u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
            let Some(raw) = names.get(name_off..).and_then(|rest| rest.split(|&b| b == 0).next()) else {
                continue;
            };
            // ASCII names (nearly all) hash the same undecoded: no string per entry.
            let hash = match core::str::from_utf8(raw) {
                Ok(text) if raw.is_ascii() => name_hash(text),
                _ => name_hash(&crate::text_codec::decode(nls.clone(), raw).0),
            };
            index.push((hash, offset, size));
        }
        index.sort_unstable_by_key(|e| e.0);
        Ok((count as u64, names_len as u64, index))
    }

    #[cfg_attr(feature = "psx", allow(dead_code))]
    fn parse_source(
        source: &dyn PackSource,
        nls: Nls,
    ) -> Result<(u64, u64, StableHashMap<String, VfsEntry>)> {
        let mut header = [0u8; 8];
        Self::read_exact_at(source, 0, &mut header)?;
        let file_count = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let filename_table_size =
            u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;

        let entries_offset = header.len() as u64;
        let entries_len = file_count
            .checked_mul(12)
            .ok_or_else(|| anyhow!("pack entry table overflow"))?;
        let filename_table_offset = entries_offset
            .checked_add(entries_len)
            .ok_or_else(|| anyhow!("pack metadata overflow"))?;
        let filename_table =
            Self::read_filename_table(source, filename_table_offset, filename_table_size, nls)?;

        // Read the whole entry table in one go: a host read can be a system call.
        let entries_len =
            usize::try_from(entries_len).map_err(|_| anyhow!("pack entry table too large"))?;
        let mut table = Vec::new();
        table.resize(entries_len, 0);
        Self::read_exact_at(source, entries_offset, &mut table)?;

        let mut entries = StableHashMap::default();
        for chunk in table.chunks_exact(12) {
            let name_off = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as u64;
            let data_off = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as u64;
            let data_size = u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]) as u64;

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

    pub fn open_stream_with_len(&self, name: &str) -> Result<(VfsStream, Option<u64>)> {
        #[cfg(feature = "psx")]
        let found = {
            let h = name_hash(name);
            self.index
                .binary_search_by_key(&h, |e| e.0)
                .ok()
                .map(|i| VfsEntry { offset: self.index[i].1 as u64, size: self.index[i].2 as u64 })
        };
        #[cfg(not(feature = "psx"))]
        let found = self
            .entries
            .get(name)
            .or_else(|| {
                self.lowercase
                    .get(&name.to_ascii_lowercase())
                    .and_then(|real| self.entries.get(real))
            })
            .cloned();
        let ent = found
            .ok_or_else(|| anyhow!("file not found in host pack {}: {}", self.folder_name, name))?;
        let end = ent
            .offset
            .checked_add(ent.size)
            .ok_or_else(|| anyhow!("pack entry overflow"))?;
        if end > self.pack.0.len() {
            bail!(
                "pack entry out of range: {}/{} offset={} size={} pack_size={}",
                self.folder_name,
                name,
                ent.offset,
                ent.size,
                self.pack.0.len()
            );
        }
        Ok((
            Box::new(PackSlice {
                source: self.pack.0.clone(),
                base: ent.offset,
                len: ent.size,
                pos: 0,
            }),
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
}

/// The game directory, for assets shipped as loose files. Each directory is listed once, the
/// first time something in it is asked for: asking the file system about every asset the game
/// opens costs a system call each (and an emulator log line on a console).
struct LooseDir {
    storage: Arc<dyn RfvpStorage>,
    /// Lowercase directory -> (lowercase name -> name on disk).
    listings: Mutex<StableHashMap<String, StableHashMap<String, String>>>,
}

impl LooseDir {
    fn new(storage: Arc<dyn RfvpStorage>) -> Self {
        Self {
            storage,
            listings: Mutex::new(StableHashMap::default()),
        }
    }

    /// The path on disk of `path` (as the script wrote it, `/`-separated), if the game directory
    /// has it. Names are matched without regard to case, as the original engine on Windows does.
    fn find(&self, path: &str) -> Option<String> {
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        let mut listings = self.listings.lock().ok()?;
        let listing = listings.entry(dir.to_ascii_lowercase()).or_insert_with(|| {
            self.storage
                .list(dir)
                .unwrap_or_default()
                .into_iter()
                .map(|name| (name.to_ascii_lowercase(), name))
                .collect()
        });
        let real = listing.get(&name.to_ascii_lowercase())?;
        Some(if dir.is_empty() {
            real.clone()
        } else {
            format!("{dir}/{real}")
        })
    }
}

impl fmt::Debug for LooseDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LooseDir")
    }
}

#[derive(Debug)]
pub struct Vfs {
    pub files: StableHashMap<String, VfsFile>,
    loose_files: StableHashMap<String, Vec<u8>>,
    /// The game directory, for assets shipped as loose files.
    loose_dir: Option<LooseDir>,
    pub nls: Nls,
}

impl Default for Vfs {
    fn default() -> Self {
        Self {
            files: StableHashMap::default(),
            loose_files: StableHashMap::default(),
            loose_dir: None,
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

    /// Look for assets in the game directory too. A loose file wins over the same name in a pack,
    /// which is how patches and translations override a game's files.
    pub fn set_loose_dir(&mut self, dir: Option<Arc<dyn RfvpStorage>>) {
        self.loose_dir = dir.map(LooseDir::new);
    }

    /// A VFS over a game directory: its `.bin` packs (read on demand) and its loose files.
    pub fn from_dir(nls: Nls, dir: Arc<dyn RfvpStorage>) -> Result<Self> {
        let mut vfs = Self::new(nls)?;
        let names = dir
            .list("")
            .map_err(|err| anyhow!("list the game directory: {err:?}"))?;
        for name in names {
            let Some(folder) = name
                .len()
                .checked_sub(4)
                // Unrelated names may end in multibyte UTF-8 characters. Only slice the
                // stem after a valid ASCII extension proves this is a character boundary.
                .filter(|&stem| {
                    stem > 0
                        && name
                            .get(stem..)
                            .map(|ext| ext.eq_ignore_ascii_case(".bin"))
                            .unwrap_or(false)
                })
                .map(|stem| &name[..stem])
            else {
                continue;
            };
            let added = open_pack(dir.as_ref(), &name)
                .and_then(|source| vfs.add_pack_source(folder, source));
            if let Err(err) = added {
                log::warn!("skipping pack {name}: {err:#}");
            }
        }
        vfs.set_loose_dir(Some(dir));
        Ok(vfs)
    }

    pub fn add_loose_file(&mut self, path: &str, bytes: Vec<u8>) {
        self.loose_files.insert(normalize_vfs_key(path), bytes);
    }

    pub fn add_pack_bytes(&mut self, folder_name: &str, bytes: Vec<u8>) -> Result<()> {
        self.add_pack_source(folder_name, Arc::new(MemoryPack(bytes)))
    }

    /// Add a pack that is read from `source` on demand.
    pub fn add_pack_source(&mut self, folder_name: &str, source: Arc<dyn PackSource>) -> Result<()> {
        let folder = folder_name
            .strip_suffix(".bin")
            .unwrap_or(folder_name)
            .to_ascii_lowercase();
        let file = VfsFile::from_pack_source(folder.clone(), source, self.nls)?;
        self.files.insert(folder, file);
        Ok(())
    }

    pub fn open_stream_with_len(&self, path: &str) -> Result<(VfsStream, Option<u64>)> {
        let key = normalize_vfs_key(path);
        if key.starts_with("voice/") || key.starts_with("voice2/") {
            crate::perf::event(format_args!("voice resource path={path:?}"));
        }
        if let Some(bytes) = self.loose_files.get(&key) {
            return Ok((
                Box::new(Cursor::new(bytes.clone())),
                Some(bytes.len() as u64),
            ));
        }

        if let Some(dir) = &self.loose_dir {
            // The name as the script wrote it: the card may care about case.
            let on_disk = path.trim_start_matches("./").replace('\\', "/");
            let on_disk = on_disk.trim_start_matches('/');
            if let Some(real) = dir.find(on_disk) {
                return open_loose(dir.storage.as_ref(), &real);
            }
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
        let _timing = crate::perf::scope(crate::perf::VFS_READ);
        let started = crate::perf::start();
        let result = (|| {
            let (mut stream, len) = self.open_stream_with_len(path)?;
            let mut out = Vec::with_capacity(len.and_then(|n| usize::try_from(n).ok()).unwrap_or(0));
            stream.read_to_end(&mut out)?;
            Ok(out)
        })();
        if crate::perf::enabled() {
            let us = crate::perf::start().saturating_sub(started);
            match &result {
                Ok(bytes) if us >= 10_000 => crate::perf::event(format_args!("vfs path={path:?} bytes={} us={us}", bytes.len())),
                Err(err) => crate::perf::event(format_args!("vfs FAILED path={path:?}: {err:?}")),
                _ => {}
            }
        }
        result
    }
}

/// A file of `dir` as a pack source: streamed when the host can open files, read whole otherwise.
fn open_pack(dir: &dyn RfvpStorage, path: &str) -> Result<Arc<dyn PackSource>> {
    match dir.open(path) {
        Ok(file) => Ok(Arc::new(
            HostPackFile::new(file).map_err(|err| anyhow!("open {path}: {err:?}"))?,
        )),
        Err(RfvpError::Unsupported) => Ok(Arc::new(MemoryPack(
            dir.read(path).map_err(|err| anyhow!("read {path}: {err:?}"))?,
        ))),
        Err(err) => bail!("open {path}: {err:?}"),
    }
}

fn open_loose(dir: &dyn RfvpStorage, path: &str) -> Result<(VfsStream, Option<u64>)> {
    let source = open_pack(dir, path)?;
    let len = source.len();
    Ok((
        Box::new(PackSlice {
            source,
            base: 0,
            len,
            pos: 0,
        }),
        Some(len),
    ))
}

fn normalize_vfs_key(path: &str) -> String {
    path.trim_start_matches("./")
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    /// A game directory held in memory.
    struct MemoryDir(BTreeMap<String, Vec<u8>>);

    impl RfvpStorage for MemoryDir {
        fn read(&self, path: &str) -> RfvpResult<Vec<u8>> {
            self.0.get(path).cloned().ok_or(RfvpError::NotFound)
        }
        fn write(&self, _path: &str, _bytes: &[u8]) -> RfvpResult<()> {
            Err(RfvpError::Unsupported)
        }
        fn exists(&self, path: &str) -> bool {
            self.0.contains_key(path)
        }
        fn remove(&self, _path: &str) -> RfvpResult<()> {
            Err(RfvpError::Unsupported)
        }
        fn list(&self, dir: &str) -> RfvpResult<Vec<String>> {
            Ok(self
                .0
                .keys()
                .filter_map(|path| {
                    let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
                    (parent == dir).then(|| name.to_string())
                })
                .collect())
        }
    }

    fn dir(files: &[(&str, &[u8])]) -> Arc<dyn RfvpStorage> {
        Arc::new(MemoryDir(
            files.iter().map(|(name, bytes)| (name.to_string(), bytes.to_vec())).collect(),
        ))
    }

    /// A pack in the layout the engine reads: entry count, name table size, 12-byte entries,
    /// names, then the data.
    fn pack(name: &str, data: &[u8]) -> Vec<u8> {
        let names = [name.as_bytes(), &[0]].concat();
        let data_offset = 8 + 12 + names.len();
        let mut out = Vec::new();
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&(names.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // name offset
        out.extend_from_slice(&(data_offset as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&names);
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn directory_scan_skips_non_pack_names_without_slicing_unicode() {
        let bytes = pack("a", b"image");
        let vfs = Vfs::from_dir(
            Nls::ShiftJIS,
            dir(&[
                ("汉化", b""),
                ("font_中文", b""),
                ("📁", b""),
                ("汉", b""),
                ("", b""),
                (".bin", &bytes),
                ("notes.txt", b"notes"),
            ]),
        )
        .unwrap();
        assert!(vfs.files.is_empty());
        assert_eq!(vfs.read_file("notes.txt").unwrap(), b"notes");
    }

    #[test]
    fn directory_scan_opens_unicode_pack_names_and_mixed_case_extensions() {
        let bytes = pack("a", b"image");
        let vfs = Vfs::from_dir(
            Nls::ShiftJIS,
            dir(&[("图像.BiN", &bytes), ("graph.bin", &bytes)]),
        )
        .unwrap();
        assert_eq!(vfs.files.len(), 2);
        assert_eq!(vfs.read_file("图像/a").unwrap(), b"image");
        assert_eq!(vfs.read_file("graph/a").unwrap(), b"image");
    }

    #[test]
    fn a_folder_of_loose_files_is_found_without_a_pack() {
        let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
        vfs.set_loose_dir(Some(dir(&[("graph/BG001", b"image bytes")])));
        assert_eq!(vfs.read_file("graph/BG001").unwrap(), b"image bytes");
        // Backslashes and a leading ./ are how some scripts write it.
        assert_eq!(vfs.read_file("graph\\BG001").unwrap(), b"image bytes");
        assert!(vfs.read_file("graph/missing").is_err());
    }

    #[test]
    fn loose_files_are_found_whatever_the_case() {
        let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
        vfs.set_loose_dir(Some(dir(&[("graph/Title_BG", b"bg"), ("akaseka.hcb", b"script")])));
        assert_eq!(vfs.read_file("graph/title_bg").unwrap(), b"bg");
        assert_eq!(vfs.read_file("AKASEKA.HCB").unwrap(), b"script");
    }

    #[test]
    fn a_loose_file_overrides_the_same_name_in_a_pack() {
        let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
        vfs.add_pack_bytes("graph", pack("a", b"from the pack")).unwrap();
        assert_eq!(vfs.read_file("graph/a").unwrap(), b"from the pack");
        vfs.set_loose_dir(Some(dir(&[("graph/a", b"patched")])));
        assert_eq!(vfs.read_file("graph/a").unwrap(), b"patched");
    }

    #[test]
    fn packs_still_serve_what_the_directory_does_not_have() {
        let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
        vfs.add_pack_bytes("graph", pack("a", b"from the pack")).unwrap();
        vfs.set_loose_dir(Some(dir(&[("graph/other", b"loose")])));
        assert_eq!(vfs.read_file("graph/a").unwrap(), b"from the pack");
    }

    #[test]
    fn pack_entries_are_found_whatever_the_case() {
        let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
        vfs.add_pack_bytes("graph", pack("title_bg1_A", b"background")).unwrap();
        assert_eq!(vfs.read_file("graph/title_bg1_a").unwrap(), b"background");
        assert_eq!(vfs.read_file("GRAPH/Title_BG1_a").unwrap(), b"background");
    }

    #[test]
    fn a_pack_entry_is_read_from_the_source_on_demand() {
        let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
        vfs.add_pack_bytes("bgm", pack("song", b"0123456789")).unwrap();
        let (mut stream, len) = vfs.open_stream_with_len("bgm/song").unwrap();
        assert_eq!(len, Some(10));
        // Seeking within the entry works, and reads stop at its end.
        stream.seek(SeekFrom::Start(4)).unwrap();
        let mut rest = Vec::new();
        stream.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"456789");
    }
}
