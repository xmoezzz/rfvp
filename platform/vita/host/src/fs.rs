use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::c_void;
use rfvp::host_api::{
    RfvpError, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpResult, RfvpStorage,
};

use crate::raw::{RawFileHandle, RawFileInfo, RawFileKind, RawFileSystemVTable};
use crate::status::status_to_result;

pub struct PsvFileSystem {
    ctx: *mut c_void,
    vtable: RawFileSystemVTable,
}

impl PsvFileSystem {
    pub const fn new(ctx: *mut c_void, vtable: RawFileSystemVTable) -> Self {
        Self { ctx, vtable }
    }
}

pub struct PsvFile {
    ctx: *mut c_void,
    vtable: RawFileSystemVTable,
    handle: RawFileHandle,
}

impl PsvFile {
    pub const fn raw_handle(&self) -> RawFileHandle {
        self.handle
    }
}

// SAFETY: the context is the platform layer's global state and its functions may be called from
// any thread; a file is used by one thread at a time.
unsafe impl Send for PsvFile {}

impl Drop for PsvFile {
    fn drop(&mut self) {
        unsafe {
            (self.vtable.close)(self.ctx, self.handle);
        }
    }
}

impl RfvpFile for PsvFile {
    fn len(&mut self) -> RfvpResult<u64> {
        let mut out_len = 0_u64;
        let status = unsafe { (self.vtable.len)(self.ctx, self.handle, &mut out_len) };
        status_to_result(status)?;
        Ok(out_len)
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut out_read = 0_usize;
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
pub struct PsvStorage {
    ctx: *mut c_void,
    vtable: RawFileSystemVTable,
}

// SAFETY: the context is the platform layer's global state and its functions may be called from
// any thread; the engine only uses saves from one thread at a time anyway.
unsafe impl Send for PsvStorage {}
unsafe impl Sync for PsvStorage {}

impl PsvStorage {
    fn open(&self, path: &str) -> RfvpResult<PsvFile> {
        let mut handle = RawFileHandle::INVALID;
        let status =
            unsafe { (self.vtable.open)(self.ctx, path.as_ptr(), path.len(), &mut handle) };
        status_to_result(status)?;
        if handle == RawFileHandle::INVALID {
            return Err(RfvpError::Backend);
        }
        Ok(PsvFile {
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

impl RfvpStorage for PsvStorage {
    /// Lets the engine read big files (packs, music) a piece at a time instead of whole.
    fn open(&self, path: &str) -> RfvpResult<alloc::boxed::Box<dyn RfvpFile + Send>> {
        Ok(alloc::boxed::Box::new(PsvStorage::open(self, path)?))
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
        let mut names: Vec<String> = Vec::new();
        let mut visitor = |name: &str, _info: RfvpFileInfo| -> RfvpResult<()> {
            names.push(String::from(name));
            Ok(())
        };
        let mut bridge = VisitorBridge { visitor: &mut visitor };
        let status = unsafe {
            rfvp_psv_c_list_files(
                dir.as_ptr(),
                dir.len(),
                (&mut bridge as *mut VisitorBridge<'_>).cast::<c_void>(),
                enumerate_visitor_bridge,
            )
        };
        status_to_result(status)?;
        Ok(names)
    }
}

impl RfvpFileSystem for PsvFileSystem {
    type File = PsvFile;

    fn storage(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        Some(Arc::new(PsvStorage {
            ctx: self.ctx,
            vtable: self.vtable,
        }))
    }

    fn game_files(&mut self) -> Option<Arc<dyn RfvpStorage>> {
        self.storage()
    }

    /// `ux0:data/rfvp`: each game keeps its saves in `saves/<game>/` there.
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
        Ok(PsvFile {
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
    let kind = match info.kind {
        RawFileKind::File => RfvpFileKind::File,
        RawFileKind::Directory => RfvpFileKind::Directory,
        RawFileKind::Other => RfvpFileKind::Other,
    };
    RfvpFileInfo {
        len: info.len,
        kind,
    }
}

struct VisitorBridge<'a> {
    visitor: &'a mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
}


extern "C" {
    /// The files directly in a directory of the game folder (rfvp_psv_c.c).
    fn rfvp_psv_c_list_files(
        dir: *const u8,
        dir_len: usize,
        visitor_ctx: *mut c_void,
        visitor: crate::raw::RawEnumerateByExtensionVisitorFn,
    ) -> i32;
}

unsafe extern "C" fn enumerate_visitor_bridge(
    visitor_ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    info: RawFileInfo,
) -> i32 {
    if visitor_ctx.is_null() || path.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }

    let bridge = unsafe { &mut *visitor_ctx.cast::<VisitorBridge<'_>>() };
    let path_bytes = unsafe { core::slice::from_raw_parts(path, path_len) };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(value) => value,
        Err(_) => return crate::status::PsvStatus::InvalidData.as_i32(),
    };

    match (bridge.visitor)(path, raw_file_info_to_rfvp(info)) {
        Ok(()) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => rfvp_error_to_psv_status(err),
    }
}

fn rfvp_error_to_psv_status(err: RfvpError) -> i32 {
    match err {
        RfvpError::Io => crate::status::PsvStatus::Io.as_i32(),
        RfvpError::NotFound => crate::status::PsvStatus::NotFound.as_i32(),
        RfvpError::InvalidData => crate::status::PsvStatus::InvalidData.as_i32(),
        RfvpError::InvalidArgument => crate::status::PsvStatus::InvalidArgument.as_i32(),
        RfvpError::Unsupported => crate::status::PsvStatus::Unsupported.as_i32(),
        RfvpError::OutOfMemory => crate::status::PsvStatus::OutOfMemory.as_i32(),
        RfvpError::CapacityExceeded => crate::status::PsvStatus::CapacityExceeded.as_i32(),
        RfvpError::EndOfFile => crate::status::PsvStatus::EndOfFile.as_i32(),
        RfvpError::Backend => crate::status::PsvStatus::Backend.as_i32(),
    }
}
