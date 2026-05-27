use alloc::string::String;
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
