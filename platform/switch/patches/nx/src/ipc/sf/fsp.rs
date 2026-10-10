use crate::ipc::sf;
use crate::util;
use crate::version;

pub mod rc;

use nx_derive::{Request, Response};

define_bit_set! {
    FileOpenMode (u32) {
        None = 0,
        Read = bit!(0),
        Write = bit!(1),
        Append = bit!(2)
    }
}

define_bit_set! {
    DirectoryOpenMode (u32) {
        ReadDirectories = bit!(0),
        ReadFiles = bit!(1),
        NoFileSizes = bit!(31)
    }
}

define_bit_set! {
    FileAttribute (u32) {
        None = 0,
        ConcatenationFile = bit!(0)
    }
}

define_bit_set! {
    FileReadOption (u32) {
        None = 0
    }
}

define_bit_set! {
    FileWriteOption (u32) {
        None = 0,
        Flush = bit!(0)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum DirectoryEntryType {
    #[default]
    Directory = 0,
    File = 1,
}

pub type Path = util::ArrayString<0x301>;

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct DirectoryEntry {
    pub name: Path,
    pub attr: u8,
    pub pad: [u8; 2],
    pub entry_type: DirectoryEntryType,
    pub pad_2: [u8; 3],
    pub file_size: usize,
}
const_assert!(core::mem::size_of::<DirectoryEntry>() == 0x310);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct FileTimeStampRaw {
    pub create: i64,
    pub modify: i64,
    pub access: i64,
    pub is_local_time: bool,
    pub pad: [u8; 7],
}
const_assert!(core::mem::size_of::<FileTimeStampRaw>() == 0x20);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum QueryId {
    SetConcatenationFileAttribute = 0,
    UpdateMac = 1,
    IsSignedSystemPartitionOnSdCardValid = 2,
    QueryUnpreparedFileInformation = 3,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct FileQueryRangeInfo {
    pub aes_ctr_key_type: u32,
    pub speed_emulation_type: u32,
    pub reserved_1: [u8; 0x20],
    pub reserved_2: [u8; 0x18],
}
const_assert!(core::mem::size_of::<FileQueryRangeInfo>() == 0x40);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum OperationId {
    FillZero = 0,
    DestroySignature = 1,
    Invalidate = 2,
    QueryRange = 3,
    QueryUnpreparedRange = 4,
    QueryLazyLoadCompletionRate = 5,
    SetLazyLoadPriority = 6,
    ReadLazyLoadFileForciblyForDebug = 10001,
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait File {
    #[ipc_rid(0)]
    fn read(
        &mut self,
        option: FileReadOption,
        offset: usize,
        size: usize,
        out_buf: sf::OutNonSecureMapAliasBuffer<'_, u8>,
    ) -> usize;
    #[ipc_rid(1)]
    fn write(
        &mut self,
        option: FileWriteOption,
        offset: usize,
        size: usize,
        buf: sf::InNonSecureMapAliasBuffer<'_, u8>,
    );
    #[ipc_rid(2)]
    fn flush(&self);
    #[ipc_rid(3)]
    fn set_size(&mut self, size: usize);
    #[ipc_rid(4)]
    fn get_size(&mut self) -> usize;
    #[ipc_rid(5)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn operate_range(
        &mut self,
        operation_id: OperationId,
        offset: usize,
        size: usize,
    ) -> FileQueryRangeInfo;
    #[ipc_rid(6)]
    #[version(version::VersionInterval::from(version::Version::new(12, 0, 0)))]
    fn operate_range_with_buffer(
        &mut self,
        operation_id: OperationId,
        offset: usize,
        size: usize,
        in_buf: sf::InNonSecureMapAliasBuffer<'_, u8>,
        out_buf: sf::OutNonSecureMapAliasBuffer<'_, u8>,
    );
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait Directory {
    #[ipc_rid(0)]
    fn read(&self, out_entries: sf::OutMapAliasBuffer<'_, DirectoryEntry>) -> u64;
    #[ipc_rid(1)]
    fn get_entry_count(&self) -> u64;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait FileSystem {
    #[ipc_rid(0)]
    fn create_file(
        &self,
        attribute: FileAttribute,
        size: usize,
        path_buf: sf::InFixedPointerBuffer<'_, Path>,
    );
    #[ipc_rid(1)]
    fn delete_file(&self, path_buf: sf::InFixedPointerBuffer<'_, Path>);
    #[ipc_rid(2)]
    fn create_directory(&self, path_buf: sf::InFixedPointerBuffer<'_, Path>);
    #[ipc_rid(3)]
    fn delete_directory(&self, path_buf: sf::InFixedPointerBuffer<'_, Path>);
    #[ipc_rid(4)]
    fn delete_directory_recursively(&self, path_buf: sf::InFixedPointerBuffer<'_, Path>);
    #[ipc_rid(5)]
    fn rename_file(
        &self,
        old_path_buf: sf::InFixedPointerBuffer<'_, Path>,
        new_path_buf: sf::InFixedPointerBuffer<'_, Path>,
    );
    #[ipc_rid(6)]
    fn rename_directory(
        &self,
        old_path_buf: sf::InFixedPointerBuffer<'_, Path>,
        new_path_buf: sf::InFixedPointerBuffer<'_, Path>,
    );
    #[ipc_rid(7)]
    fn get_entry_type(&self, path_buf: sf::InFixedPointerBuffer<'_, Path>) -> DirectoryEntryType;
    #[ipc_rid(8)]
    #[return_session]
    fn open_file(&self, mode: FileOpenMode, path_buf: sf::InFixedPointerBuffer<'_, Path>) -> File;
    #[ipc_rid(9)]
    #[return_session]
    fn open_directory(
        &self,
        mode: DirectoryOpenMode,
        path_buf: sf::InFixedPointerBuffer<'_, Path>,
    ) -> Directory;
    #[ipc_rid(10)]
    fn commit(&self);
    #[ipc_rid(11)]
    fn get_free_space_size(&self, path_buf: sf::InFixedPointerBuffer<'_, Path>) -> usize;
    #[ipc_rid(12)]
    fn get_total_space_size(&self, path_buf: sf::InFixedPointerBuffer<'_, Path>) -> usize;
    #[ipc_rid(13)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn clean_directory_recursively(&self, path_buf: sf::InFixedPointerBuffer<'_, Path>);
    #[ipc_rid(14)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn get_file_time_stamp_raw(
        &self,
        path_buf: sf::InFixedPointerBuffer<'_, Path>,
    ) -> FileTimeStampRaw;
    #[ipc_rid(15)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn query_entry(
        &self,
        path_buf: sf::InFixedPointerBuffer<'_, Path>,
        query_id: QueryId,
        in_buf: sf::InNonSecureMapAliasBuffer<'_, u8>,
        out_buf: sf::OutNonSecureMapAliasBuffer<'_, u8>,
    );
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait FileSystemProxy {
    #[ipc_rid(1)]
    fn set_current_process(&self, process_id: sf::ProcessId);
    #[ipc_rid(18)]
    #[return_session]
    fn open_sd_card_filesystem(&self) -> FileSystem;
    #[ipc_rid(1006)]
    fn output_access_log_to_sd_card(&self, log_buf: sf::InMapAliasBuffer<'_, u8>);
}

pub mod srv;
