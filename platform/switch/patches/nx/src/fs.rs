//! FileSystem support

use crate::ipc::sf as ipc_sf;
use crate::ipc::sf::fsp::IDirectoryClient;
use crate::ipc::sf::fsp::IFileClient;
use crate::ipc::sf::fsp::IFileSystemClient;
use crate::result::*;
use crate::service;
use crate::service::fsp;
use crate::service::fsp::srv::IFileSystemProxyClient;
use crate::sync::RwLock;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::mem as cmem;
use core::ops::DerefMut;
use embedded_io::ErrorType;
pub use embedded_io::SeekFrom;
pub use embedded_io::Write;

pub mod rc;

// TODO: define this types here and alias them in fsp-srv?

pub use fsp::fsp_sf::DirectoryEntry;
pub use fsp::fsp_sf::DirectoryEntryType;
pub use fsp::fsp_sf::DirectoryOpenMode;
pub use fsp::fsp_sf::FileAttribute;
pub use fsp::fsp_sf::FileOpenMode;
pub use fsp::fsp_sf::FileQueryRangeInfo;
pub use fsp::fsp_sf::FileReadOption;
pub use fsp::fsp_sf::FileTimeStampRaw;
pub use fsp::fsp_sf::FileWriteOption;
pub use fsp::fsp_sf::OperationId;
pub use fsp::fsp_sf::QueryId;

/// Represents a file, abstracted from the IPC client API.
pub trait File: Sync {
    /// Reads data from the file, returning the actual read size.
    ///
    /// # Arguments:
    ///
    /// * `offset`: The absolute offset.
    /// * `out_buf`: The output slice to fill.
    /// * `option`: [`FileReadOption`] for file reading flags.
    fn read(&mut self, offset: usize, out_buf: &mut [u8], option: FileReadOption) -> Result<usize>;

    /// Writes data to a file (this one doesn't return the actual written size, thanks N).
    ///
    /// # Arguments.
    ///
    /// * `offset`: The absolute offset.
    /// * `buf`: The input data to write into the file.
    /// * `option`: [`FileWriteOption`] value.
    fn write(&mut self, offset: usize, buf: &[u8], option: FileWriteOption) -> Result<()>;

    /// Flushes the pending file writes.
    fn flush(&self) -> Result<()>;

    /// Sets the file size.
    ///
    /// This effectively truncates the file.
    ///
    /// # Arguments.
    ///
    /// * `size`: The new file size.
    fn set_size(&mut self, size: usize) -> Result<()>;

    /// Gets the current file size.
    fn get_size(&mut self) -> Result<usize>;

    /// Performs a range-operation on the file, returning corresponding result data.
    ///
    /// # Arguments:
    ///
    /// * `operation_id`: The ID of the file operation to perform on the specified range.
    /// * `offset`: The absolute offset.
    /// * `size`: The file data size in which to operate. i.e. we are operating in the range `[offset, offset+size)`.
    fn operate_range(
        &mut self,
        operation_id: OperationId,
        offset: usize,
        size: usize,
    ) -> Result<FileQueryRangeInfo>;

    /// Performs a range-operation on the file with custom input/output data.
    ///
    /// # Arguments:
    ///
    /// * `operation_id`: The ID of the file operation to perform on the specified range.
    /// * `offset`: The absolute offset.
    /// * `size`: The file data size in which to operate. i.e. we are operating in the range `[offset, offset+size)`.
    /// * `in_buf`: Input data buffer.
    /// * `out_buf`: Output data buffer.
    fn operate_range_with_buffer(
        &mut self,
        operation_id: OperationId,
        offset: usize,
        size: usize,
        in_buf: &[u8],
        out_buf: &mut [u8],
    ) -> Result<()>;
}

/// Represents a directory.
pub trait Directory: Sync {
    /// Reads existing entries, returning the actual number of read entries.
    ///
    /// The max number of entries to read is determined by the output slice size and the actually existing entry count.
    ///
    /// # Arguments:
    ///
    /// * `out_entries`: The out [`DirectoryEntry`] slice to fill.
    fn read(&self, out_entries: &mut [DirectoryEntry]) -> Result<usize>;

    /// Gets the [`Directory`]'s entry count.
    fn get_entry_count(&self) -> Result<u64>;
}

/// Represents a filesystem.
pub trait FileSystem: Sync {
    /// Creates a file.
    ///
    /// # Arguments:
    ///
    /// * `path`: The file path to create.
    /// * `size`: The initial file size.
    /// * `attribute`: The file attribute flags.
    fn create_file(&self, path: &str, attribute: FileAttribute, size: usize) -> Result<()>;

    /// Deletes a file.
    ///
    /// # Arguments:
    ///
    /// * `path`: The file path to delete.
    fn remove_file(&self, path: &str) -> Result<()>;

    /// Creates a directory.
    ///
    /// # Arguments.
    ///
    /// * `path`: The directory path to create.
    fn create_directory(&self, path: &str) -> Result<()>;

    /// Deletes a directory.
    ///
    /// # Arguments.
    ///
    /// * `path`: The directory path to delete.
    fn remove_dir(&self, path: &str) -> Result<()>;

    /// Deletes a directory and all its children files/directories.
    ///
    /// # Arguments:
    ///
    /// * `path`: The directory to recursively remove.
    fn remove_dir_all(&self, path: &str) -> Result<()>;

    /// Renames a file.
    ///
    /// # Arguments.
    ///
    /// * `old_path`: The current file name/path.
    /// * `new_path`: The new file name/path.
    fn rename_file(&self, old_path: &str, new_path: &str) -> Result<()>;

    /// Renames a directory.
    ///
    /// # Arguments.
    ///
    /// * `old_path`: The current directory path.
    /// * `new_path`: The new directory path.
    fn rename_directory(&self, old_path: &str, new_path: &str) -> Result<()>;

    /// Gets a path's [`DirectoryEntryType`].
    ///
    /// # Arguments.
    ///
    /// * `path`: The path we are checking the entity type of.
    fn get_entry_type(&self, path: &str) -> Result<DirectoryEntryType>;

    /// Opens a [`File`].
    ///
    /// # Arguments:
    ///
    /// * `path`: The file path to open.
    /// * `mode`: The open mode.
    fn open_file(&self, path: &str, mode: FileOpenMode) -> Result<Box<dyn File>>;

    /// Opens a [`Directory`].
    ///
    /// # Arguments:
    ///
    /// * `path`: The directory path to open.
    /// * `mode`: The open mode.
    fn open_directory(&self, path: &str, mode: DirectoryOpenMode) -> Result<Box<dyn Directory>>;

    /// Commits the filesystem, flushing pending writes.
    fn commit(&self) -> Result<()>;

    /// Gets the free space size at a given path.
    ///
    /// # Argument.
    ///
    /// * `path`: The path to check.
    fn get_free_space_size(&self, path: &str) -> Result<usize>;

    /// Gets the total space size at a given path.
    ///
    /// # Arguments:
    ///
    /// * `path`: The path to use.
    fn get_total_space_size(&self, path: &str) -> Result<usize>;

    /// Deletes all the children files/directories inside a directory.
    ///
    /// # Arguments:
    ///
    /// * `path`: The path to use.
    fn remove_children_all(&self, path: &str) -> Result<()>;

    /// Gets the [`FileTimeStampRaw`] of a file.
    ///
    /// # Arguments:
    ///
    /// * `path`: The path to use.
    fn get_file_time_stamp_raw(&self, path: &str) -> Result<FileTimeStampRaw>;

    /// Queries on a path.
    ///
    /// # Arguments:
    ///
    /// * `query_id`: The [`QueryId`].
    /// * `in_buf`: Input data.
    /// * `out_buf`: Output data.
    fn query_entry(
        &self,
        path: &str,
        query_id: QueryId,
        in_buf: &[u8],
        out_buf: &mut [u8],
    ) -> Result<()>;
}

/// Represents a wrapper [`File`] implementation to translate IPC [`IFileClient`] objects to [`File`] objects.
pub struct ProxyFile {
    file_obj: Box<dyn IFileClient>,
}

// TODO: Remove. This fixes a problem in emuiibo but this whole construct is probably not needed.
unsafe impl Sync for ProxyFile {}
unsafe impl Send for ProxyFile {}

impl ProxyFile {
    /// Creates a new [`ProxyFile`] from a [`IFileClient`] shared object.
    ///
    /// # Arguments:
    ///
    /// * `file_obj`: The IPC [`IFileClient`] implementation to wrap.
    pub fn new(file: impl IFileClient + 'static) -> Self {
        Self {
            file_obj: Box::new(file),
        }
    }
}

impl From<Box<dyn IFileClient>> for ProxyFile {
    fn from(value: Box<dyn IFileClient>) -> Self {
        Self { file_obj: value }
    }
}

impl File for ProxyFile {
    fn read(&mut self, offset: usize, out_buf: &mut [u8], option: FileReadOption) -> Result<usize> {
        self.file_obj.read(
            option,
            offset,
            out_buf.len(),
            ipc_sf::Buffer::from_mut_array(out_buf),
        )
    }

    fn write(&mut self, offset: usize, buf: &[u8], option: FileWriteOption) -> Result<()> {
        self.file_obj
            .write(option, offset, buf.len(), ipc_sf::Buffer::from_array(buf))
    }

    fn flush(&self) -> Result<()> {
        self.file_obj.flush()
    }

    fn set_size(&mut self, size: usize) -> Result<()> {
        self.file_obj.set_size(size)
    }

    fn get_size(&mut self) -> Result<usize> {
        self.file_obj.get_size()
    }

    fn operate_range(
        &mut self,
        operation_id: OperationId,
        offset: usize,
        size: usize,
    ) -> Result<FileQueryRangeInfo> {
        self.file_obj.operate_range(operation_id, offset, size)
    }

    fn operate_range_with_buffer(
        &mut self,
        operation_id: OperationId,
        offset: usize,
        size: usize,
        in_buf: &[u8],
        out_buf: &mut [u8],
    ) -> Result<()> {
        self.file_obj.operate_range_with_buffer(
            operation_id,
            offset,
            size,
            ipc_sf::Buffer::from_array(in_buf),
            ipc_sf::Buffer::from_mut_array(out_buf),
        )
    }
}

/// Represents a wrapper [`Directory`] implementation to translate IPC [`IDirectoryClient`] objects to [`Directory`] objects.
#[derive(Clone)]
pub struct ProxyDirectory {
    dir_obj: Arc<dyn IDirectoryClient>,
}

// TODO: Remove because we don't actually have a guarantee that IDirectoryClient is Sync
unsafe impl Sync for ProxyDirectory {}
unsafe impl Send for ProxyDirectory {}

impl From<Arc<dyn IDirectoryClient>> for ProxyDirectory {
    fn from(value: Arc<dyn IDirectoryClient>) -> Self {
        Self { dir_obj: value }
    }
}

impl ProxyDirectory {
    /// Creates a new [`ProxyDirectory`] from a [`IDirectoryClient`] shared object
    ///
    /// # Arguments
    ///
    /// * `dir_obj`: The IPC [`IDirectoryClient`] object to wrap
    pub fn new(dir: impl IDirectoryClient + 'static) -> Self {
        Self {
            dir_obj: Arc::new(dir),
        }
    }
}

impl Directory for ProxyDirectory {
    fn read(&self, out_entries: &mut [DirectoryEntry]) -> Result<usize> {
        self.dir_obj
            .read(ipc_sf::Buffer::from_mut_array(out_entries))
            .map(|r| r as usize)
    }

    fn get_entry_count(&self) -> Result<u64> {
        self.dir_obj.get_entry_count()
    }
}

/// Represents a wrapper [`FileSystem`] implementation to translate IPC [`IFileSystemClient`] objects to [`FileSystem`] objects
#[derive(Clone)]
pub struct ProxyFileSystem {
    fs_obj: Arc<dyn IFileSystemClient>,
}

unsafe impl Send for ProxyFileSystem {}
unsafe impl Sync for ProxyFileSystem {}

impl ProxyFileSystem {
    /// Creates a new [`ProxyFileSystem`] from a [`IFileSystemClient`] shared object
    ///
    /// # Arguments
    ///
    /// * `fs_obj`: The IPC [`IFileSystemClient`] object to wrap
    pub fn new(fs_obj: Arc<dyn IFileSystemClient>) -> Self {
        Self { fs_obj }
    }
}

impl FileSystem for ProxyFileSystem {
    fn create_file(&self, path: &str, attribute: FileAttribute, size: usize) -> Result<()> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .create_file(attribute, size, ipc_sf::Buffer::from_var(&sf_path))
    }

    fn remove_file(&self, path: &str) -> Result<()> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj.delete_file(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn create_directory(&self, path: &str) -> Result<()> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .create_directory(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn remove_dir(&self, path: &str) -> Result<()> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .delete_directory(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn remove_dir_all(&self, path: &str) -> Result<()> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .delete_directory_recursively(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn get_entry_type(&self, path: &str) -> Result<DirectoryEntryType> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .get_entry_type(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn rename_file(&self, old_path: &str, new_path: &str) -> Result<()> {
        let sf_old_path = fsp::fsp_sf::Path::from_str(old_path);
        let sf_new_path = fsp::fsp_sf::Path::from_str(new_path);
        self.fs_obj.rename_file(
            ipc_sf::Buffer::from_var(&sf_old_path),
            ipc_sf::Buffer::from_var(&sf_new_path),
        )
    }

    fn rename_directory(&self, old_path: &str, new_path: &str) -> Result<()> {
        let sf_old_path = fsp::fsp_sf::Path::from_str(old_path);
        let sf_new_path = fsp::fsp_sf::Path::from_str(new_path);
        self.fs_obj.rename_directory(
            ipc_sf::Buffer::from_var(&sf_old_path),
            ipc_sf::Buffer::from_var(&sf_new_path),
        )
    }

    fn open_file(&self, path: &str, mode: FileOpenMode) -> Result<Box<dyn File>> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        let file_obj = self
            .fs_obj
            .open_file(mode, ipc_sf::Buffer::from_var(&sf_path))?;
        Ok(Box::new(ProxyFile::new(file_obj)))
    }

    fn open_directory(&self, path: &str, mode: DirectoryOpenMode) -> Result<Box<dyn Directory>> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        let dir_obj = self
            .fs_obj
            .open_directory(mode, ipc_sf::Buffer::from_var(&sf_path))?;
        Ok(Box::new(ProxyDirectory::new(dir_obj)))
    }

    fn commit(&self) -> Result<()> {
        self.fs_obj.commit()
    }

    fn get_free_space_size(&self, path: &str) -> Result<usize> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .get_free_space_size(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn get_total_space_size(&self, path: &str) -> Result<usize> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .get_total_space_size(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn remove_children_all(&self, path: &str) -> Result<()> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .clean_directory_recursively(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn get_file_time_stamp_raw(&self, path: &str) -> Result<FileTimeStampRaw> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj
            .get_file_time_stamp_raw(ipc_sf::Buffer::from_var(&sf_path))
    }

    fn query_entry(
        &self,
        path: &str,
        query_id: QueryId,
        in_buf: &[u8],
        out_buf: &mut [u8],
    ) -> Result<()> {
        let sf_path = fsp::fsp_sf::Path::from_str(path);
        self.fs_obj.query_entry(
            ipc_sf::Buffer::from_var(&sf_path),
            query_id,
            ipc_sf::Buffer::from_array(in_buf),
            ipc_sf::Buffer::from_mut_array(out_buf),
        )
    }
}

/// Represents a wrapper type to simplify file access, tracking the currently seek-ed location in the file.
pub struct FileAccessor {
    file: Box<dyn File>,
    offset: usize,
}

// we can do this because we never leak the `file` field, which would require us to also require `Send` on the trait `File`
unsafe impl Sync for FileAccessor {}
unsafe impl Send for FileAccessor {}

impl From<Box<dyn File>> for FileAccessor {
    fn from(value: Box<dyn File>) -> Self {
        Self {
            file: value,
            offset: 0,
        }
    }
}

impl FileAccessor {
    /// Creates a new [`FileAccessor`] from a given [`File`] shared object.
    ///
    /// # Arguments.
    ///
    /// * `file`: The `File` implementor.
    pub fn new(file: impl File + 'static) -> Self {
        Self {
            file: Box::new(file),
            offset: 0,
        }
    }

    /// Gets the file size.
    pub fn get_size(&mut self) -> Result<usize> {
        self.file.get_size()
    }

    /// Seeks in the file to a certain offset.
    ///
    /// # Arguments:
    ///
    /// * `offset`: The offset to seek to.
    pub fn seek(&mut self, pos: SeekFrom) -> Result<()> {
        match pos {
            SeekFrom::Start(offset) => self.offset = offset as _,
            SeekFrom::Current(offset) => {
                self.offset = self.offset.saturating_add_signed(offset as _)
            }
            SeekFrom::End(offset) => {
                let size = self.get_size()?;
                self.offset = size.saturating_add_signed(offset as _);
            }
        };
        Ok(())
    }

    /// Reads data into the given array.
    ///
    /// # Arguments:
    ///
    /// * `out_arr`: The output array.
    pub fn read_array<T: Copy>(&mut self, out_arr: &mut [T]) -> Result<usize> {
        // SAFETY: This is safe as we're constructing the slice from another valid slice of `T: Copy`.
        let read_size = self.file.read(
            self.offset,
            unsafe {
                core::slice::from_raw_parts_mut(
                    out_arr.as_mut_ptr() as _,
                    cmem::size_of_val(out_arr),
                )
            },
            FileReadOption::None(),
        )?;
        self.offset += read_size;
        Ok(read_size)
    }

    /// Reads a value.
    pub fn read_val<T: Copy>(&mut self) -> Result<T> {
        let mut t = unsafe { cmem::zeroed::<T>() };
        let read_size = self.file.read(
            self.offset,
            unsafe { core::slice::from_raw_parts_mut(&mut t as *mut T as _, cmem::size_of::<T>()) },
            FileReadOption::None(),
        )?;
        self.offset += read_size;
        Ok(t)
    }

    /// Writes data from the given array
    ///
    /// # Arguments
    ///
    /// * `arr`: The input array
    pub fn write_array<T: Copy, const FLUSH: bool>(&mut self, arr: &[T]) -> Result<()> {
        let transmuted: &[u8] =
            unsafe { core::slice::from_raw_parts(arr.as_ptr() as _, core::mem::size_of_val(arr)) };
        self.file.write(
            self.offset,
            transmuted,
            if FLUSH {
                FileWriteOption::Flush()
            } else {
                FileWriteOption::None()
            },
        )?;
        self.offset += transmuted.len();
        Ok(())
    }

    /// Writes a value
    ///
    /// # Arguments
    ///
    /// * `t`: The value to write
    pub fn write_val<T: Copy, const FLUSH: bool>(&mut self, t: &T) -> Result<()> {
        let transmuted = unsafe {
            core::slice::from_raw_parts(t as *const T as *const u8, cmem::size_of::<T>())
        };

        self.file.write(
            self.offset,
            transmuted,
            if FLUSH {
                FileWriteOption::Flush()
            } else {
                FileWriteOption::None()
            },
        )?;
        self.offset += transmuted.len();
        Ok(())
    }

    /// Flushes reads/writes to the underlying file.
    ///
    /// It is provided, but it is not currently required as all writes to [`FileAccessor`] send the `Flush` flag.
    pub fn flush(&self) -> Result<()> {
        self.file.flush()
    }
}

impl ErrorType for FileAccessor {
    type Error = ResultCode;
}

impl embedded_io::Read for FileAccessor {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.len() == 0 {
            return Ok(0);
        }

        self.read_array(buf)
    }
}

impl embedded_io::Write for FileAccessor {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.len() == 0 {
            return Ok(0);
        }

        self.write_array::<u8, false>(buf).map(|_| buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        FileAccessor::flush(&*self)
    }
}

impl core::fmt::Write for FileAccessor {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_array::<u8, true>(s.as_bytes())
            .map_err(|_| core::fmt::Error)
    }
}
/// Represents a wrapper type to simplify directory access
pub struct DirectoryAccessor {
    dir: Arc<dyn Directory>,
}

impl From<Arc<dyn Directory>> for DirectoryAccessor {
    fn from(value: Arc<dyn Directory>) -> Self {
        Self { dir: value }
    }
}

impl DirectoryAccessor {
    /// Creates a new [`DirectoryAccessor`] from a given [`Directory`] shared object
    ///
    /// # Arguments
    ///
    /// * `dir`: The shared object
    pub fn new(dir: impl Directory + 'static) -> Self {
        Self { dir: Arc::new(dir) }
    }

    /// Gets the directory entry count.
    pub fn get_entry_count(&mut self) -> Result<u64> {
        self.dir.get_entry_count()
    }

    /// Gets the underlying [`Directory`] shared object.
    pub fn get_object(&self) -> Arc<dyn Directory> {
        self.dir.clone()
    }

    /// Tries to read the next entry.
    ///
    /// Note that if the end is reached this will return `Ok(None)`, the result reflects other possible inner I/O errors.
    pub fn read_next(&mut self) -> Result<Option<DirectoryEntry>> {
        let mut entries: [DirectoryEntry; 1] = Default::default();
        let read_count = self.dir.read(&mut entries)?;
        if read_count == 1 {
            Ok(Some(entries[0]))
        } else {
            Ok(None)
        }
    }
}

pub(crate) struct FileSystemDevice {
    mount_name: String,
    fs: Arc<dyn FileSystem>,
}

impl FileSystemDevice {
    pub fn new(mount_name: String, fs: Arc<dyn FileSystem>) -> Self {
        Self { mount_name, fs }
    }
}

unsafe impl Sync for FileSystemDevice {}
unsafe impl Send for FileSystemDevice {}

pub(crate) static G_DEVICES: RwLock<Vec<FileSystemDevice>> = RwLock::new(Vec::new());

fn find_device_by_name(name: &str) -> Result<Arc<dyn FileSystem>> {
    let device_guard = G_DEVICES.read();
    for device in device_guard.iter() {
        if device.mount_name.as_str() == name {
            return Ok(device.fs.clone());
        }
    }
    rc::ResultDeviceNotFound::make_err()
}

static G_FSPSRV_SESSION: RwLock<Option<Arc<fsp::srv::FileSystemProxyService>>> = RwLock::new(None);

define_bit_set! {
    /// Represents options for opening files
    FileOpenOption (u32) {
        None = 0,
        Create = bit!(0),
        Read = bit!(1),
        Write = bit!(2),
        Append = bit!(3)
    }
}

/// Initializes `fsp-srv` support instantiating a [`FileSystemProxyService`][`fsp::srv::FileSystemProxyService`] shared object
#[inline]
pub fn initialize_fspsrv_session() -> Result<()> {
    let mut guard = G_FSPSRV_SESSION.write();
    debug_assert!(guard.is_none(), "Double initializing FSP session");
    *guard = Some(Arc::new(service::new_service_object::<
        fsp::srv::FileSystemProxyService,
    >()?));
    Ok(())
}

/// Gets whether `fsp-srv` support was initialized
#[inline]
pub fn is_fspsrv_session_initialized() -> bool {
    G_FSPSRV_SESSION.read().is_some()
}

/// Finalizes `fsp-srv` support
#[inline]
pub(crate) fn finalize_fspsrv_session() {
    G_FSPSRV_SESSION.write().take();
}

/// Gets the global [`IFileSystemProxyClient`] shared object used for `fsp-srv` support
#[inline]
pub fn get_fspsrv_session() -> Result<Arc<fsp::srv::FileSystemProxyService>> {
    G_FSPSRV_SESSION
        .read()
        .as_ref()
        .map(Clone::clone)
        .ok_or(super::rc::ResultNotInitialized::make())
}

/// Mounts a [`FileSystem`]
///
/// Paths inside the filesystem will be accessible as `<name>:/<path>` with fns like [`open_file`], etc.
///
/// # Arguments
///
/// * `name`: The mount name
/// * `fs`: The [`FileSystem`] shared object
pub fn mount(name: &str, fs: Arc<dyn FileSystem>) {
    G_DEVICES
        .write()
        .push(FileSystemDevice::new(String::from(name), fs));
}

/// Mounts an IPC [`IFileSystemClient`]
///
/// Essentially creates a [`ProxyFileSystem`] and [`mount`]s it
///
/// # Arguments
///
/// * `name`: The mount name
/// * `fs_obj`: The [`IFileSystemClient`] shared object
pub fn mount_fsp_filesystem(name: &str, fs_obj: Arc<dyn IFileSystemClient>) {
    let proxy_fs = Arc::new(ProxyFileSystem::new(fs_obj));
    mount(name, proxy_fs);
}

/// Mounts the system's SD card using `fsp-srv` support
///
/// This will fail with [`ResultNotInitialized`][`super::rc::ResultNotInitialized`] if `fsp-srv` support isn't initialized
///
/// # Arguments
///
/// * `name`: The name of the mount that we store for the sdcard
pub fn mount_sd_card(name: &str) -> Result<()> {
    let sd_fs_obj = get_fspsrv_session()?.open_sd_card_filesystem()?;
    mount_fsp_filesystem(name, Arc::new(sd_fs_obj));
    Ok(())
}

/// Unmounts a mounted filesystem
///
/// Note that this does nothing if there is no mounted filesystem with the given name
///
/// # Arguments
///
/// * `mount_name`: The mount name
pub fn unmount(mount_name: &str) {
    G_DEVICES
        .write()
        .deref_mut()
        .retain(|dev| dev.mount_name.as_str() != mount_name);
}

/// Unmounts all filesystems
pub fn unmount_all() {
    G_DEVICES.write().deref_mut().clear();
}

/// Returns the [`FileSystem`] corresponding to a given path
///
/// If there is a filesystem mounted as `demo`, calling this with `"demo:/anything"` will return an instance to that mounted filesystem
///
/// # Arguments
///
/// * `path`: The path to use
pub fn get_path_filesystem(path: &str) -> Result<Arc<dyn FileSystem>> {
    let split = path.find(':').ok_or(rc::ResultDeviceNotFound::make())?;
    let fs = find_device_by_name(&path[..split])?;
    Ok(fs)
}

/// Returns the [`FileSystem`] and the processed path corresponding to a given path
///
/// If there is a filesystem mounted as `demo`, calling this with `"demo:/anything"` will return an instance to that mounted filesystem and `"anything"` as the processed path
///
/// # Arguments
///
/// * `path`: The path to use
pub fn format_path(path: &str) -> Result<(Arc<dyn FileSystem>, &str)> {
    let split = path.find(':');
    let split = split.ok_or(rc::ResultDeviceNotFound::make())?;
    let fs = find_device_by_name(&path[..split])?;

    Ok((fs, &path[split + 1..]))
}

/// Creates a file
///
/// # Arguments
///
/// * `path`: The path to use
/// * `size`: The initial file size, default/IPC behavior is to fill the file with zeros
/// * `attribute`: The file attribute, default/IPC behavior uses this to allow creating "concatenation files" (allowing 32GB+ files in FAT32 filesystems)
pub fn create_file(path: &str, size: usize, attribute: FileAttribute) -> Result<()> {
    let (fs, processed_path) = format_path(path)?;
    fs.create_file(processed_path, attribute, size)
}

/// Deletes a file
///
/// # Arguments
///
/// * `path`: The path to use
pub fn remove_file(path: &str) -> Result<()> {
    let (fs, processed_path) = format_path(path)?;
    fs.remove_file(processed_path)
}

/// Creates a directory
///
/// # Arguments
///
/// * `path`: The path to use
pub fn create_directory(path: &str) -> Result<()> {
    let (fs, processed_path) = format_path(path)?;
    fs.create_directory(processed_path)
}

/// Deletes a directory
///
/// Note that (in default/IPC behavior) this won't succeed unless the directory is empty (see [`remove_dir_all`])
///
/// # Arguments
///
/// * `path`: The path to use
pub fn remove_dir(path: &str) -> Result<()> {
    let (fs, processed_path) = format_path(path)?;
    fs.remove_dir(processed_path)
}

/// Deletes a directory and all its children files/directories
///
/// # Arguments
///
/// * `path`: The path to use
pub fn remove_dir_all(path: &str) -> Result<()> {
    let (fs, processed_path) = format_path(path)?;
    fs.remove_dir_all(processed_path)
}

/// Deletes all the children files/directories inside a directory
///
/// # Arguments
///
/// * `path`: The path to use
pub fn remove_children_all(path: &str) -> Result<()> {
    let (fs, processed_path) = format_path(path)?;
    fs.remove_children_all(processed_path)
}

/// Gets a path's [`DirectoryEntryType`]
///
/// This can be use to easily check if a file/directory exists, or whether they actually are a file or a directory
///
/// # Arguments
///
/// * `path`: The path to use
pub fn get_entry_type(path: &str) -> Result<DirectoryEntryType> {
    let (fs, processed_path) = format_path(path)?;
    fs.get_entry_type(processed_path)
}

/// Converts a [`FileOpenOption`] to a [`FileOpenMode`]
///
/// # Arguments
///
/// * `option`: Input option
pub fn convert_file_open_option_to_mode(option: FileOpenOption) -> FileOpenMode {
    let mut mode = FileOpenMode::None();
    if option.contains(FileOpenOption::Read()) {
        mode |= FileOpenMode::Read();
    }
    if option.contains(FileOpenOption::Write()) {
        mode |= FileOpenMode::Write();
    }
    if option.contains(FileOpenOption::Append()) {
        mode |= FileOpenMode::Append();
    }
    mode
}

/// Converts a [`FileOpenMode`] to a [`FileOpenOption`]
///
/// # Arguments
///
/// * `mode`: Input mode
pub fn convert_file_open_mode_to_option(mode: FileOpenMode) -> FileOpenOption {
    let mut option = FileOpenOption::None();
    if mode.contains(FileOpenMode::Read()) {
        option |= FileOpenOption::Read();
    }
    if mode.contains(FileOpenMode::Write()) {
        option |= FileOpenOption::Write();
    }
    if mode.contains(FileOpenMode::Append()) {
        option |= FileOpenOption::Append();
    }
    option
}

/// Renames a file
///
/// # Arguments
///
/// * `old_path`: The old path to use
/// * `new_path`: The new path to use
pub fn rename_file(old_path: &str, new_path: &str) -> Result<()> {
    let (old_fs, processed_old_path) = format_path(old_path)?;
    let (new_fs, processed_new_path) = format_path(new_path)?;
    result_return_unless!(
        Arc::<dyn FileSystem>::ptr_eq(&old_fs, &new_fs),
        rc::ResultNotInSameFileSystem
    );

    old_fs.rename_file(processed_old_path, processed_new_path)
}

/// Renames a directory
///
/// # Arguments
///
/// * `old_path`: The old path to use
/// * `new_path`: The new path to use
pub fn rename_directory(old_path: &str, new_path: &str) -> Result<()> {
    let (old_fs, processed_old_path) = format_path(old_path)?;
    let (new_fs, processed_new_path) = format_path(new_path)?;
    result_return_unless!(
        Arc::<dyn FileSystem>::ptr_eq(&old_fs, &new_fs),
        rc::ResultNotInSameFileSystem
    );

    old_fs.rename_directory(processed_old_path, processed_new_path)
}

/// Renames a file/directory
///
/// Essentially is a wrapper for checking the entry type and calling [`rename_file`] or [`rename_directory`] according to that
///
/// Note that, to minimize overhead, this should only be used if the entry type isn't known beforehand
///
/// # Arguments
///
/// * `old_path`: The old path to use
/// * `new_path`: The new path to use
pub fn rename(old_path: &str, new_path: &str) -> Result<()> {
    let (old_fs, processed_old_path) = format_path(old_path)?;
    let (new_fs, processed_new_path) = format_path(new_path)?;
    result_return_unless!(
        Arc::<dyn FileSystem>::ptr_eq(&old_fs, &new_fs),
        rc::ResultNotInSameFileSystem
    );

    match old_fs.get_entry_type(processed_old_path)? {
        DirectoryEntryType::Directory => {
            old_fs.rename_directory(processed_old_path, processed_new_path)
        }
        DirectoryEntryType::File => old_fs.rename_file(processed_old_path, processed_new_path),
    }
}

/// Opens a file as a [`FileAccessor`]
///
/// # Arguments
///
/// * `path`: The path to use
/// * `option`: The open option
pub fn open_file(path: &str, option: FileOpenOption) -> Result<FileAccessor> {
    let (fs, processed_path) = format_path(path)?;

    let mode = convert_file_open_option_to_mode(option);
    let mut file = match fs.open_file(processed_path, mode) {
        Ok(file) => file,
        Err(rc) => {
            if fsp::fsp_sf::rc::ResultPathNotFound::matches(rc)
                && option.contains(FileOpenOption::Create())
            {
                // Create the file if it doesn't exist and we were told to do so
                fs.create_file(processed_path, FileAttribute::None(), 0)?;
                fs.open_file(processed_path, mode)?
            } else {
                return Err(rc);
            }
        }
    };

    let offset: u64 = match option.contains(FileOpenOption::Append()) {
        true => file.get_size().unwrap_or(0) as _,
        false => 0,
    };

    // convert the Boxed file to a FileAccessor
    let mut file_acc: FileAccessor = file.into();
    file_acc.seek(SeekFrom::Start(offset))?;
    Ok(file_acc)
}

/// Opens a directory as a [`DirectoryAccessor`]
///
/// # Arguments
///
/// * `path`: The path to use
/// * `mode`: The open mode
pub fn open_directory(path: &str, mode: DirectoryOpenMode) -> Result<DirectoryAccessor> {
    let (fs, processed_path) = format_path(path)?;

    let dir = fs.open_directory(processed_path, mode)?;
    let dir: Arc<dyn Directory> = dir.into();
    Ok(dir.into())
}

/// Commits on a filesystem
///
/// The only part of the path used is the filesystem mount name (to determine the filesystem to use)
///
/// # Argument
///
/// * `path`: The path to use
pub fn commit(path: &str) -> Result<()> {
    let (fs, _) = format_path(path)?;
    fs.commit()
}

/// Gets the free space size at a given path
///
/// # Argument
///
/// * `path`: The path to use
pub fn get_free_space_size(path: &str) -> Result<usize> {
    let (fs, processed_path) = format_path(path)?;

    fs.get_free_space_size(processed_path)
}

/// Gets the total space size at a given path
///
/// # Argument
///
/// * `path`: The path to use
pub fn get_total_space_size(path: &str) -> Result<usize> {
    let (fs, processed_path) = format_path(path)?;

    fs.get_total_space_size(processed_path)
}

/// Gets the [`FileTimeStampRaw`] of a file
///
/// # Arguments
///
/// * `path`: The path to use
pub fn get_file_time_stamp_raw(path: &str) -> Result<FileTimeStampRaw> {
    let (fs, processed_path) = format_path(path)?;

    fs.get_file_time_stamp_raw(processed_path)
}

/// Queries on a path
///
/// # Arguments
///
/// * `query_id`: The [`QueryId`]
/// * `in_buf`: Input data
/// * `out_buf`: Output data
pub fn query_entry(path: &str, query_id: QueryId, in_buf: &[u8], out_buf: &mut [u8]) -> Result<()> {
    let (fs, processed_path) = format_path(path)?;

    fs.query_entry(processed_path, query_id, in_buf, out_buf)
}

/// Sets the "concatenation file" attribute on a file.
///
/// This essentially is a special case of [`query_entry`] to set an existing file as a "concatenation file".
///
/// # Arguments:
///
/// * `path`: The path to use
#[inline]
pub fn set_concatenation_file_attribute(path: &str) -> Result<()> {
    query_entry(path, QueryId::SetConcatenationFileAttribute, &[], &mut [])
}

pub mod subdir;

//pub mod pfs0;

//pub mod nca;
