use crate::ipc::sf;
use crate::util::ArrayString;
use crate::version;
use core::fmt::{Debug, Display, Formatter, Result as FmtResult};

use nx_derive::{Request, Response};

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(C)]
pub struct ProgramId(pub u64);

impl Display for ProgramId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:#018X}", self.0)
    }
}

impl Debug for ProgramId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:#018X}", self.0)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(C)]
pub struct ApplicationId(pub u64);

impl Display for ApplicationId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:#018X}", self.0)
    }
}

impl Debug for ApplicationId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:#018X}", self.0)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum StorageId {
    #[default]
    None = 0,
    Host = 1,
    GameCard = 2,
    BuiltInSystem = 3,
    BuiltInUser = 4,
    SdCard = 5,
    Any = 6,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum ContentMetaType {
    #[default]
    Unknown = 0x0,
    SystemProgram = 0x1,
    SystemData = 0x2,
    SystemUpdate = 0x3,
    BootImagePackage = 0x4,
    BootImagePackageSafe = 0x5,
    Application = 0x80,
    Patch = 0x81,
    AddOnContent = 0x82,
    Delta = 0x83,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum ContentType {
    #[default]
    Meta = 0,
    Program = 1,
    Data = 2,
    Control = 3,
    HtmlDocument = 4,
    LegalInformation = 5,
    DeltaFragment = 6,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum ContentInstallType {
    #[default]
    Full = 0x0,
    FragmentOnly = 0x1,
    Unknown = 0x7,
}

pub type ContentPath = ArrayString<0x301>;

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ContentMetaKey {
    pub program_id: ProgramId,
    pub version: u32,
    pub meta_type: ContentMetaType,
    pub install_type: ContentInstallType,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ApplicationContentMetaKey {
    pub key: ContentMetaKey,
    pub app_id: ApplicationId,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ContentId {
    pub id: [u8; 0x10],
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct LegacyContentInfo {
    pub hash: [u8; 0x20],
    pub id: ContentId,
    pub size: [u8; 0x6],
    pub cnt_type: ContentType,
    pub id_offset: u8,
}
const_assert!(core::mem::size_of::<LegacyContentInfo>() == 0x38);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ContentInfo {
    pub hash: [u8; 0x20],
    pub id: ContentId,
    pub size: [u8; 0x5],
    pub attrs: u8,
    pub cnt_type: ContentType,
    pub id_offset: u8,
}
const_assert!(core::mem::size_of::<ContentInfo>() == 0x38);

#[derive(Request, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ContentMetaInfo {
    pub program_id: ProgramId,
    pub version: u32,
    pub meta_type: ContentMetaType,
    pub attrs: u8,
    pub pad: [u8; 2],
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait ContentMetaDatabase {
    #[ipc_rid(0)]
    fn set(&self, meta_key: ContentMetaKey, in_rec_buf: sf::InMapAliasBuffer<'_, u8>);
    #[ipc_rid(1)]
    fn get(&self, meta_key: ContentMetaKey, out_rec_buf: sf::OutMapAliasBuffer<'_, u8>) -> usize;
    #[ipc_rid(2)]
    fn remove(&self, meta_key: ContentMetaKey);
    #[ipc_rid(3)]
    fn get_content_id_by_type(&self, meta_key: ContentMetaKey, cnt_type: ContentType) -> ContentId;
    #[ipc_rid(4)]
    fn list_content_info(
        &self,
        out_rec_buf: sf::OutMapAliasBuffer<'_, ContentInfo>,
        meta_key: ContentMetaKey,
        offset: u32,
    ) -> u32;
    #[ipc_rid(5)]
    fn list(
        &self,
        out_meta_keys: sf::OutMapAliasBuffer<'_, ContentMetaKey>,
        meta_type: ContentMetaType,
        program_id: ProgramId,
        program_id_min: ProgramId,
        program_id_max: ProgramId,
        install_type: ContentInstallType,
    ) -> (u32, u32);
    #[ipc_rid(6)]
    fn get_latest_content_meta_key(&self, program_id: ProgramId) -> ContentMetaKey;
    #[ipc_rid(7)]
    fn list_application(
        &self,
        out_app_meta_keys: sf::OutMapAliasBuffer<'_, ApplicationContentMetaKey>,
        meta_type: ContentMetaType,
    ) -> (u32, u32);
    #[ipc_rid(8)]
    fn has(&self, meta_key: ContentMetaKey) -> bool;
    #[ipc_rid(9)]
    fn has_all(&self, meta_keys_buf: sf::InMapAliasBuffer<'_, ContentMetaKey>) -> bool;
    #[ipc_rid(10)]
    fn get_size(&self, meta_key: ContentMetaKey) -> usize;
    #[ipc_rid(11)]
    fn get_required_system_version(&self, meta_key: ContentMetaKey) -> u32;
    #[ipc_rid(12)]
    fn get_patch_content_meta_id(&self, meta_key: ContentMetaKey) -> ProgramId;
    #[ipc_rid(13)]
    fn disable_forcibly(&self);
    #[ipc_rid(14)]
    fn lookup_orphan_content(
        &self,
        content_ids_buf: sf::InMapAliasBuffer<'_, ContentId>,
        out_orphaned_buf: sf::OutMapAliasBuffer<'_, bool>,
    );
    #[ipc_rid(15)]
    fn commit(&self);
    #[ipc_rid(16)]
    fn has_content(&self, meta_key: ContentMetaKey, id: ContentId) -> bool;
    #[ipc_rid(17)]
    fn list_content_meta_info(
        &self,
        out_meta_infos: sf::OutMapAliasBuffer<'_, ContentMetaInfo>,
        meta_key: ContentMetaKey,
        offset: u32,
    ) -> u32;
    #[ipc_rid(18)]
    fn get_attributes(&self, meta_key: ContentMetaKey) -> u8;
    #[ipc_rid(19)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn get_required_application_version(&self, meta_key: ContentMetaKey) -> u32;
    #[ipc_rid(20)]
    #[version(version::VersionInterval::from(version::Version::new(5, 0, 0)))]
    fn get_content_id_by_type_and_offset(
        &self,
        meta_key: ContentMetaKey,
        cnt_type: ContentType,
        id_offset: u8,
    ) -> ContentId;
    #[ipc_rid(21)]
    #[version(version::VersionInterval::from(version::Version::new(10, 0, 0)))]
    fn get_count(&self) -> u32;
    #[ipc_rid(22)]
    #[version(version::VersionInterval::from(version::Version::new(10, 0, 0)))]
    fn get_owner_application_id(&self, meta_key: ContentMetaKey) -> ApplicationId;
}

#[nx_derive::ipc_trait]
pub trait ContentManager {
    #[ipc_rid(0)]
    #[return_session]
    fn open_content_meta_database(&self, storage_id: StorageId) -> ContentMetaDatabase;
}
