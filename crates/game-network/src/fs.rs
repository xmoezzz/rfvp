// Copyright (c) 2026 CrySirror contributors. MPL-2.0.
use crate::Directory;
use std::{
    collections::BTreeMap,
    fmt,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    time::SystemTime,
};
static ROOTS: OnceLock<Mutex<BTreeMap<PathBuf, Weak<Mounted>>>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Mounted {
    root: PathBuf,
    source: Directory,
    user: Option<PathBuf>,
}
impl Drop for Mounted {
    fn drop(&mut self) {
        if let Ok(mut roots) = ROOTS.get_or_init(Default::default).lock() {
            roots.remove(&self.root);
        }
    }
}
#[derive(Clone)]
pub struct Mount(Arc<Mounted>);
impl fmt::Debug for Mount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceMount")
            .field("root", &self.0.root)
            .finish()
    }
}
impl Mount {
    pub fn connect(url: &str) -> anyhow::Result<Self> {
        Self::new(Directory::connect(url)?)
    }
    pub fn new(source: Directory) -> anyhow::Result<Self> {
        Self::with_user(source, None)
    }
    pub fn connect_with_user(url: &str, user: &Path) -> anyhow::Result<Self> {
        Self::from_directory(Directory::connect(url)?, user)
    }
    pub fn from_directory(source: Directory, user: &Path) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !user.as_os_str().is_empty() && !user.to_string_lossy().contains("://"),
            "user root must be a local directory"
        );
        std::fs::create_dir_all(user)?;
        Self::with_user(source, Some(user.canonicalize()?))
    }
    fn with_user(source: Directory, user: Option<PathBuf>) -> anyhow::Result<Self> {
        let id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| anyhow::anyhow!("resource mount identities exhausted"))?;
        let root = std::env::temp_dir().join(format!(".game-network-{}-{id}", std::process::id()));
        let mounted = Arc::new(Mounted {
            root: root.clone(),
            source,
            user,
        });
        ROOTS
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| anyhow::anyhow!("resource mount registry poisoned"))?
            .insert(root, Arc::downgrade(&mounted));
        Ok(Self(mounted))
    }
    pub fn path(&self) -> &Path {
        &self.0.root
    }
}
fn source(path: &Path) -> io::Result<Option<(Directory, String)>> {
    // Upgrade/drop mount owners outside the registry lock: the last owner
    // removes its entry in Drop and must never recursively lock this mutex.
    let roots = ROOTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| io::Error::other("resource registry poisoned"))?
        .iter()
        .map(|(p, w)| (p.clone(), w.clone()))
        .collect::<Vec<_>>();
    for (root, weak) in roots.iter() {
        if let Ok(suffix) = path.strip_prefix(root) {
            let mounted = weak
                .upgrade()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            let mut parts = Vec::new();
            for c in suffix.components() {
                match c {
                    Component::Normal(n) => parts.push(n.to_str().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "network resource name is not UTF-8",
                        )
                    })?),
                    Component::CurDir => {}
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "resource path escapes mount",
                        ))
                    }
                }
            }
            return Ok(Some((mounted.source.clone(), parts.join("/"))));
        }
    }
    Ok(None)
}
pub fn is_remote(path: impl AsRef<Path>) -> bool {
    source(path.as_ref()).is_ok_and(|v| v.is_some())
}
pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let p = path.as_ref();
    match source(p)? {
        Some((s, r)) => {
            let file = s.open(&r)?;
            let len = usize::try_from(file.len())
                .map_err(|_| io::Error::other("resource length overflow"))?;
            if len > 1024 * 1024 * 1024 {
                return Err(io::Error::other(
                    "whole resource exceeds 1GiB; use streaming",
                ));
            }
            let mut out = Vec::new();
            out.try_reserve_exact(len)
                .map_err(|_| io::Error::other("resource allocation budget"))?;
            file.take(len as u64 + 1).read_to_end(&mut out)?;
            if out.len() != len {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "network resource length changed",
                ));
            }
            Ok(out)
        }
        None => std::fs::read(p),
    }
}
pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
pub fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    let p = path.as_ref();
    match source(p)? {
        Some((s, r)) => {
            let m = s.metadata(&r)?;
            Ok(Metadata {
                local: None,
                len: m.len,
                directory: m.directory,
            })
        }
        None => {
            let m = std::fs::metadata(p)?;
            Ok(Metadata {
                len: m.len(),
                directory: m.is_dir(),
                local: Some(m),
            })
        }
    }
}
pub fn is_file(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok_and(|m| m.is_file())
}
pub fn is_dir(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok_and(|m| m.is_dir())
}
pub fn exists(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok()
}
pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let p = path.as_ref();
    match source(p)? {
        Some((s, r)) => {
            s.metadata(&r)?;
            Ok(p.to_owned())
        }
        None => std::fs::canonicalize(p),
    }
}
#[derive(Clone, Debug)]
pub struct Metadata {
    local: Option<std::fs::Metadata>,
    len: u64,
    directory: bool,
}
impl Metadata {
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_file(&self) -> bool {
        self.local.as_ref().map_or(!self.directory, |m| m.is_file())
    }
    pub fn is_dir(&self) -> bool {
        self.directory
    }
    pub fn modified(&self) -> io::Result<SystemTime> {
        self.local
            .as_ref()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "network modification time unavailable",
                )
            })?
            .modified()
    }
}
#[derive(Clone, Debug)]
pub struct DirEntry {
    path: PathBuf,
    name: std::ffi::OsString,
}
impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
    pub fn file_name(&self) -> std::ffi::OsString {
        self.name.clone()
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        metadata(&self.path)
    }
    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(FileType(self.metadata()?.is_dir()))
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FileType(bool);
impl FileType {
    pub fn is_dir(&self) -> bool {
        self.0
    }
    pub fn is_file(&self) -> bool {
        !self.0
    }
    pub fn is_symlink(&self) -> bool {
        false
    }
}
pub fn read_dir(path: impl AsRef<Path>) -> io::Result<std::vec::IntoIter<io::Result<DirEntry>>> {
    let p = path.as_ref();
    let names = match source(p)? {
        Some((s, r)) => s
            .list(&r)?
            .into_iter()
            .map(Into::into)
            .collect::<Vec<std::ffi::OsString>>(),
        None => std::fs::read_dir(p)?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<io::Result<Vec<_>>>()?,
    };
    Ok(names
        .into_iter()
        .map(|name| {
            Ok(DirEntry {
                path: p.join(&name),
                name,
            })
        })
        .collect::<Vec<_>>()
        .into_iter())
}
#[derive(Debug)]
pub enum File {
    Local(std::fs::File),
    Remote(crate::File),
}
impl File {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let p = path.as_ref();
        match source(p)? {
            Some((s, r)) => Ok(Self::Remote(s.open(&r)?)),
            None => Ok(Self::Local(std::fs::File::open(p)?)),
        }
    }
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        writable(path.as_ref())?;
        Ok(Self::Local(std::fs::File::create(path)?))
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        match self {
            Self::Local(f) => {
                let m = f.metadata()?;
                Ok(Metadata {
                    len: m.len(),
                    directory: m.is_dir(),
                    local: Some(m),
                })
            }
            Self::Remote(f) => Ok(Metadata {
                len: f.len(),
                directory: false,
                local: None,
            }),
        }
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(match self {
            Self::Local(f) => Self::Local(f.try_clone()?),
            Self::Remote(f) => Self::Remote(f.clone()),
        })
    }
    pub fn sync_all(&self) -> io::Result<()> {
        match self {
            Self::Local(f) => f.sync_all(),
            Self::Remote(_) => Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        }
    }
}
impl Read for File {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Local(f) => f.read(b),
            Self::Remote(f) => f.read(b),
        }
    }
}
impl Seek for File {
    fn seek(&mut self, s: SeekFrom) -> io::Result<u64> {
        match self {
            Self::Local(f) => f.seek(s),
            Self::Remote(f) => f.seek(s),
        }
    }
}
impl Write for File {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        match self {
            Self::Local(f) => f.write(b),
            Self::Remote(_) => Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Local(f) => f.flush(),
            Self::Remote(_) => Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        }
    }
}
fn writable(path: &Path) -> io::Result<()> {
    if source(path)?.is_some() {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "network resources are read-only",
        ))
    } else {
        Ok(())
    }
}
pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    writable(path.as_ref())?;
    std::fs::write(path, bytes)
}
pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    writable(path.as_ref())?;
    std::fs::create_dir_all(path)
}
pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    writable(path.as_ref())?;
    std::fs::remove_file(path)
}
pub fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    writable(path.as_ref())?;
    std::fs::remove_dir_all(path)
}

/// Explicitly configured local save directory, distinct from resource authority.
pub fn user_root(path: &Path) -> io::Result<Option<PathBuf>> {
    // Upgrade/drop mount owners outside the registry lock: the last owner
    // removes its entry in Drop and must never recursively lock this mutex.
    let roots = ROOTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| io::Error::other("resource registry poisoned"))?
        .iter()
        .map(|(p, w)| (p.clone(), w.clone()))
        .collect::<Vec<_>>();
    for (root, weak) in roots.iter() {
        if path.starts_with(root) {
            return weak
                .upgrade()
                .and_then(|m| m.user.clone())
                .map(Some)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "network runtime requires a local user root",
                    )
                });
        }
    }
    Ok(None)
}
