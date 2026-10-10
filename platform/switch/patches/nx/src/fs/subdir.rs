//! Helper object types for treating FS subdirectories as IPC filesystems

use crate::fs;
use crate::result::*;
use alloc::string::ToString;
use alloc::{boxed::Box, string::String};

// TODO: subdir FS object, non-IPC version?

/// Represents an  [`FileSystem`][`fs::FileSystem`] object wrapping around a FS subdirectory path
pub struct SubDir<FS: fs::FileSystem> {
    fs: FS,
    base: String,
}

impl<FS: fs::FileSystem> SubDir<FS> {
    /// Creates a new [`SubDir`] from a [`FileSystem`][`fs::FileSystem`] instance.
    ///
    /// # Arguments
    ///
    /// * `sub_dir`: The subdirectory path to wrap
    #[inline]
    pub fn new(fs: FS, dir: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            fs,
            base: dir.as_ref().to_string(),
        })
    }

    fn make_path(&self, path: &str) -> String {
        let mut out = self.base.clone();
        out.push_str(path);
        out
    }
}

impl<FS: fs::FileSystem> fs::FileSystem for SubDir<FS> {
    fn commit(&self) -> Result<()> {
        self.fs.commit()
    }

    fn create_directory(&self, path: &str) -> Result<()> {
        self.fs.create_directory(self.make_path(path).as_str())
    }

    fn create_file(&self, path: &str, attribute: fs::FileAttribute, size: usize) -> Result<()> {
        self.fs
            .create_file(self.make_path(path).as_str(), attribute, size)
    }

    fn get_entry_type(&self, path: &str) -> Result<fs::DirectoryEntryType> {
        self.fs.get_entry_type(self.make_path(path).as_str())
    }

    fn get_file_time_stamp_raw(&self, path: &str) -> Result<fs::FileTimeStampRaw> {
        self.fs
            .get_file_time_stamp_raw(self.make_path(path).as_str())
    }

    fn get_free_space_size(&self, path: &str) -> Result<usize> {
        // we don't need to make a new path here, as the path is guaranteed to be in the same volume
        self.fs.get_free_space_size(path)
    }

    fn get_total_space_size(&self, path: &str) -> Result<usize> {
        // we don't need to make a new path here, as the path is guaranteed to be in the same volume
        self.fs.get_total_space_size(path)
    }

    fn open_directory(
        &self,
        path: &str,
        mode: fs::DirectoryOpenMode,
    ) -> Result<Box<dyn fs::Directory>> {
        self.fs.open_directory(self.make_path(path).as_str(), mode)
    }

    fn open_file(&self, path: &str, mode: fs::FileOpenMode) -> Result<Box<dyn super::File>> {
        self.fs.open_file(self.make_path(path).as_str(), mode)
    }

    fn query_entry(
        &self,
        path: &str,
        query_id: fs::QueryId,
        in_buf: &[u8],
        out_buf: &mut [u8],
    ) -> Result<()> {
        self.fs
            .query_entry(self.make_path(path).as_str(), query_id, in_buf, out_buf)
    }

    fn remove_children_all(&self, path: &str) -> Result<()> {
        self.fs.remove_children_all(self.make_path(path).as_str())
    }

    fn remove_dir(&self, path: &str) -> Result<()> {
        self.fs.remove_dir(self.make_path(path).as_str())
    }

    fn remove_dir_all(&self, path: &str) -> Result<()> {
        self.fs.remove_dir_all(self.make_path(path).as_str())
    }

    fn remove_file(&self, path: &str) -> Result<()> {
        self.fs.remove_file(self.make_path(path).as_str())
    }

    fn rename_directory(&self, old_path: &str, new_path: &str) -> Result<()> {
        self.fs.rename_directory(
            self.make_path(old_path).as_str(),
            self.make_path(new_path).as_str(),
        )
    }

    fn rename_file(&self, old_path: &str, new_path: &str) -> Result<()> {
        self.fs.rename_file(
            self.make_path(old_path).as_str(),
            self.make_path(new_path).as_str(),
        )
    }
}
