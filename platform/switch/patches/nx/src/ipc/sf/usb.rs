pub mod hs;

use nx_derive::{Request, Response};

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DescriptorType {
    Device = 0x1,
    Config = 0x2,
    String = 0x3,
    Interface = 0x4,
    EndPoint = 0x5,
    Bos = 0xF,
    DeviceCapability = 0x10,
    Hid = 0x21,
    Report = 0x22,
    Physical = 0x23,
    Hub = 0x29,
    SuperSpeedHub = 0x2A,
    SsEndPointCompanion = 0x30,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum ClassCode {
    #[default]
    PerInterface = 0x0,
    Audio = 0x1,
    Comm = 0x2,
    Hid = 0x3,
    Physical = 0x5,
    Printer = 0x7,
    Image = 0x6,
    MassStorage = 0x8,
    Hub = 0x9,
    Data = 0xA,
    SmartCard = 0xB,
    ContentSecurity = 0xD,
    Video = 0xE,
    PersonalHealthcare = 0xF,
    DiagnosticDevice = 0xDC,
    Wireless = 0xE0,
    Application = 0xFE,
    VendorSpec = 0xFF,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct InterfaceDescriptor {
    length: u8,
    descriptor_type: DescriptorType,
    interface_number: u8,
    alternate_setting: u8,
    endpoint_count: u8,
    interface_class: ClassCode,
    interface_subclass: u8,
    interface_protocol: u8,
    interface: u8,
}
const_assert!(core::mem::size_of::<InterfaceDescriptor>() == 0x9);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C, packed)]
pub struct EndPointDescriptor {
    length: u8,
    descriptor_type: DescriptorType,
    endpoint_access: u8,
    attributes: u8,
    max_packet_size: u16,
    interval: u8,
}
const_assert!(core::mem::size_of::<EndPointDescriptor>() == 0x7);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct SsEndPointCompanionDescriptor {
    length: u8,
    descriptor_type: DescriptorType,
    max_burst: u8,
    attributes: u8,
    bytes_per_interval: u16,
}
const_assert!(core::mem::size_of::<SsEndPointCompanionDescriptor>() == 0x6);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct DeviceDescriptor {
    length: u8,
    descriptor_type: DescriptorType,
    usb_bcd: u16,
    device_class: ClassCode,
    device_subclass: u8,
    device_protocol: u8,
    max_packet_size_0: u8,
    vendor_id: u16,
    product_id: u16,
    device_bcd: u8,
    manufacturer: u8,
    product: u8,
    serial_number: u8,
    configuration_count: u8,
}
const_assert!(core::mem::size_of::<DeviceDescriptor>() == 0x12);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C, packed)]
pub struct ConfigDescriptor {
    length: u8,
    descriptor_type: DescriptorType,
    total_length: u16,
    interface_count: u8,
    configuration_value: u8,
    configuration: u8,
    attributes: u8,
    max_power: u8,
}
const_assert!(core::mem::size_of::<ConfigDescriptor>() == 0x9);
