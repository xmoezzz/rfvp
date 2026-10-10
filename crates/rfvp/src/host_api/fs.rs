use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use super::error::{RfvpError, RfvpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfvpFileKind {
    File,
    Directory,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfvpFileInfo {
    pub len: u64,
    pub kind: RfvpFileKind,
}

impl RfvpFileInfo {
    pub const fn file(len: u64) -> Self {
        Self {
            len,
            kind: RfvpFileKind::File,
        }
    }
}

pub trait RfvpFile {
    fn len(&mut self) -> RfvpResult<u64>;

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize>;

    fn read_exact_at(&mut self, mut offset: u64, mut buf: &mut [u8]) -> RfvpResult<()> {
        while !buf.is_empty() {
            let read = self.read_at(offset, buf)?;
            if read == 0 {
                return Err(RfvpError::EndOfFile);
            }
            offset = offset
                .checked_add(read as u64)
                .ok_or(RfvpError::InvalidArgument)?;
            let (_, rest) = buf.split_at_mut(read);
            buf = rest;
        }
        Ok(())
    }

    fn read_to_vec(&mut self, limit: usize) -> RfvpResult<Vec<u8>> {
        let len = self.len()?;
        if len > limit as u64 {
            return Err(RfvpError::CapacityExceeded);
        }
        let mut out = Vec::new();
        out.resize(len as usize, 0);
        self.read_exact_at(0, &mut out)?;
        Ok(out)
    }
}

impl<F: RfvpFile + ?Sized> RfvpFile for Box<F> {
    fn len(&mut self) -> RfvpResult<u64> {
        (**self).len()
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        (**self).read_at(offset, buf)
    }
}

/// Whole-file storage for the player's own data (saves), shared with the game state.
///
/// Paths are relative to the game's directory, with `/` separators (`save/rfvp_s000.bin`);
/// `write` creates the directories it needs. A host that cannot persist anything simply does not
/// provide one (see [`RfvpFileSystem::storage`]) and saves stay in memory for the session.
pub trait RfvpStorage: Send + Sync {
    fn read(&self, path: &str) -> RfvpResult<Vec<u8>>;

    /// The first `max_len` bytes of the file (all of it if it is shorter). Reading just the head
    /// of a save is enough to list it, and saves can be over a megabyte.
    fn read_prefix(&self, path: &str, max_len: usize) -> RfvpResult<Vec<u8>> {
        let mut bytes = self.read(path)?;
        bytes.truncate(max_len);
        Ok(bytes)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> RfvpResult<()>;

    fn exists(&self, path: &str) -> bool;

    fn remove(&self, path: &str) -> RfvpResult<()>;

    /// Names (not paths) of the files directly inside `dir`.
    fn list(&self, dir: &str) -> RfvpResult<Vec<String>>;

    /// Open a file to read parts of it on demand. A host that can should implement this: packs
    /// and large loose assets (songs, movies) are then streamed instead of read whole.
    fn open(&self, _path: &str) -> RfvpResult<Box<dyn RfvpFile + Send>> {
        Err(RfvpError::Unsupported)
    }
}

pub trait RfvpFileSystem {
    type File: RfvpFile;

    fn open(&mut self, path: &str) -> RfvpResult<Self::File>;

    fn read_required_file(&mut self, name: &str, out: &mut Vec<u8>) -> RfvpResult<()> {
        out.clear();
        let mut file = self.open(name)?;
        let bytes = file.read_to_vec(usize::MAX)?;
        out.extend_from_slice(&bytes);
        Ok(())
    }

    fn write_all(&mut self, _path: &str, _bytes: &[u8]) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    /// Storage for the player's saves, if this host can persist them.
    fn storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        None
    }

    /// Read access to the game's own directory, for assets that are loose files instead of in a
    /// pack (a folder such as `graph/` next to the `.bin` files). Paths are relative to the
    /// game directory.
    fn game_files(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        None
    }

    fn metadata(&mut self, path: &str) -> RfvpResult<RfvpFileInfo>;

    fn exists(&mut self, path: &str) -> bool {
        self.metadata(path).is_ok()
    }

    fn enumerate_by_extension(
        &mut self,
        _root: &str,
        _extension_without_dot: &str,
        _visitor: &mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
    ) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfvpAssetPath {
    inner: String,
}

impl RfvpAssetPath {
    pub fn new(path: &str) -> RfvpResult<Self> {
        if path.is_empty() || path.as_bytes().iter().any(|b| *b == 0) {
            return Err(RfvpError::InvalidArgument);
        }
        Ok(Self {
            inner: String::from(path),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }
}
