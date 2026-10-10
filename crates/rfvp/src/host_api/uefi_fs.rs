//! The host file API over the UEFI simple file system of the volume the app was loaded from.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use uefi::boot;
use uefi::proto::media::file::{File as _, FileAttribute, FileMode, RegularFile};
use uefi::CString16;

use super::{RfvpError, RfvpFile, RfvpResult, RfvpStorage};

pub struct UefiFile(RegularFile);

impl UefiFile {
    fn open(path: &str) -> RfvpResult<Self> {
        let mut fs =
            boot::get_image_file_system(boot::image_handle()).map_err(|_| RfvpError::Io)?;
        let mut root = fs.open_volume().map_err(|_| RfvpError::Io)?;
        let path = CString16::try_from(path).map_err(|_| RfvpError::InvalidArgument)?;
        let handle = root
            .open(path.as_ref(), FileMode::Read, FileAttribute::empty())
            .map_err(|_| RfvpError::NotFound)?;
        handle.into_regular_file().map(Self).ok_or(RfvpError::NotFound)
    }
}

impl RfvpFile for UefiFile {
    fn len(&mut self) -> RfvpResult<u64> {
        self.0
            .set_position(RegularFile::END_OF_FILE)
            .map_err(|_| RfvpError::Io)?;
        self.0.get_position().map_err(|_| RfvpError::Io)
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        self.0.set_position(offset).map_err(|_| RfvpError::Io)?;
        self.0.read(buf).map_err(|_| RfvpError::Io)
    }
}

// SAFETY: UEFI boot services run on one processor; the handle is never used concurrently.
unsafe impl Send for UefiFile {}

/// A read-only directory on the boot volume, such as `\rfvp`. Paths are relative to it, with `/`
/// separators.
pub struct UefiDir {
    root: String,
}

impl UefiDir {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.trim_end_matches('\\').into(),
        }
    }

    fn resolve(&self, path: &str) -> String {
        format!("{}\\{}", self.root, path.trim_start_matches('/').replace('/', "\\"))
    }
}

impl RfvpStorage for UefiDir {
    fn read(&self, path: &str) -> RfvpResult<Vec<u8>> {
        UefiFile::open(&self.resolve(path))?.read_to_vec(usize::MAX)
    }

    fn write(&self, _path: &str, _bytes: &[u8]) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn exists(&self, path: &str) -> bool {
        UefiFile::open(&self.resolve(path)).is_ok()
    }

    fn remove(&self, _path: &str) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn list(&self, _dir: &str) -> RfvpResult<Vec<String>> {
        Err(RfvpError::Unsupported)
    }

    fn open(&self, path: &str) -> RfvpResult<Box<dyn RfvpFile + Send>> {
        Ok(Box::new(UefiFile::open(&self.resolve(path))?))
    }
}
