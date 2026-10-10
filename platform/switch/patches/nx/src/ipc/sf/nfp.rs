use crate::ipc::sf;
use crate::ipc::sf::applet;
use crate::ipc::sf::hid;
use crate::ipc::sf::mii;
use crate::util;
use crate::version;

use super::ncm;

use nx_derive::{Request, Response};

pub mod rc;

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct McuVersionData {
    pub version: u64,
    pub reserved: [u8; 0x18],
}
const_assert!(core::mem::size_of::<McuVersionData>() == 0x20);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct DeviceHandle {
    pub id: u32,
    pub reserved: [u8; 4],
}
const_assert!(core::mem::size_of::<DeviceHandle>() == 0x8);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum State {
    NonInitialized = 0,
    Initialized = 1,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum DeviceState {
    Initialized = 0,
    SearchingForTag = 1,
    TagFound = 2,
    TagRemoved = 3,
    TagMounted = 4,
    Unavailable = 5,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ModelType {
    Amiibo = 0,
}

define_bit_set! {
    MountTarget (u32) {
        Rom = bit!(0),
        Ram = bit!(1)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}
const_assert!(core::mem::size_of::<Date>() == 0x4);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TagId {
    pub uuid: [u8; 10],
    pub uuid_length: u8,
    pub reserved_1: [u8; 0x15],
}
const_assert!(core::mem::size_of::<TagId>() == 0x20);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TagInfo {
    pub uid: TagId,
    pub protocol: u32,
    pub tag_type: u32,
    pub reserved_2: [u8; 0x30],
}
const_assert!(core::mem::size_of::<TagInfo>() == 0x58);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct RegisterInfo {
    pub mii_charinfo: mii::CharInfo,
    pub first_write_date: Date,
    pub name: util::ArrayString<41>,
    pub font_region: u8,
    pub reserved: [u8; 0x7A],
}
const_assert!(core::mem::size_of::<RegisterInfo>() == 0x100);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct CommonInfo {
    pub last_write_date: Date,
    pub write_counter: u16,
    pub version: u8,
    pub pad: u8,
    pub application_area_size: u32,
    pub reserved: [u8; 0x34],
}
const_assert!(core::mem::size_of::<CommonInfo>() == 0x40);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct ModelInfo {
    pub character_id: [u8; 3],
    pub series_id: u8,
    pub numbering_id: u16,
    pub nfp_type: u8,
    pub reserved: [u8; 0x39],
}
const_assert!(core::mem::size_of::<ModelInfo>() == 0x40);

pub type AccessId = u32;

define_bit_set! {
    AdminInfoFlags (u8) { // Note: plain amiibo flags shifted 4 bits (original bits 0-3 are discarded)
        IsInitialized = bit!(0),
        HasApplicationArea = bit!(1)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum ApplicationAreaVersion {
    // Note: unofficial name
    #[default]
    Default = 0,
    NintendoWiiU = 1,
    Nintendo3DS = 2,
    NintendoSwitch = 3,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct AdminInfo {
    pub app_id: ncm::ProgramId,
    pub access_id: AccessId,
    pub terminal_id_crc32_change_counter: u16,
    pub flags: AdminInfoFlags,
    pub unk: u8,
    pub app_area_version: ApplicationAreaVersion,
    pub pad: [u8; 0x7],
    pub reserved: [u8; 0x28],
}
const_assert!(core::mem::size_of::<AdminInfo>() == 0x40);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct RegisterInfoPrivate {
    pub mii_store_data: mii::StoreData,
    pub first_write_date: Date,
    pub name: util::ArrayString<41>,
    pub font_region: u8,
    pub reserved: [u8; 0x8E],
}
const_assert!(core::mem::size_of::<RegisterInfoPrivate>() == 0x100);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NfpData {
    pub header_magic: u8,
    pub reserved: u8,
    pub header_write_counter: u16,
    pub terminal_id_crc32: u32,
    pub reserved_2: [u8; 0x38],
    pub common_info: CommonInfo,
    pub mii_v3: mii::Ver3StoreData,
    pub pad: [u8; 2],
    pub mii_crc16: u16,
    pub mii_store_data_extension: mii::NfpStoreDataExtension,
    pub first_write_date: Date,
    pub name: util::ArrayWideString<11>,
    pub font_region: u8,
    pub unk_1: u8,
    pub mii_crc32: u32,
    pub unk_2: [u8; 0x14],
    pub reserved_3: [u8; 100],
    pub modified_app_id: u64,
    pub access_id: AccessId,
    pub terminal_id_crc32_change_counter: u16,
    pub flags: AdminInfoFlags,
    pub unk_3: u8,
    pub app_id_byte: u8,
    pub reserved_4: [u8; 0x2E],
    pub app_area: [u8; 0xD8],
}
const_assert!(core::mem::size_of::<NfpData>() == 0x298);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum BreakType {
    // Note: unofficial names
    FlushOnly = 0,
    BreakDataHash = 1,
    BreakHeaderMagic = 2,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum WriteType {
    Unk0 = 0,
    Unk1 = 1,
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait User {
    #[ipc_rid(0)]
    fn initialize(
        &self,
        aruid: applet::AppletResourceUserId,
        process_id: sf::ProcessId,
        mcu_data: sf::InMapAliasBuffer<'_, McuVersionData>,
    );
    #[ipc_rid(1)]
    fn finalize(&self);
    #[ipc_rid(2)]
    fn list_devices(&self, out_devices: sf::OutPointerBuffer<'_, DeviceHandle>) -> u32;
    #[ipc_rid(3)]
    fn start_detection(&self, device_handle: DeviceHandle);
    #[ipc_rid(4)]
    fn stop_detection(&self, device_handle: DeviceHandle);
    #[ipc_rid(5)]
    fn mount(&self, device_handle: DeviceHandle, model_type: ModelType, mount_target: MountTarget);
    #[ipc_rid(6)]
    fn unmount(&self, device_handle: DeviceHandle);
    #[ipc_rid(7)]
    fn open_application_area(&self, device_handle: DeviceHandle, access_id: AccessId);
    #[ipc_rid(8)]
    fn get_application_area(
        &self,
        device_handle: DeviceHandle,
        out_data: sf::OutMapAliasBuffer<'_, u8>,
    ) -> u32;
    #[ipc_rid(9)]
    fn set_application_area(&self, device_handle: DeviceHandle, data: sf::InMapAliasBuffer<'_, u8>);
    #[ipc_rid(10)]
    fn flush(&self, device_handle: DeviceHandle);
    #[ipc_rid(11)]
    fn restore(&self, device_handle: DeviceHandle);
    #[ipc_rid(12)]
    fn create_application_area(
        &self,
        device_handle: DeviceHandle,
        access_id: AccessId,
        data: sf::InMapAliasBuffer<'_, u8>,
    );
    #[ipc_rid(13)]
    fn get_tag_info(
        &self,
        device_handle: DeviceHandle,
        out_tag_info: sf::OutFixedPointerBuffer<'_, TagInfo>,
    );
    #[ipc_rid(14)]
    fn get_register_info(
        &self,
        device_handle: DeviceHandle,
        out_register_info: sf::OutFixedPointerBuffer<'_, RegisterInfo>,
    );
    #[ipc_rid(15)]
    fn get_common_info(
        &self,
        device_handle: DeviceHandle,
        out_common_info: sf::OutFixedPointerBuffer<'_, CommonInfo>,
    );
    #[ipc_rid(16)]
    fn get_model_info(
        &self,
        device_handle: DeviceHandle,
        out_model_info: sf::OutFixedPointerBuffer<'_, ModelInfo>,
    );
    #[ipc_rid(17)]
    fn attach_activate_event(&self, device_handle: DeviceHandle) -> sf::CopyHandle;
    #[ipc_rid(18)]
    fn attach_deactivate_event(&self, device_handle: DeviceHandle) -> sf::CopyHandle;
    #[ipc_rid(19)]
    fn get_state(&self) -> State;
    #[ipc_rid(20)]
    fn get_device_state(&self, device_handle: DeviceHandle) -> DeviceState;
    #[ipc_rid(21)]
    fn get_npad_id(&self, device_handle: DeviceHandle) -> hid::NpadIdType;
    #[ipc_rid(22)]
    fn get_application_area_size(&self, device_handle: DeviceHandle) -> u32;
    #[ipc_rid(23)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn attach_availability_change_event(&self) -> sf::CopyHandle;
    #[ipc_rid(24)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn recreate_application_area(
        &self,
        device_handle: DeviceHandle,
        access_id: AccessId,
        data: sf::InMapAliasBuffer<'_, u8>,
    );
}

#[nx_derive::ipc_trait]
pub trait UserManager {
    #[ipc_rid(0)]
    #[return_session]
    fn create_user_interface(&self) -> User;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait System {
    #[ipc_rid(0)]
    fn initialize(
        &self,
        aruid: applet::AppletResourceUserId,
        process_id: sf::ProcessId,
        mcu_data: sf::InMapAliasBuffer<'_, McuVersionData>,
    );
    #[ipc_rid(1)]
    fn finalize(&self);
    #[ipc_rid(2)]
    fn list_devices(&self, out_devices: sf::OutPointerBuffer<'_, DeviceHandle>) -> u32;
    #[ipc_rid(3)]
    fn start_detection(&self, device_handle: DeviceHandle);
    #[ipc_rid(4)]
    fn stop_detection(&self, device_handle: DeviceHandle);
    #[ipc_rid(5)]
    fn mount(&self, device_handle: DeviceHandle, model_type: ModelType, mount_target: MountTarget);
    #[ipc_rid(6)]
    fn unmount(&self, device_handle: DeviceHandle);
    #[ipc_rid(10)]
    fn flush(&self, device_handle: DeviceHandle);
    #[ipc_rid(11)]
    fn restore(&self, device_handle: DeviceHandle);
    #[ipc_rid(13)]
    fn get_tag_info(
        &self,
        device_handle: DeviceHandle,
        out_tag_info: sf::OutFixedPointerBuffer<'_, TagInfo>,
    );
    #[ipc_rid(14)]
    fn get_register_info(
        &self,
        device_handle: DeviceHandle,
        out_register_info: sf::OutFixedPointerBuffer<'_, RegisterInfo>,
    );
    #[ipc_rid(15)]
    fn get_common_info(
        &self,
        device_handle: DeviceHandle,
        out_common_info: sf::OutFixedPointerBuffer<'_, CommonInfo>,
    );
    #[ipc_rid(16)]
    fn get_model_info(
        &self,
        device_handle: DeviceHandle,
        out_model_info: sf::OutFixedPointerBuffer<'_, ModelInfo>,
    );
    #[ipc_rid(17)]
    fn attach_activate_event(&self, device_handle: DeviceHandle) -> sf::CopyHandle;
    #[ipc_rid(18)]
    fn attach_deactivate_event(&self, device_handle: DeviceHandle) -> sf::CopyHandle;
    #[ipc_rid(19)]
    fn get_state(&self) -> State;
    #[ipc_rid(20)]
    fn get_device_state(&self, device_handle: DeviceHandle) -> DeviceState;
    #[ipc_rid(21)]
    fn get_npad_id(&self, device_handle: DeviceHandle) -> hid::NpadIdType;
    #[ipc_rid(23)]
    fn attach_availability_change_event(&self) -> sf::CopyHandle;
    #[ipc_rid(100)]
    fn format(&self, device_handle: DeviceHandle);
    #[ipc_rid(101)]
    fn get_admin_info(
        &self,
        device_handle: DeviceHandle,
        out_admin_info: sf::OutFixedPointerBuffer<'_, AdminInfo>,
    );
    #[ipc_rid(102)]
    fn get_register_info_private(
        &self,
        device_handle: DeviceHandle,
        out_register_info_private: sf::OutFixedPointerBuffer<'_, RegisterInfoPrivate>,
    );
    #[ipc_rid(103)]
    fn set_register_info_private(
        &self,
        device_handle: DeviceHandle,
        register_info_private: sf::InFixedPointerBuffer<'_, RegisterInfoPrivate>,
    );
    #[ipc_rid(104)]
    fn delete_register_info(&self, device_handle: DeviceHandle);
    #[ipc_rid(105)]
    fn delete_application_area(&self, device_handle: DeviceHandle);
    #[ipc_rid(106)]
    fn exists_application_area(&self, device_handle: DeviceHandle) -> bool;
}

#[nx_derive::ipc_trait]
pub trait SystemManager {
    #[ipc_rid(0)]
    #[return_session]
    fn create_system_interface(&self) -> System;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait Debug {
    #[ipc_rid(0)]
    fn initialize(
        &self,
        aruid: applet::AppletResourceUserId,
        process_id: sf::ProcessId,
        mcu_data: sf::InMapAliasBuffer<'_, McuVersionData>,
    );
    #[ipc_rid(1)]
    fn finalize(&self);
    #[ipc_rid(2)]
    fn list_devices(&self, out_devices: sf::OutPointerBuffer<'_, DeviceHandle>) -> u32;
    #[ipc_rid(3)]
    fn start_detection(&self, device_handle: DeviceHandle);
    #[ipc_rid(4)]
    fn stop_detection(&self, device_handle: DeviceHandle);
    #[ipc_rid(5)]
    fn mount(&self, device_handle: DeviceHandle, model_type: ModelType, mount_target: MountTarget);
    #[ipc_rid(6)]
    fn unmount(&self, device_handle: DeviceHandle);
    #[ipc_rid(7)]
    fn open_application_area(&self, device_handle: DeviceHandle, access_id: AccessId);
    #[ipc_rid(8)]
    fn get_application_area(
        &self,
        device_handle: DeviceHandle,
        out_data: sf::OutMapAliasBuffer<'_, u8>,
    ) -> u32;
    #[ipc_rid(9)]
    fn set_application_area(&self, device_handle: DeviceHandle, data: sf::InMapAliasBuffer<'_, u8>);
    #[ipc_rid(10)]
    fn flush(&self, device_handle: DeviceHandle);
    #[ipc_rid(11)]
    fn restore(&self, device_handle: DeviceHandle);
    #[ipc_rid(12)]
    fn create_application_area(
        &self,
        device_handle: DeviceHandle,
        access_id: AccessId,
        data: sf::InMapAliasBuffer<'_, u8>,
    );
    #[ipc_rid(13)]
    fn get_tag_info(
        &self,
        device_handle: DeviceHandle,
        out_tag_info: sf::OutFixedPointerBuffer<'_, TagInfo>,
    );
    #[ipc_rid(14)]
    fn get_register_info(
        &self,
        device_handle: DeviceHandle,
        out_register_info: sf::OutFixedPointerBuffer<'_, RegisterInfo>,
    );
    #[ipc_rid(15)]
    fn get_common_info(
        &self,
        device_handle: DeviceHandle,
        out_common_info: sf::OutFixedPointerBuffer<'_, CommonInfo>,
    );
    #[ipc_rid(16)]
    fn get_model_info(
        &self,
        device_handle: DeviceHandle,
        out_model_info: sf::OutFixedPointerBuffer<'_, ModelInfo>,
    );
    #[ipc_rid(17)]
    fn attach_activate_event(&self, device_handle: DeviceHandle) -> sf::CopyHandle;
    #[ipc_rid(18)]
    fn attach_deactivate_event(&self, device_handle: DeviceHandle) -> sf::CopyHandle;
    #[ipc_rid(19)]
    fn get_state(&self) -> State;
    #[ipc_rid(20)]
    fn get_device_state(&self, device_handle: DeviceHandle) -> DeviceState;
    #[ipc_rid(21)]
    fn get_npad_id(&self, device_handle: DeviceHandle) -> hid::NpadIdType;
    #[ipc_rid(22)]
    fn get_application_area_size(&self, device_handle: DeviceHandle) -> u32;
    #[ipc_rid(23)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn attach_availability_change_event(&self) -> sf::CopyHandle;
    #[ipc_rid(24)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn recreate_application_area(
        &self,
        device_handle: DeviceHandle,
        access_id: AccessId,
        data: sf::InMapAliasBuffer<'_, u8>,
    );
    #[ipc_rid(100)]
    fn format(&self, device_handle: DeviceHandle);
    #[ipc_rid(101)]
    fn get_admin_info(
        &self,
        device_handle: DeviceHandle,
        out_admin_info: sf::OutFixedPointerBuffer<'_, AdminInfo>,
    );
    #[ipc_rid(102)]
    fn get_register_info_private(
        &self,
        device_handle: DeviceHandle,
        out_register_info_private: sf::OutFixedPointerBuffer<'_, RegisterInfoPrivate>,
    );
    #[ipc_rid(103)]
    fn set_register_info_private(
        &self,
        device_handle: DeviceHandle,
        register_info_private: sf::InFixedPointerBuffer<'_, RegisterInfoPrivate>,
    );
    #[ipc_rid(104)]
    fn delete_register_info(&self, device_handle: DeviceHandle);
    #[ipc_rid(105)]
    fn delete_application_area(&self, device_handle: DeviceHandle);
    #[ipc_rid(106)]
    fn exists_application_area(&self, device_handle: DeviceHandle) -> bool;
    #[ipc_rid(200)]
    fn get_all(
        &self,
        device_handle: DeviceHandle,
        out_data: sf::OutFixedPointerBuffer<'_, NfpData>,
    );
    #[ipc_rid(201)]
    fn set_all(&self, device_handle: DeviceHandle, data: sf::InFixedPointerBuffer<'_, NfpData>);
    #[ipc_rid(202)]
    fn flush_debug(&self, device_handle: DeviceHandle);
    #[ipc_rid(203)]
    fn break_tag(&self, device_handle: DeviceHandle, break_type: BreakType);
    #[ipc_rid(204)]
    fn read_backup_data(
        &self,
        device_handle: DeviceHandle,
        out_buf: sf::OutMapAliasBuffer<'_, u8>,
    ) -> u32;
    #[ipc_rid(205)]
    fn write_backup_data(&self, device_handle: DeviceHandle, buf: sf::InMapAliasBuffer<'_, u8>);
    #[ipc_rid(206)]
    fn write_ntf(
        &self,
        device_handle: DeviceHandle,
        write_type: WriteType,
        buf: sf::InMapAliasBuffer<'_, u8>,
    );
}

#[nx_derive::ipc_trait]
pub trait DebugManager {
    #[ipc_rid(0)]
    #[return_session]
    fn create_debug_interface(&self) -> Debug;
}
