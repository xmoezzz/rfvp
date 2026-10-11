//! Files on the disc, by the game's own names (main.c maps them to the disc's through
//! FILES.MAP and reads them a sector cache at a time).
//!
//! Saves are kept in RAM for now (lost when the console is turned off); the memory card comes
//! later.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::c_char;

use rfvp::host_api::{RfvpError, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpResult, RfvpStorage};

use crate::ffi::{self, c_path};

/// A file of the disc.
pub struct Ps1File {
    handle: i32,
    size: u32,
}

impl Ps1File {
    pub fn open(path: &str) -> RfvpResult<Self> {
        let c = c_path(path).ok_or(RfvpError::InvalidArgument)?;
        let mut size = 0u32;
        let handle = unsafe { ffi::rfvp_ps1_file_find(c.as_ptr() as *const c_char, &mut size) };
        if handle < 0 {
            crate::ffi::log(&alloc::format!("rfvp: no file {path}\n"));
            return Err(RfvpError::NotFound);
        }
        crate::ffi::log(&alloc::format!("rfvp: [{}] [heap {}] open {path} ({size} bytes)\n", crate::ffi::ticks_us() / 1000, unsafe { crate::ffi::HEAP_IN_USE } / 1024));
        Ok(Self { handle, size })
    }
}

impl RfvpFile for Ps1File {
    fn len(&mut self) -> RfvpResult<u64> {
        Ok(self.size as u64)
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        if offset >= self.size as u64 {
            return Ok(0);
        }
        let n = unsafe { ffi::rfvp_ps1_file_read(self.handle, offset as u32, buf.as_mut_ptr(), buf.len() as u32) };
        if n < 0 {
            Err(RfvpError::Io)
        } else {
            Ok(n as usize)
        }
    }
}

/// A file kept in RAM (a save).
struct MemFile {
    bytes: Arc<Vec<u8>>,
}

impl RfvpFile for MemFile {
    fn len(&mut self) -> RfvpResult<u64> {
        Ok(self.bytes.len() as u64)
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        let start = (offset as usize).min(self.bytes.len());
        let n = buf.len().min(self.bytes.len() - start);
        buf[..n].copy_from_slice(&self.bytes[start..start + n]);
        Ok(n)
    }
}

/// The game's files on the disc; writes (saves) go to RAM.
#[derive(Default)]
pub struct Ps1Storage {
    saves: spin::Mutex<BTreeMap<String, Arc<Vec<u8>>>>,
}

fn normalize(path: &str) -> String {
    let path = path.trim_start_matches("./").trim_start_matches('/');
    let path = if path == "." { "" } else { path };
    path.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase()
}

impl RfvpStorage for Ps1Storage {
    fn read(&self, path: &str) -> RfvpResult<Vec<u8>> {
        self.read_prefix(path, usize::MAX)
    }

    fn read_prefix(&self, path: &str, max_len: usize) -> RfvpResult<Vec<u8>> {
        if let Some(bytes) = self.saves.lock().get(&normalize(path)) {
            return Ok(bytes[..bytes.len().min(max_len)].to_vec());
        }
        let mut file = Ps1File::open(path)?;
        let len = (file.size as usize).min(max_len);
        let mut out = alloc::vec![0u8; len];
        file.read_exact_at(0, &mut out)?;
        Ok(out)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        self.saves.lock().insert(normalize(path), Arc::new(bytes.to_vec()));
        Ok(())
    }

    fn exists(&self, path: &str) -> bool {
        self.saves.lock().contains_key(&normalize(path)) || Ps1File::open(path).is_ok()
    }

    fn remove(&self, path: &str) -> RfvpResult<()> {
        self.saves.lock().remove(&normalize(path)).map(|_| ()).ok_or(RfvpError::NotFound)
    }

    fn list(&self, dir: &str) -> RfvpResult<Vec<String>> {
        let prefix = normalize(dir);
        let prefix = if prefix.is_empty() { prefix } else { prefix + "/" };
        let mut out: Vec<String> = self
            .saves
            .lock()
            .keys()
            .filter_map(|k| k.strip_prefix(prefix.as_str()).filter(|rest| !rest.contains('/')).map(String::from))
            .collect();
        for name in disc_names() {
            if let Some(rest) = name.strip_prefix(prefix.as_str()) {
                if !rest.contains('/') {
                    out.push(String::from(rest));
                }
            }
        }
        Ok(out)
    }

    fn open(&self, path: &str) -> RfvpResult<alloc::boxed::Box<dyn RfvpFile + Send>> {
        if let Some(bytes) = self.saves.lock().get(&normalize(path)) {
            return Ok(alloc::boxed::Box::new(MemFile { bytes: bytes.clone() }));
        }
        Ok(alloc::boxed::Box::new(Ps1File::open(path)?))
    }
}

// SAFETY: the engine runs on the one CPU thread there is.
unsafe impl Send for Ps1File {}

/// The game's names of the disc's files (FILES.MAP).
fn disc_names() -> Vec<String> {
    let count = unsafe { ffi::rfvp_ps1_file_count() };
    (0..count)
        .filter_map(|i| {
            let name = unsafe { core::ffi::CStr::from_ptr(ffi::rfvp_ps1_file_name(i)) };
            name.to_str().ok().map(String::from)
        })
        .collect()
}

pub struct Ps1FileSystem {
    storage: Arc<Ps1Storage>,
}

impl Ps1FileSystem {
    pub fn new() -> Self {
        Self { storage: Arc::new(Ps1Storage::default()) }
    }

    pub fn exists(&mut self, path: &str) -> bool {
        self.storage.exists(path)
    }
}

impl RfvpFileSystem for Ps1FileSystem {
    type File = Ps1File;

    fn storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        Some(self.storage.clone())
    }

    fn game_files(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        Some(self.storage.clone())
    }

    fn save_storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        Some(self.storage.clone())
    }

    fn open(&mut self, path: &str) -> RfvpResult<Self::File> {
        Ps1File::open(path)
    }

    fn write_all(&mut self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        self.storage.write(path, bytes)
    }

    fn metadata(&mut self, path: &str) -> RfvpResult<RfvpFileInfo> {
        let mut file = Ps1File::open(path)?;
        Ok(RfvpFileInfo::file(file.len()?))
    }

    fn enumerate_by_extension(
        &mut self,
        root: &str,
        extension_without_dot: &str,
        visitor: &mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
    ) -> RfvpResult<()> {
        let prefix = normalize(root);
        for name in disc_names() {
            let rest = if prefix.is_empty() { Some(name.as_str()) } else { name.strip_prefix(&(prefix.clone() + "/")) };
            let Some(rest) = rest else { continue };
            if rest.contains('/') {
                continue;
            }
            if rest.rsplit_once('.').is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(extension_without_dot)) {
                let mut file = Ps1File::open(&name)?;
                visitor(&name, RfvpFileInfo { len: file.len()?, kind: RfvpFileKind::File })?;
            }
        }
        Ok(())
    }
}
