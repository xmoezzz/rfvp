use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use nx::fs;
use rfvp::host_api::{
    RfvpError, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpResult, RfvpStorage,
};

static FS_READY: AtomicBool = AtomicBool::new(false);

/// Where the game's files live on the SD card when nothing else is configured.
pub const DEFAULT_GAME_ROOT: &str = "sdmc:/switch/rfvp/game";

pub struct HorizonFileSystem {
    /// Relative paths are resolved against this directory.
    root: String,
}

impl HorizonFileSystem {
    pub fn new() -> Self {
        Self::with_root(DEFAULT_GAME_ROOT)
    }

    pub fn with_root(root: &str) -> Self {
        Self {
            root: String::from(root.trim_end_matches('/')),
        }
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    fn ensure_ready(&mut self) -> RfvpResult<()> {
        if FS_READY.load(Ordering::Acquire) {
            return Ok(());
        }
        if !fs::is_fspsrv_session_initialized() {
            fs::initialize_fspsrv_session().map_err(|_| RfvpError::Backend)?;
        }
        fs::mount_sd_card("sdmc").map_err(|_| RfvpError::Backend)?;
        FS_READY.store(true, Ordering::Release);
        Ok(())
    }

    fn horizon_path(&self, path: &str) -> RfvpResult<String> {
        if path.is_empty() || path.as_bytes().iter().any(|b| *b == 0) {
            return Err(RfvpError::InvalidArgument);
        }
        if path.contains(':') {
            Ok(String::from(path))
        } else if path.starts_with('/') {
            Ok(format!("sdmc:{path}"))
        } else {
            let relative = path.trim_start_matches("./");
            Ok(format!("{}/{relative}", self.root))
        }
    }
}

impl Default for HorizonFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

pub struct HorizonFile {
    file: fs::FileAccessor,
}

impl HorizonFile {
    fn new(file: fs::FileAccessor) -> Self {
        Self { file }
    }
}

impl RfvpFile for HorizonFile {
    fn len(&mut self) -> RfvpResult<u64> {
        self.file
            .get_size()
            .map(|len| len as u64)
            .map_err(|_| RfvpError::Backend)
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        self.file
            .seek(fs::SeekFrom::Start(offset))
            .map_err(|_| RfvpError::Backend)?;
        self.file.read_array(buf).map_err(|_| RfvpError::Backend)
    }
}

/// The player's saves, in a `save` directory next to the game files.
pub struct HorizonStorage {
    root: String,
}

impl HorizonStorage {
    fn full(&self, path: &str) -> String {
        format!("{}/{}", self.root, path.trim_start_matches('/'))
    }
}

impl RfvpStorage for HorizonStorage {
    fn read(&self, path: &str) -> RfvpResult<Vec<u8>> {
        self.read_prefix(path, usize::MAX)
    }

    fn read_prefix(&self, path: &str, max_len: usize) -> RfvpResult<Vec<u8>> {
        let full = self.full(path);
        let mut file = fs::open_file(&full, fs::FileOpenOption::Read())
            .map_err(|_| RfvpError::NotFound)?;
        let len = (file.get_size().map_err(|_| RfvpError::Backend)? as usize).min(max_len);
        let mut out = alloc::vec![0u8; len];
        file.seek(fs::SeekFrom::Start(0))
            .map_err(|_| RfvpError::Backend)?;
        let mut filled = 0;
        while filled < len {
            let read = file
                .read_array(&mut out[filled..])
                .map_err(|_| RfvpError::Backend)?;
            if read == 0 {
                return Err(RfvpError::EndOfFile);
            }
            filled += read;
        }
        Ok(out)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        let full = self.full(path);
        create_parent_dirs(&full)?;
        let _ = fs::remove_file(&full);
        fs::create_file(&full, bytes.len(), fs::FileAttribute::None())
            .map_err(|_| RfvpError::Backend)?;
        let mut file =
            fs::open_file(&full, fs::FileOpenOption::Write()).map_err(|_| RfvpError::Backend)?;
        file.seek(fs::SeekFrom::Start(0))
            .map_err(|_| RfvpError::Backend)?;
        file.write_array::<u8, true>(bytes)
            .map_err(|_| RfvpError::Backend)
    }

    fn exists(&self, path: &str) -> bool {
        matches!(
            fs::get_entry_type(&self.full(path)),
            Ok(fs::DirectoryEntryType::File)
        )
    }

    fn remove(&self, path: &str) -> RfvpResult<()> {
        fs::remove_file(&self.full(path)).map_err(|_| RfvpError::Backend)
    }

    fn list(&self, dir: &str) -> RfvpResult<Vec<String>> {
        let mut dir = fs::open_directory(&self.full(dir), fs::DirectoryOpenMode::ReadFiles())
            .map_err(|_| RfvpError::NotFound)?;
        let mut names = Vec::new();
        while let Some(entry) = dir.read_next().map_err(|_| RfvpError::Backend)? {
            if let Ok(name) = entry.name.get_str() {
                names.push(String::from(name));
            }
        }
        Ok(names)
    }
}

impl RfvpFileSystem for HorizonFileSystem {
    type File = HorizonFile;

    fn storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        self.ensure_ready().ok()?;
        Some(Arc::new(HorizonStorage {
            root: self.root.clone(),
        }))
    }

    fn game_files(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        self.storage()
    }

    fn open(&mut self, path: &str) -> RfvpResult<Self::File> {
        self.ensure_ready()?;
        let path = self.horizon_path(path)?;
        fs::open_file(&path, fs::FileOpenOption::Read())
            .map(HorizonFile::new)
            .map_err(|_| RfvpError::NotFound)
    }

    fn write_all(&mut self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        self.ensure_ready()?;
        let path = self.horizon_path(path)?;
        create_parent_dirs(&path)?;
        let _ = fs::remove_file(&path);
        fs::create_file(&path, bytes.len(), fs::FileAttribute::None())
            .map_err(|_| RfvpError::Backend)?;
        let mut file =
            fs::open_file(&path, fs::FileOpenOption::Write()).map_err(|_| RfvpError::Backend)?;
        file.seek(fs::SeekFrom::Start(0))
            .map_err(|_| RfvpError::Backend)?;
        file.write_array::<u8, true>(bytes)
            .map_err(|_| RfvpError::Backend)
    }

    fn metadata(&mut self, path: &str) -> RfvpResult<RfvpFileInfo> {
        self.ensure_ready()?;
        let path = self.horizon_path(path)?;
        let entry_type = fs::get_entry_type(&path).map_err(|_| RfvpError::NotFound)?;
        match entry_type {
            fs::DirectoryEntryType::File => {
                let mut file = fs::open_file(&path, fs::FileOpenOption::Read())
                    .map_err(|_| RfvpError::Backend)?;
                let len = file.get_size().map_err(|_| RfvpError::Backend)? as u64;
                Ok(RfvpFileInfo {
                    len,
                    kind: RfvpFileKind::File,
                })
            }
            fs::DirectoryEntryType::Directory => Ok(RfvpFileInfo {
                len: 0,
                kind: RfvpFileKind::Directory,
            }),
        }
    }

    fn enumerate_by_extension(
        &mut self,
        root: &str,
        extension_without_dot: &str,
        visitor: &mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
    ) -> RfvpResult<()> {
        self.ensure_ready()?;
        let root = self.horizon_path(root)?;
        enumerate_dir_recursive(&root, extension_without_dot, visitor)
    }
}

fn enumerate_dir_recursive(
    root: &str,
    extension_without_dot: &str,
    visitor: &mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
) -> RfvpResult<()> {
    let mut dir = fs::open_directory(
        root,
        fs::DirectoryOpenMode::ReadDirectories() | fs::DirectoryOpenMode::ReadFiles(),
    )
    .map_err(|_| RfvpError::NotFound)?;

    loop {
        let entry = match dir.read_next().map_err(|_| RfvpError::Backend)? {
            Some(entry) => entry,
            None => break,
        };
        let name = entry.name.get_str().map_err(|_| RfvpError::InvalidData)?;
        if name == "." || name == ".." {
            continue;
        }
        let child = join_path(root, name);
        match entry.entry_type {
            fs::DirectoryEntryType::Directory => {
                enumerate_dir_recursive(&child, extension_without_dot, visitor)?;
            }
            fs::DirectoryEntryType::File => {
                if path_has_extension(&child, extension_without_dot) {
                    visitor(
                        &child,
                        RfvpFileInfo {
                            len: entry.file_size as u64,
                            kind: RfvpFileKind::File,
                        },
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn join_path(root: &str, name: &str) -> String {
    if root.ends_with('/') {
        format!("{root}{name}")
    } else {
        format!("{root}/{name}")
    }
}

fn path_has_extension(path: &str, extension_without_dot: &str) -> bool {
    let Some(pos) = path.rfind('.') else {
        return false;
    };
    path[pos + 1..].eq_ignore_ascii_case(extension_without_dot)
}

fn create_parent_dirs(path: &str) -> RfvpResult<()> {
    let Some(last_slash) = path.rfind('/') else {
        return Ok(());
    };
    let dir = &path[..last_slash];
    if dir.is_empty() || dir.ends_with(':') {
        return Ok(());
    }

    let Some((prefix, rest)) = dir.split_once(":/") else {
        return Ok(());
    };
    let mut current = format!("{prefix}:");
    for part in rest.split('/') {
        if part.is_empty() {
            continue;
        }
        current.push('/');
        current.push_str(part);
        let _ = fs::create_directory(&current);
    }
    Ok(())
}
