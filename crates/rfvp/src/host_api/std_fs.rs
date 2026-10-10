//! The host file API over `std::fs`: the desktop builds, and the `no_std` harness that runs the
//! console code paths on a desktop.

extern crate std as real_std;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use real_std::fs;
use real_std::io::{Read, Seek, SeekFrom};
use real_std::path::PathBuf;

use super::{RfvpError, RfvpFile, RfvpResult, RfvpStorage};

pub struct StdFile(pub fs::File);

impl RfvpFile for StdFile {
    fn len(&mut self) -> RfvpResult<u64> {
        self.0
            .metadata()
            .map(|m| m.len())
            .map_err(|_| RfvpError::Io)
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        self.0
            .seek(SeekFrom::Start(offset))
            .map_err(|_| RfvpError::Io)?;
        self.0.read(buf).map_err(|_| RfvpError::Io)
    }
}

/// A directory on disk. Paths are relative to it, with `/` separators.
pub struct StdDir {
    root: PathBuf,
}

impl StdDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        let path = path.trim_start_matches("./");
        if path.is_empty() {
            self.root.clone()
        } else {
            self.root.join(path)
        }
    }
}

impl RfvpStorage for StdDir {
    fn read(&self, path: &str) -> RfvpResult<Vec<u8>> {
        fs::read(self.resolve(path)).map_err(|_| RfvpError::NotFound)
    }

    fn read_prefix(&self, path: &str, max_len: usize) -> RfvpResult<Vec<u8>> {
        let file = fs::File::open(self.resolve(path)).map_err(|_| RfvpError::NotFound)?;
        let mut head = Vec::new();
        file.take(max_len as u64)
            .read_to_end(&mut head)
            .map_err(|_| RfvpError::Io)?;
        Ok(head)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        let resolved = self.resolve(path);
        if let Some(parent) = resolved.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(resolved, bytes).map_err(|_| RfvpError::Io)
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).is_file()
    }

    fn remove(&self, path: &str) -> RfvpResult<()> {
        fs::remove_file(self.resolve(path)).map_err(|_| RfvpError::Io)
    }

    fn list(&self, dir: &str) -> RfvpResult<Vec<String>> {
        let read = fs::read_dir(self.resolve(dir)).map_err(|_| RfvpError::NotFound)?;
        Ok(read
            .flatten()
            .filter(|entry| entry.path().is_file())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect())
    }

    fn open(&self, path: &str) -> RfvpResult<Box<dyn RfvpFile + Send>> {
        let file = fs::File::open(self.resolve(path)).map_err(|_| RfvpError::NotFound)?;
        Ok(Box::new(StdFile(file)))
    }
}
