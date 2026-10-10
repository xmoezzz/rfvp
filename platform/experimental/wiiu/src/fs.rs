use alloc::format;
use alloc::string::String;
use core::ffi::c_void;

use rfvp::host_api::{RfvpError, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpResult};

use crate::status::wiiu_status_to_rfvp_error;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawWiiUFileHandle {
    value: u64,
}

impl RawWiiUFileHandle {
    const INVALID: Self = Self { value: u64::MAX };
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawWiiUFileKind {
    File = 0,
    Directory = 1,
    Other = 2,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawWiiUFileInfo {
    len: u64,
    kind: RawWiiUFileKind,
}

type RawEnumerateVisitor = unsafe extern "C" fn(
    visitor_ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    info: RawWiiUFileInfo,
) -> i32;

unsafe extern "C" {
    fn rfvp_wiiu_platform_fs_open(
        path: *const u8,
        path_len: usize,
        out_handle: *mut RawWiiUFileHandle,
    ) -> i32;
    fn rfvp_wiiu_platform_fs_close(handle: RawWiiUFileHandle);
    fn rfvp_wiiu_platform_fs_read_at(
        handle: RawWiiUFileHandle,
        offset: u64,
        buf: *mut u8,
        len: usize,
        out_read: *mut usize,
    ) -> i32;
    fn rfvp_wiiu_platform_fs_len(handle: RawWiiUFileHandle, out_len: *mut u64) -> i32;
    fn rfvp_wiiu_platform_fs_metadata(
        path: *const u8,
        path_len: usize,
        out_info: *mut RawWiiUFileInfo,
    ) -> i32;
    fn rfvp_wiiu_platform_fs_write_all(
        path: *const u8,
        path_len: usize,
        bytes: *const u8,
        byte_len: usize,
    ) -> i32;
    fn rfvp_wiiu_platform_fs_enumerate_by_extension(
        root: *const u8,
        root_len: usize,
        extension: *const u8,
        extension_len: usize,
        visitor_ctx: *mut c_void,
        visitor: RawEnumerateVisitor,
    ) -> i32;
}

pub struct WiiUFileSystem;

impl WiiUFileSystem {
    pub const fn new() -> Self {
        Self
    }

    fn wiiu_path(path: &str) -> RfvpResult<String> {
        if path.is_empty() || path.as_bytes().iter().any(|b| *b == 0) {
            return Err(RfvpError::InvalidArgument);
        }
        if path.contains(':') || path.starts_with("/vol/") {
            Ok(String::from(path))
        } else if path.starts_with('/') {
            Ok(format!("sd:{path}"))
        } else {
            Ok(format!("sd:/{path}"))
        }
    }
}

impl Default for WiiUFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WiiUFile {
    handle: RawWiiUFileHandle,
}

impl WiiUFile {
    fn new(handle: RawWiiUFileHandle) -> Self {
        Self { handle }
    }
}

impl Drop for WiiUFile {
    fn drop(&mut self) {
        unsafe {
            rfvp_wiiu_platform_fs_close(self.handle);
        }
    }
}

impl RfvpFile for WiiUFile {
    fn len(&mut self) -> RfvpResult<u64> {
        let mut out_len = 0;
        let status = unsafe { rfvp_wiiu_platform_fs_len(self.handle, &mut out_len) };
        if status == 0 {
            Ok(out_len)
        } else {
            Err(wiiu_status_to_rfvp_error(status))
        }
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut out_read = 0;
        let status = unsafe {
            rfvp_wiiu_platform_fs_read_at(
                self.handle,
                offset,
                buf.as_mut_ptr(),
                buf.len(),
                &mut out_read,
            )
        };
        if status != 0 {
            return Err(wiiu_status_to_rfvp_error(status));
        }
        if out_read > buf.len() {
            return Err(RfvpError::Backend);
        }
        Ok(out_read)
    }
}

impl RfvpFileSystem for WiiUFileSystem {
    type File = WiiUFile;

    fn open(&mut self, path: &str) -> RfvpResult<Self::File> {
        let path = Self::wiiu_path(path)?;
        let mut handle = RawWiiUFileHandle::INVALID;
        let status = unsafe { rfvp_wiiu_platform_fs_open(path.as_ptr(), path.len(), &mut handle) };
        if status != 0 {
            return Err(wiiu_status_to_rfvp_error(status));
        }
        if handle == RawWiiUFileHandle::INVALID {
            return Err(RfvpError::Backend);
        }
        Ok(WiiUFile::new(handle))
    }

    fn write_all(&mut self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        let path = Self::wiiu_path(path)?;
        let status = unsafe {
            rfvp_wiiu_platform_fs_write_all(path.as_ptr(), path.len(), bytes.as_ptr(), bytes.len())
        };
        if status == 0 {
            Ok(())
        } else {
            Err(wiiu_status_to_rfvp_error(status))
        }
    }

    fn metadata(&mut self, path: &str) -> RfvpResult<RfvpFileInfo> {
        let path = Self::wiiu_path(path)?;
        let mut info = RawWiiUFileInfo {
            len: 0,
            kind: RawWiiUFileKind::Other,
        };
        let status =
            unsafe { rfvp_wiiu_platform_fs_metadata(path.as_ptr(), path.len(), &mut info) };
        if status != 0 {
            return Err(wiiu_status_to_rfvp_error(status));
        }
        Ok(raw_file_info_to_rfvp(info))
    }

    fn enumerate_by_extension(
        &mut self,
        root: &str,
        extension_without_dot: &str,
        visitor: &mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
    ) -> RfvpResult<()> {
        let root = Self::wiiu_path(root)?;
        let mut bridge = VisitorBridge { visitor };
        let status = unsafe {
            rfvp_wiiu_platform_fs_enumerate_by_extension(
                root.as_ptr(),
                root.len(),
                extension_without_dot.as_ptr(),
                extension_without_dot.len(),
                (&mut bridge as *mut VisitorBridge<'_>).cast::<c_void>(),
                enumerate_visitor_bridge,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(wiiu_status_to_rfvp_error(status))
        }
    }
}

fn raw_file_info_to_rfvp(info: RawWiiUFileInfo) -> RfvpFileInfo {
    RfvpFileInfo {
        len: info.len,
        kind: match info.kind {
            RawWiiUFileKind::File => RfvpFileKind::File,
            RawWiiUFileKind::Directory => RfvpFileKind::Directory,
            RawWiiUFileKind::Other => RfvpFileKind::Other,
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
    info: RawWiiUFileInfo,
) -> i32 {
    if visitor_ctx.is_null() || path.is_null() {
        return crate::status::WiiUStatus::InvalidArgument.as_i32();
    }
    let bridge = unsafe { &mut *visitor_ctx.cast::<VisitorBridge<'_>>() };
    let path_bytes = unsafe { core::slice::from_raw_parts(path, path_len) };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(value) => value,
        Err(_) => return crate::status::WiiUStatus::InvalidData.as_i32(),
    };
    match (bridge.visitor)(path, raw_file_info_to_rfvp(info)) {
        Ok(()) => crate::status::WiiUStatus::Ok.as_i32(),
        Err(err) => crate::status::rfvp_error_to_status(err),
    }
}
