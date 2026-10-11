//! Files on the SD card, through newlib's stdio (libctru mounts `sdmc:` before `main`).

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::{c_char, c_long, c_void, CStr};

use rfvp::host_api::{
    RfvpError, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpResult, RfvpStorage,
};

use crate::ffi::{self, c_path, CFile};

pub struct CtrFileSystem {
    /// Relative paths are resolved against this directory.
    root: String,
    /// Where saves go.
    save_root: String,
}

impl CtrFileSystem {
    pub fn with_root(root: &str) -> Self {
        Self::with_roots(root, root)
    }

    /// Game files from `root`, saves (and the files written when something goes wrong) in
    /// `save_root`.
    pub fn with_roots(root: &str, save_root: &str) -> Self {
        Self {
            root: String::from(root.trim_end_matches(['/', '\\'])),
            save_root: String::from(save_root.trim_end_matches(['/', '\\'])),
        }
    }

    pub fn exists(&mut self, path: &str) -> bool {
        self.metadata(path).is_ok()
    }

    fn full_path(&self, path: &str) -> RfvpResult<String> {
        if path.is_empty() {
            return Err(RfvpError::InvalidArgument);
        }
        if path.contains(':') {
            Ok(String::from(path))
        } else {
            Ok(format!("{}/{}", self.root, path.trim_start_matches("./")))
        }
    }
}

/// An open file; closed when dropped.
pub struct CtrFile {
    file: *mut CFile,
}

impl CtrFile {
    fn open(path: &str, mode: &CStr) -> RfvpResult<Self> {
        let path = c_path(path).ok_or(RfvpError::InvalidArgument)?;
        let file = unsafe { ffi::fopen(path.as_ptr() as *const c_char, mode.as_ptr()) };
        if file.is_null() {
            Err(RfvpError::NotFound)
        } else {
            Ok(Self { file })
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> RfvpResult<()> {
        let written = unsafe { ffi::fwrite(bytes.as_ptr() as *const c_void, 1, bytes.len(), self.file) };
        if written == bytes.len() {
            Ok(())
        } else {
            Err(RfvpError::Io)
        }
    }
}

impl Drop for CtrFile {
    fn drop(&mut self) {
        unsafe { ffi::fclose(self.file) };
    }
}

// SAFETY: a `FILE` may be used from any thread; the VFS only touches it under a lock.
unsafe impl Send for CtrFile {}

impl RfvpFile for CtrFile {
    fn len(&mut self) -> RfvpResult<u64> {
        unsafe {
            if ffi::fseek(self.file, 0, ffi::SEEK_END) != 0 {
                return Err(RfvpError::Io);
            }
            let len = ffi::ftell(self.file);
            if len < 0 {
                return Err(RfvpError::Io);
            }
            Ok(len as u64)
        }
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        let offset = c_long::try_from(offset).map_err(|_| RfvpError::InvalidArgument)?;
        unsafe {
            if ffi::fseek(self.file, offset, ffi::SEEK_SET) != 0 {
                return Err(RfvpError::Io);
            }
            Ok(ffi::fread(buf.as_mut_ptr() as *mut c_void, 1, buf.len(), self.file))
        }
    }
}

fn write_file(path: &str, bytes: &[u8]) -> RfvpResult<()> {
    create_parent_dirs(path);
    CtrFile::open(path, c"wb")
        .map_err(|_| RfvpError::Io)?
        .write_all(bytes)
}

/// What is at `path`: a file (with its size), a directory, or nothing.
fn path_kind(path: &str) -> Option<RfvpFileInfo> {
    let c = c_path(path)?;
    let mut size = 0u64;
    match unsafe { ffi::rfvp_3ds_path_kind(c.as_ptr() as *const c_char, &mut size) } {
        1 => Some(RfvpFileInfo::file(size)),
        2 => Some(RfvpFileInfo {
            len: 0,
            kind: RfvpFileKind::Directory,
        }),
        _ => None,
    }
}

/// The entries of a directory: name, whether it is a directory, size.
fn list_dir(path: &str) -> RfvpResult<Vec<(String, bool, u64)>> {
    unsafe extern "C" fn visit(ctx: *mut c_void, name: *const c_char, is_dir: bool, size: u64) {
        let out = unsafe { &mut *(ctx as *mut Vec<(String, bool, u64)>) };
        if let Ok(name) = unsafe { CStr::from_ptr(name) }.to_str() {
            out.push((String::from(name), is_dir, size));
        }
    }
    let c = c_path(path).ok_or(RfvpError::InvalidArgument)?;
    let mut entries: Vec<(String, bool, u64)> = Vec::new();
    let status = unsafe {
        ffi::rfvp_3ds_list_dir(
            c.as_ptr() as *const c_char,
            visit,
            &mut entries as *mut Vec<(String, bool, u64)> as *mut c_void,
        )
    };
    if status == 0 {
        Ok(entries)
    } else {
        Err(RfvpError::NotFound)
    }
}

/// The game directory: the player's saves (in its `save` directory) and loose game files.
pub struct CtrStorage {
    root: String,
}

impl CtrStorage {
    fn full(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            self.root.clone()
        } else {
            format!("{}/{}", self.root, path)
        }
    }
}

impl RfvpStorage for CtrStorage {
    fn read(&self, path: &str) -> RfvpResult<Vec<u8>> {
        self.read_prefix(path, usize::MAX)
    }

    fn read_prefix(&self, path: &str, max_len: usize) -> RfvpResult<Vec<u8>> {
        let mut file = CtrFile::open(&self.full(path), c"rb")?;
        let len = (file.len()? as usize).min(max_len);
        let mut out = alloc::vec![0u8; len];
        file.read_exact_at(0, &mut out)?;
        Ok(out)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        write_file(&self.full(path), bytes)
    }

    fn exists(&self, path: &str) -> bool {
        path_kind(&self.full(path)).is_some_and(|info| info.kind == RfvpFileKind::File)
    }

    fn remove(&self, path: &str) -> RfvpResult<()> {
        let c = c_path(&self.full(path)).ok_or(RfvpError::InvalidArgument)?;
        if unsafe { ffi::remove(c.as_ptr() as *const c_char) } == 0 {
            Ok(())
        } else {
            Err(RfvpError::Io)
        }
    }

    fn list(&self, dir: &str) -> RfvpResult<Vec<String>> {
        Ok(list_dir(&self.full(dir))?
            .into_iter()
            .filter(|(_, is_dir, _)| !is_dir)
            .map(|(name, _, _)| name)
            .collect())
    }

    fn open(&self, path: &str) -> RfvpResult<alloc::boxed::Box<dyn RfvpFile + Send>> {
        Ok(alloc::boxed::Box::new(CtrFile::open(&self.full(path), c"rb")?))
    }
}

impl RfvpFileSystem for CtrFileSystem {
    type File = CtrFile;

    fn storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        Some(Arc::new(CtrStorage {
            root: self.root.clone(),
        }))
    }

    fn game_files(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        self.storage()
    }

    /// `sdmc:/3ds/rfvp`: each game keeps its saves in `saves/<game>/` there.
    fn save_storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        Some(Arc::new(CtrStorage {
            root: self.save_root.clone(),
        }))
    }

    fn open(&mut self, path: &str) -> RfvpResult<Self::File> {
        CtrFile::open(&self.full_path(path)?, c"rb")
    }

    fn write_all(&mut self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        write_file(&self.full_path(path)?, bytes)
    }

    fn metadata(&mut self, path: &str) -> RfvpResult<RfvpFileInfo> {
        path_kind(&self.full_path(path)?).ok_or(RfvpError::NotFound)
    }

    fn enumerate_by_extension(
        &mut self,
        root: &str,
        extension_without_dot: &str,
        visitor: &mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
    ) -> RfvpResult<()> {
        // Only the directory itself: what is looked for this way (the script) sits at the top,
        // and a repacked game has thousands of loose files in its asset folders.
        let root = self.full_path(root)?;
        for (name, is_dir, size) in list_dir(&root)? {
            let child = format!("{}/{}", root.trim_end_matches('/'), name);
            if !is_dir
                && child
                    .rfind('.')
                    .is_some_and(|pos| child[pos + 1..].eq_ignore_ascii_case(extension_without_dot))
            {
                visitor(&child, RfvpFileInfo::file(size))?;
            }
        }
        Ok(())
    }
}

/// Make the directories leading to `path` (`sdmc:/3ds/rfvp/saves/x/y.bin`).
fn create_parent_dirs(path: &str) {
    let Some((device, rest)) = path.split_once(':') else {
        return;
    };
    let Some((dir, _file)) = rest.rsplit_once('/') else {
        return;
    };
    let mut current = format!("{device}:");
    if rest.starts_with('/') {
        current.push('/');
    }
    for part in dir.split('/').filter(|part| !part.is_empty()) {
        if !current.ends_with(':') && !current.ends_with('/') {
            current.push('/');
        }
        current.push_str(part);
        if let Some(c) = c_path(&current) {
            unsafe { ffi::mkdir(c.as_ptr() as *const c_char, 0o777) };
        }
    }
}
