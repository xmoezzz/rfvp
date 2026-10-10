use core::ffi::c_void;

use rfvp::host_api::{RfvpError, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpResult};

use crate::raw::{RawFileHandle, RawFileInfo, RawFileKind, RawFileSystemVTable};
use crate::status::{status_to_result, Ps2Status};

pub struct Ps2FileSystem {
    ctx: *mut c_void,
    vtable: RawFileSystemVTable,
}

impl Ps2FileSystem {
    pub const fn new(ctx: *mut c_void, vtable: RawFileSystemVTable) -> Self {
        Self { ctx, vtable }
    }
}

pub struct Ps2File {
    ctx: *mut c_void,
    vtable: RawFileSystemVTable,
    handle: RawFileHandle,
}

impl Drop for Ps2File {
    fn drop(&mut self) {
        unsafe {
            (self.vtable.close)(self.ctx, self.handle);
        }
    }
}

impl RfvpFile for Ps2File {
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

impl RfvpFileSystem for Ps2FileSystem {
    type File = Ps2File;

    fn open(&mut self, path: &str) -> RfvpResult<Self::File> {
        let mut handle = RawFileHandle::INVALID;
        let status =
            unsafe { (self.vtable.open)(self.ctx, path.as_ptr(), path.len(), &mut handle) };
        status_to_result(status)?;
        if handle == RawFileHandle::INVALID {
            return Err(RfvpError::Backend);
        }
        Ok(Ps2File {
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
        return Ps2Status::InvalidArgument.as_i32();
    }
    let bridge = unsafe { &mut *visitor_ctx.cast::<VisitorBridge<'_>>() };
    let path_bytes = unsafe { core::slice::from_raw_parts(path, path_len) };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(value) => value,
        Err(_) => return Ps2Status::InvalidData.as_i32(),
    };
    match (bridge.visitor)(path, raw_file_info_to_rfvp(info)) {
        Ok(()) => Ps2Status::Ok.as_i32(),
        Err(err) => match err {
            RfvpError::Io => Ps2Status::Io.as_i32(),
            RfvpError::NotFound => Ps2Status::NotFound.as_i32(),
            RfvpError::InvalidData => Ps2Status::InvalidData.as_i32(),
            RfvpError::InvalidArgument => Ps2Status::InvalidArgument.as_i32(),
            RfvpError::Unsupported => Ps2Status::Unsupported.as_i32(),
            RfvpError::OutOfMemory => Ps2Status::OutOfMemory.as_i32(),
            RfvpError::CapacityExceeded => Ps2Status::CapacityExceeded.as_i32(),
            RfvpError::EndOfFile => Ps2Status::EndOfFile.as_i32(),
            RfvpError::Backend => Ps2Status::Backend.as_i32(),
        },
    }
}
