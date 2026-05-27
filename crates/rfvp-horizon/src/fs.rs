use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

use nx::fs;
use rfvp::host_api::{RfvpError, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpResult};

static FS_READY: AtomicBool = AtomicBool::new(false);

pub struct HorizonFileSystem;

impl HorizonFileSystem {
    pub const fn new() -> Self {
        Self
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

    fn horizon_path(path: &str) -> RfvpResult<String> {
        if path.is_empty() || path.as_bytes().iter().any(|b| *b == 0) {
            return Err(RfvpError::InvalidArgument);
        }
        if path.contains(':') {
            Ok(String::from(path))
        } else if path.starts_with('/') {
            Ok(format!("sdmc:{path}"))
        } else {
            Ok(format!("sdmc:/{path}"))
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

impl RfvpFileSystem for HorizonFileSystem {
    type File = HorizonFile;

    fn open(&mut self, path: &str) -> RfvpResult<Self::File> {
        self.ensure_ready()?;
        let path = Self::horizon_path(path)?;
        fs::open_file(&path, fs::FileOpenOption::Read())
            .map(HorizonFile::new)
            .map_err(|_| RfvpError::NotFound)
    }

    fn write_all(&mut self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        self.ensure_ready()?;
        let path = Self::horizon_path(path)?;
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
        let path = Self::horizon_path(path)?;
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
        let root = Self::horizon_path(root)?;
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
