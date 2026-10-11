use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::c_void;

use rfvp::host_api::{
    RfvpError, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpResult, RfvpStorage,
};

use crate::raw::{RawFileHandle, RawFileInfo, RawFileKind, RawFileSystemVTable};
use crate::status::{status_to_result, PspStatus};

pub struct PspFileSystem {
    ctx: *mut c_void,
    vtable: RawFileSystemVTable,
}

impl PspFileSystem {
    pub const fn new(ctx: *mut c_void, vtable: RawFileSystemVTable) -> Self {
        Self { ctx, vtable }
    }
}

pub struct PspFile {
    ctx: *mut c_void,
    vtable: RawFileSystemVTable,
    handle: RawFileHandle,
}

// SAFETY: the context is the platform layer's global state; a file is used by one thread at a
// time (the audio thread reads streamed music under the mixer's lock).
unsafe impl Send for PspFile {}

impl Drop for PspFile {
    fn drop(&mut self) {
        unsafe {
            (self.vtable.close)(self.ctx, self.handle);
        }
    }
}

impl RfvpFile for PspFile {
    fn len(&mut self) -> RfvpResult<u64> {
        let mut out_len = 0;
        let status = unsafe { (self.vtable.len)(self.ctx, self.handle, &mut out_len) };
        status_to_result(status)?;
        Ok(out_len)
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut out_read = 0;
        let status = unsafe {
            (self.vtable.read_at)(
                self.ctx,
                self.handle,
                offset,
                buf.as_mut_ptr(),
                buf.len(),
                &mut out_read,
            )
        };
        status_to_result(status)?;
        if out_read > buf.len() {
            return Err(RfvpError::Backend);
        }
        Ok(out_read)
    }
}

/// The player's saves, on the same file system as the game.
///
/// There is no way to delete a file through the platform layer, so a removed file is truncated
/// to nothing and an empty file counts as missing.
pub struct PspStorage {
    ctx: *mut c_void,
    vtable: RawFileSystemVTable,
}

// SAFETY: the context is the platform layer's global state, and the engine only uses saves from
// one thread at a time.
unsafe impl Send for PspStorage {}
unsafe impl Sync for PspStorage {}

impl PspStorage {
    fn open(&self, path: &str) -> RfvpResult<PspFile> {
        let mut handle = RawFileHandle::INVALID;
        let status =
            unsafe { (self.vtable.open)(self.ctx, path.as_ptr(), path.len(), &mut handle) };
        status_to_result(status)?;
        if handle == RawFileHandle::INVALID {
            return Err(RfvpError::Backend);
        }
        Ok(PspFile {
            ctx: self.ctx,
            vtable: self.vtable,
            handle,
        })
    }

    fn file_info(&self, path: &str) -> RfvpResult<RfvpFileInfo> {
        let mut info = RawFileInfo {
            len: 0,
            kind: RawFileKind::Other,
        };
        let status =
            unsafe { (self.vtable.metadata)(self.ctx, path.as_ptr(), path.len(), &mut info) };
        status_to_result(status)?;
        Ok(raw_file_info_to_rfvp(info))
    }
}

impl RfvpStorage for PspStorage {
    /// Lets the engine read big files (packs, music) a piece at a time instead of whole.
    fn open(&self, path: &str) -> RfvpResult<alloc::boxed::Box<dyn RfvpFile + Send>> {
        Ok(alloc::boxed::Box::new(PspStorage::open(self, path)?))
    }

    fn read(&self, path: &str) -> RfvpResult<Vec<u8>> {
        let mut file = self.open(path)?;
        file.read_to_vec(usize::MAX)
    }

    fn read_prefix(&self, path: &str, max_len: usize) -> RfvpResult<Vec<u8>> {
        let mut file = self.open(path)?;
        let len = (file.len()? as usize).min(max_len);
        let mut out = Vec::new();
        out.resize(len, 0);
        file.read_exact_at(0, &mut out)?;
        Ok(out)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        // The platform layer creates the directories on the way.
        let status = unsafe {
            (self.vtable.write_all)(
                self.ctx,
                path.as_ptr(),
                path.len(),
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        status_to_result(status)
    }

    fn exists(&self, path: &str) -> bool {
        matches!(
            self.file_info(path),
            Ok(RfvpFileInfo { kind: RfvpFileKind::File, len, .. }) if len > 0
        )
    }

    fn remove(&self, path: &str) -> RfvpResult<()> {
        self.write(path, &[])
    }

    fn list(&self, dir: &str) -> RfvpResult<Vec<String>> {
        #[cfg(target_os = "psp")]
        {
            crate::platform::list_files(dir).map_err(|status| {
                status_to_result(status).err().unwrap_or(RfvpError::Io)
            })
        }
        #[cfg(not(target_os = "psp"))]
        {
            let _ = dir;
            Err(RfvpError::Unsupported)
        }
    }
}

impl RfvpFileSystem for PspFileSystem {
    type File = PspFile;

    fn storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        Some(Arc::new(PspStorage {
            ctx: self.ctx,
            vtable: self.vtable,
        }))
    }

    fn game_files(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        self.storage()
    }

    /// The game's folder on the memory stick: each game keeps its saves in `saves/<game>/`.
    fn save_storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        self.storage()
    }

    fn open(&mut self, path: &str) -> RfvpResult<Self::File> {
        let mut handle = RawFileHandle::INVALID;
        let status =
            unsafe { (self.vtable.open)(self.ctx, path.as_ptr(), path.len(), &mut handle) };
        status_to_result(status)?;
        if handle == RawFileHandle::INVALID {
            return Err(RfvpError::Backend);
        }
        Ok(PspFile {
            ctx: self.ctx,
            vtable: self.vtable,
            handle,
        })
    }

    fn write_all(&mut self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        let status = unsafe {
            (self.vtable.write_all)(
                self.ctx,
                path.as_ptr(),
                path.len(),
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        status_to_result(status)
    }

    fn metadata(&mut self, path: &str) -> RfvpResult<RfvpFileInfo> {
        let mut info = RawFileInfo {
            len: 0,
            kind: RawFileKind::Other,
        };
        let status =
            unsafe { (self.vtable.metadata)(self.ctx, path.as_ptr(), path.len(), &mut info) };
        status_to_result(status)?;
        Ok(raw_file_info_to_rfvp(info))
    }

    fn enumerate_by_extension(
        &mut self,
        root: &str,
        extension_without_dot: &str,
        visitor: &mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
    ) -> RfvpResult<()> {
        let Some(enumerate) = self.vtable.enumerate_by_extension else {
            return Err(RfvpError::Unsupported);
        };
        let mut bridge = VisitorBridge { visitor };
        let status = unsafe {
            enumerate(
                self.ctx,
                root.as_ptr(),
                root.len(),
                extension_without_dot.as_ptr(),
                extension_without_dot.len(),
                (&mut bridge as *mut VisitorBridge<'_>).cast::<c_void>(),
                enumerate_visitor_bridge,
            )
        };
        status_to_result(status)
    }
}

fn raw_file_info_to_rfvp(info: RawFileInfo) -> RfvpFileInfo {
    RfvpFileInfo {
        len: info.len,
        kind: match info.kind {
            RawFileKind::File => RfvpFileKind::File,
            RawFileKind::Directory => RfvpFileKind::Directory,
            RawFileKind::Other => RfvpFileKind::Other,
        },
    }
}

struct VisitorBridge<'a> {
    visitor: &'a mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
}

unsafe extern "C" fn enumerate_visitor_bridge(
    visitor_ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    info: RawFileInfo,
) -> i32 {
    if visitor_ctx.is_null() || path.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let bridge = unsafe { &mut *visitor_ctx.cast::<VisitorBridge<'_>>() };
    let path_bytes = unsafe { core::slice::from_raw_parts(path, path_len) };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(value) => value,
        Err(_) => return PspStatus::InvalidData.as_i32(),
    };
    match (bridge.visitor)(path, raw_file_info_to_rfvp(info)) {
        Ok(()) => PspStatus::Ok.as_i32(),
        Err(err) => match err {
            RfvpError::Io => PspStatus::Io.as_i32(),
            RfvpError::NotFound => PspStatus::NotFound.as_i32(),
            RfvpError::InvalidData => PspStatus::InvalidData.as_i32(),
            RfvpError::InvalidArgument => PspStatus::InvalidArgument.as_i32(),
            RfvpError::Unsupported => PspStatus::Unsupported.as_i32(),
            RfvpError::OutOfMemory => PspStatus::OutOfMemory.as_i32(),
            RfvpError::CapacityExceeded => PspStatus::CapacityExceeded.as_i32(),
            RfvpError::EndOfFile => PspStatus::EndOfFile.as_i32(),
            RfvpError::Backend => PspStatus::Backend.as_i32(),
        },
    }
}
