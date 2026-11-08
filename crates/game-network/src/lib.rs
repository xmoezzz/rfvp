//! Optional game-resource transports. Neither `nfs` nor `smb` is enabled by
//! default. Connect and read on a session/resource worker, not an event loop.
//! Reads use bounded shared block caches; each file has its own seek cursor.
//! Resources are read-only; `fs::Mount::from_directory` binds an explicit local
//! user directory for native saves. No game data is mirrored to a local tree.
// Copyright (c) 2026 CrySirror contributors. MPL-2.0.
//! Optional native network resource directories. Connect and read on the
//! engine's resource owner/worker, never on its window/event thread.
#![forbid(unsafe_code)]
use std::{
    fmt,
    io::{self, Read, Seek, SeekFrom},
    sync::Arc,
};
#[cfg(feature = "nfs")]
mod nfs;
#[cfg(feature = "smb")]
mod smb;
#[cfg(feature = "smb")]
mod tasks;

#[derive(Debug)]
pub enum Error {
    NotFound,
    Invalid(&'static str),
    Unsupported(&'static str),
    Limit(&'static str),
    Io(String),
}
pub type Result<T> = std::result::Result<T, Error>;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("resource not found"),
            Self::Invalid(s) | Self::Unsupported(s) | Self::Limit(s) => f.write_str(s),
            Self::Io(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for Error {}
impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        let kind = match e {
            Error::NotFound => io::ErrorKind::NotFound,
            Error::Invalid(_) => io::ErrorKind::InvalidInput,
            Error::Unsupported(_) => io::ErrorKind::Unsupported,
            _ => io::ErrorKind::Other,
        };
        Self::new(kind, e)
    }
}
mod path {
    use super::*;
    pub enum Mount {
        Asset,
    }
    pub struct VfsPath(String);
    impl VfsPath {
        pub fn new(_: Mount, path: &str) -> Result<Self> {
            validate(path)?;
            Ok(Self(path.into()))
        }
        pub fn relative(&self) -> &str {
            &self.0
        }
    }
}
pub mod service {
    use super::*;
    pub trait AssetReader: Send + Sync {
        fn len(&self) -> Result<u64>;
        fn read_at(&self, offset: u64, bytes: &mut [u8]) -> Result<usize>;
    }
}
pub mod vfs {
    use super::*;
    pub trait VfsProvider: Send + Sync {
        fn kind(&self) -> &'static str;
        fn read(&self, path: &str) -> Result<Vec<u8>>;
        fn open(&self, path: &str) -> Result<Arc<dyn service::AssetReader>>;
        fn exists(&self, path: &str) -> bool;
        fn list_dir(&self, path: &str) -> Result<Vec<String>>;
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Metadata {
    pub directory: bool,
    pub len: u64,
}
pub trait RemoteStorage: vfs::VfsProvider {
    fn metadata(&self, path: &str) -> Result<Metadata>;
    fn origin(&self) -> &str;
}
fn validate(path: &str) -> Result<()> {
    if path.is_empty() {
        return Ok(());
    }
    if path.contains(['\\', '\0', ':']) || path.split('/').any(|p| matches!(p, "" | "." | "..")) {
        return Err(Error::Invalid(
            "network path must be relative with canonical components",
        ));
    }
    Ok(())
}
/// A single explicitly selected read-only directory. No global current mount,
/// bulk download, network writes, or implicit local fallback is performed.
#[derive(Clone)]
pub struct Directory {
    source: Arc<dyn RemoteStorage>,
    prefix: String,
}
impl fmt::Debug for Directory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetworkDirectory")
            .field("origin", &self.source.origin())
            .field("prefix", &self.prefix)
            .finish()
    }
}
impl Directory {
    /// Inject an already-scoped host provider (e.g. a mounted network VFS).
    /// This preserves a host connection/cache and never reconnects with copied
    /// URL credentials. The provider's root is the entire granted authority.
    pub fn from_provider(source: Arc<dyn RemoteStorage>) -> anyhow::Result<Self> {
        if !source.metadata("")?.directory {
            anyhow::bail!("resource provider root is not a directory");
        }
        Ok(Self {
            source,
            prefix: String::new(),
        })
    }

    pub fn connect(url: &str) -> anyhow::Result<Self> {
        let (source, prefix): (Arc<dyn RemoteStorage>, String) =
            match url.split_once("://").map(|v| v.0) {
                #[cfg(feature = "nfs")]
                Some("nfs") => {
                    let (s, p) = nfs::NfsRoot::connect(url)?;
                    (Arc::new(s), p)
                }
                #[cfg(feature = "smb")]
                Some("smb") => {
                    let (s, p) = smb::SmbRoot::connect(url)?;
                    (Arc::new(s), p)
                }
                _ => anyhow::bail!(
                    "network scheme unavailable; enable the matching nfs or smb feature"
                ),
            };
        validate(&prefix)?;
        if !source.metadata(&prefix)?.directory {
            anyhow::bail!("network resource root is not a directory");
        }
        Ok(Self { source, prefix })
    }
    fn path(&self, path: &str) -> Result<String> {
        validate(path)?;
        Ok(match (self.prefix.is_empty(), path.is_empty()) {
            (true, _) => path.into(),
            (_, true) => self.prefix.clone(),
            _ => format!("{}/{path}", self.prefix),
        })
    }
    pub fn origin(&self) -> &str {
        self.source.origin()
    }
    pub fn metadata(&self, path: &str) -> io::Result<Metadata> {
        Ok(self.source.metadata(&self.path(path)?)?)
    }
    pub fn list(&self, path: &str) -> io::Result<Vec<String>> {
        Ok(self.source.list_dir(&self.path(path)?)?)
    }
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        Ok(self.source.read(&self.path(path)?)?)
    }
    pub fn open(&self, path: &str) -> io::Result<File> {
        let source = self.source.open(&self.path(path)?)?;
        let len = source.len()?;
        Ok(File {
            source,
            position: 0,
            len,
        })
    }
}
/// Independent seek cursor over a shared bounded random-access reader.
#[derive(Clone)]
pub struct File {
    source: Arc<dyn service::AssetReader>,
    position: u64,
    len: u64,
}
impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetworkFile")
            .field("position", &self.position)
            .field("len", &self.len)
            .finish()
    }
}
impl File {
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn read_at(&self, offset: u64, bytes: &mut [u8]) -> io::Result<usize> {
        Ok(self.source.read_at(offset, bytes)?)
    }
}
impl Read for File {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let n = self.read_at(self.position, bytes)?;
        self.position = self
            .position
            .checked_add(n as u64)
            .ok_or_else(|| io::Error::other("network cursor overflow"))?;
        Ok(n)
    }
}
impl Seek for File {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let next = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => i128::from(self.len) + i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
        };
        self.position = u64::try_from(next)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "network seek outside u64"))?;
        Ok(self.position)
    }
}

/// Native-path compatibility for resource loaders. Each mount has a unique
/// synthetic prefix, owns its connection, and leaves no files on disk.
pub mod fs;

#[cfg(test)]
mod tests {
    use super::*;
    struct Memory;
    impl service::AssetReader for Memory {
        fn len(&self) -> Result<u64> {
            Ok(6)
        }
        fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize> {
            let data = b"abcdef";
            let start = usize::try_from(offset).unwrap_or(usize::MAX);
            if start >= data.len() {
                return Ok(0);
            }
            let n = out.len().min(data.len() - start);
            out[..n].copy_from_slice(&data[start..start + n]);
            Ok(n)
        }
    }
    struct Source;
    impl vfs::VfsProvider for Source {
        fn kind(&self) -> &'static str {
            "memory-fixture"
        }
        fn read(&self, _: &str) -> Result<Vec<u8>> {
            panic!("filesystem readers must use random-access handles")
        }
        fn open(&self, path: &str) -> Result<Arc<dyn service::AssetReader>> {
            if path == "game/a.bin" {
                Ok(Arc::new(Memory))
            } else {
                Err(Error::NotFound)
            }
        }
        fn exists(&self, path: &str) -> bool {
            self.metadata(path).is_ok()
        }
        fn list_dir(&self, path: &str) -> Result<Vec<String>> {
            if path == "game" {
                Ok(vec!["a.bin".into()])
            } else {
                Err(Error::NotFound)
            }
        }
    }
    impl RemoteStorage for Source {
        fn origin(&self) -> &str {
            "fixture://root/"
        }
        fn metadata(&self, path: &str) -> Result<Metadata> {
            match path {
                "game" => Ok(Metadata {
                    directory: true,
                    len: 0,
                }),
                "game/a.bin" => Ok(Metadata {
                    directory: false,
                    len: 6,
                }),
                _ => Err(Error::NotFound),
            }
        }
    }
    fn directory() -> Directory {
        Directory {
            source: Arc::new(Source),
            prefix: "game".into(),
        }
    }
    #[test]
    fn random_access_cloned_cursors_and_checked_seeks_preserve_the_stream() {
        let mut first = directory().open("a.bin").unwrap();
        let mut bytes = [0; 2];
        first.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ab");
        let mut second = first.clone();
        first.seek(SeekFrom::End(-2)).unwrap();
        first.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ef");
        second.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"cd");
        assert!(first.seek(SeekFrom::Start(u64::MAX)).is_ok());
        assert!(first.seek(SeekFrom::Current(1)).is_err());
        assert!(second.seek(SeekFrom::Start(0)).is_ok());
        assert!(second.seek(SeekFrom::Current(-1)).is_err());
        assert!(directory().open("../a.bin").is_err());
        assert!(directory().open("/a.bin").is_err());
    }
    #[test]
    fn mounted_resources_are_scoped_read_only_and_never_materialized() {
        let mount = fs::Mount::new(directory()).unwrap();
        let root = mount.path().to_owned();
        assert!(!root.exists());
        assert!(fs::is_dir(&root));
        assert_eq!(fs::read(root.join("a.bin")).unwrap(), b"abcdef");
        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .file_name(),
            "a.bin"
        );
        assert!(fs::read(root.join("../a.bin")).is_err());
        assert!(fs::File::create(root.join("a.bin")).is_err());
        assert!(fs::write(root.join("a.bin"), b"modified").is_err());
        let mut file = fs::File::open(root.join("a.bin")).unwrap();
        drop(mount);
        assert!(!root.exists());
        assert!(!fs::is_remote(&root));
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"abcdef");
    }
}
