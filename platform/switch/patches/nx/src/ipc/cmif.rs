use super::*;

pub mod rc;

pub type DomainObjectId = u32;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ControlRequestId {
    ConvertCurrentObjectToDomain = 0,
    CopyFromCurrentDomain = 1,
    CloneCurrentObject = 2,
    QueryPointerBufferSize = 3,
    CloneCurrentObjectEx = 4,
}

pub const IN_DATA_HEADER_MAGIC: u32 = u32::from_le_bytes(*b"SFCI");
pub const OUT_DATA_HEADER_MAGIC: u32 = u32::from_le_bytes(*b"SFCO");

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum DomainCommandType {
    #[default]
    Invalid = 0,
    SendMessage = 1,
    Close = 2,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct DomainInDataHeader {
    pub command_type: DomainCommandType,
    pub object_count: u8,
    pub data_size: u16,
    pub domain_object_id: DomainObjectId,
    pub pad: u32,
    pub token: u32,
}

impl DomainInDataHeader {
    pub const fn empty() -> Self {
        Self {
            command_type: DomainCommandType::Invalid,
            object_count: 0,
            data_size: 0,
            domain_object_id: 0,
            pad: 0,
            token: 0,
        }
    }

    pub const fn new(
        command_type: DomainCommandType,
        object_count: u8,
        data_size: u16,
        domain_object_id: DomainObjectId,
        token: u32,
    ) -> Self {
        Self {
            command_type,
            object_count,
            data_size,
            domain_object_id,
            pad: 0,
            token,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct DomainOutDataHeader {
    pub out_object_count: u32,
    pub pad: [u32; 3],
}

impl DomainOutDataHeader {
    pub const fn empty() -> Self {
        Self {
            out_object_count: 0,
            pad: [0; 3],
        }
    }

    pub const fn new(out_object_count: u32) -> Self {
        let mut header = Self::empty();
        header.out_object_count = out_object_count;
        header
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u16)]
pub enum CommandType {
    #[default]
    Invalid = 0,
    LegacyRequest = 1,
    Close = 2,
    LegacyControl = 3,
    Request = 4,
    Control = 5,
    RequestWithContext = 6,
    ControlWithContext = 7,
}

pub fn convert_command_type(command_type: u32) -> CommandType {
    match command_type {
        1 => CommandType::LegacyRequest,
        2 => CommandType::Close,
        3 => CommandType::LegacyControl,
        4 => CommandType::Request,
        5 => CommandType::Control,
        6 => CommandType::RequestWithContext,
        7 => CommandType::ControlWithContext,
        _ => CommandType::Invalid,
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct DataHeader {
    pub magic: u32,
    pub version: u32,
    pub value: u32,
    pub token: u32,
}

impl DataHeader {
    pub const fn empty() -> Self {
        Self {
            magic: 0,
            version: 0,
            value: 0,
            token: 0,
        }
    }

    pub const fn new(magic: u32, version: u32, value: u32, token: u32) -> Self {
        Self {
            magic,
            version,
            value,
            token,
        }
    }
}

pub mod client;

pub mod server;
