//! Graphics and GPU support and utils

use ::alloc::sync::Arc;

use crate::ipc::sf;
use crate::mem::{alloc, wait_for_permission};
use crate::result::*;
use crate::service;
use crate::service::applet;
use crate::service::dispdrv;
use crate::service::nv;
use crate::service::nv::{ErrorCode, Fd, INvDrvClient, IoctlId};
use crate::service::vi;
use crate::service::vi::{
    ApplicationDisplay, ApplicationDisplayRootService, IApplicationDisplayClient,
    ManagerDisplayRootService, SystemDisplayRootService,
};
use crate::svc;
use crate::svc::MemoryPermission;
use crate::sync::RwLock;

pub mod rc;

pub mod parcel;

pub mod binder;

pub mod ioctl;

pub mod surface;

#[cfg(feature = "canvas")]
pub mod canvas;

/// Represents layout types
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum Layout {
    #[default]
    Invalid = 0,
    Pitch = 1,
    Tiled = 2,
    BlockLinear = 3,
}

/// Represents display scan format types
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum DisplayScanFormat {
    #[default]
    Progressive = 0,
    Interlaced = 1,
}

/// Represents kinds
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum Kind {
    #[default]
    Pitch = 0x0,
    Z16 = 0x1,
    Z16_2C = 0x2,
    Z16_MS2_2C = 0x3,
    Z16_MS4_2C = 0x4,
    Z16_MS8_2C = 0x5,
    Z16_MS16_2C = 0x6,
    Z16_2Z = 0x7,
    Z16_MS2_2Z = 0x8,
    Z16_MS4_2Z = 0x9,
    Z16_MS8_2Z = 0xa,
    Z16_MS16_2Z = 0xb,
    Z16_4CZ = 0xc,
    Z16_MS2_4CZ = 0xd,
    Z16_MS4_4CZ = 0xe,
    Z16_MS8_4CZ = 0xf,
    Z16_MS16_4CZ = 0x10,
    S8Z24 = 0x11,
    S8Z24_1Z = 0x12,
    S8Z24_MS2_1Z = 0x13,
    S8Z24_MS4_1Z = 0x14,
    S8Z24_MS8_1Z = 0x15,
    S8Z24_MS16_1Z = 0x16,
    S8Z24_2CZ = 0x17,
    S8Z24_MS2_2CZ = 0x18,
    S8Z24_MS4_2CZ = 0x19,
    S8Z24_MS8_2CZ = 0x1a,
    S8Z24_MS16_2CZ = 0x1b,
    S8Z24_2CS = 0x1C,
    S8Z24_MS2_2CS = 0x1d,
    S8Z24_MS4_2CS = 0x1e,
    S8Z24_MS8_2CS = 0x1f,
    S8Z24_MS16_2CS = 0x20,
    S8Z24_4CSZV = 0x21,
    S8Z24_MS2_4CSZV = 0x22,
    S8Z24_MS4_4CSZV = 0x23,
    S8Z24_MS8_4CSZV = 0x24,
    S8Z24_MS16_4CSZV = 0x25,
    V8Z24_MS4_VC12 = 0x26,
    V8Z24_MS4_VC4 = 0x27,
    V8Z24_MS8_VC8 = 0x28,
    V8Z24_MS8_VC24 = 0x29,
    S8 = 0x2a,
    S8_2S = 0x2b,
    V8Z24_MS4_VC12_1ZV = 0x2e,
    V8Z24_MS4_VC4_1ZV = 0x2f,
    V8Z24_MS8_VC8_1ZV = 0x30,
    V8Z24_MS8_VC24_1ZV = 0x31,
    V8Z24_MS4_VC12_2CS = 0x32,
    V8Z24_MS4_VC4_2CS = 0x33,
    V8Z24_MS8_VC8_2CS = 0x34,
    V8Z24_MS8_VC24_2CS = 0x35,
    V8Z24_MS4_VC12_2CZV = 0x3a,
    V8Z24_MS4_VC4_2CZV = 0x3b,
    V8Z24_MS8_VC8_2CZV = 0x3c,
    V8Z24_MS8_VC24_2CZV = 0x3d,
    V8Z24_MS4_VC12_2ZV = 0x3e,
    V8Z24_MS4_VC4_2ZV = 0x3f,
    V8Z24_MS8_VC8_2ZV = 0x40,
    V8Z24_MS8_VC24_2ZV = 0x41,
    V8Z24_MS4_VC12_4CSZV = 0x42,
    V8Z24_MS4_VC4_4CSZV = 0x43,
    V8Z24_MS8_VC8_4CSZV = 0x44,
    V8Z24_MS8_VC24_4CSZV = 0x45,
    Z24S8 = 0x46,
    Z24S8_1Z = 0x47,
    Z24S8_MS2_1Z = 0x48,
    Z24S8_MS4_1Z = 0x49,
    Z24S8_MS8_1Z = 0x4a,
    Z24S8_MS16_1Z = 0x4b,
    Z24S8_2CS = 0x4c,
    Z24S8_MS2_2CS = 0x4d,
    Z24S8_MS4_2CS = 0x4e,
    Z24S8_MS8_2CS = 0x4f,
    Z24S8_MS16_2CS = 0x50,
    Z24S8_2CZ = 0x51,
    Z24S8_MS2_2CZ = 0x52,
    Z24S8_MS4_2CZ = 0x53,
    Z24S8_MS8_2CZ = 0x54,
    Z24S8_MS16_2CZ = 0x55,
    Z24S8_4CSZV = 0x56,
    Z24S8_MS2_4CSZV = 0x57,
    Z24S8_MS4_4CSZV = 0x58,
    Z24S8_MS8_4CSZV = 0x59,
    Z24S8_MS16_4CSZV = 0x5a,
    Z24V8_MS4_VC12 = 0x5b,
    Z24V8_MS4_VC4 = 0x5C,
    Z24V8_MS8_VC8 = 0x5d,
    Z24V8_MS8_VC24 = 0x5e,
    Z24V8_MS4_VC12_1ZV = 0x63,
    Z24V8_MS4_VC4_1ZV = 0x64,
    Z24V8_MS8_VC8_1ZV = 0x65,
    Z24V8_MS8_VC24_1ZV = 0x66,
    Z24V8_MS4_VC12_2CS = 0x67,
    Z24V8_MS4_VC4_2CS = 0x68,
    Z24V8_MS8_VC8_2CS = 0x69,
    Z24V8_MS8_VC24_2CS = 0x6a,
    Z24V8_MS4_VC12_2CZV = 0x6f,
    Z24V8_MS4_VC4_2CZV = 0x70,
    Z24V8_MS8_VC8_2CZV = 0x71,
    Z24V8_MS8_VC24_2CZV = 0x72,
    Z24V8_MS4_VC12_2ZV = 0x73,
    Z24V8_MS4_VC4_2ZV = 0x74,
    Z24V8_MS8_VC8_2ZV = 0x75,
    Z24V8_MS8_VC24_2ZV = 0x76,
    Z24V8_MS4_VC12_4CSZV = 0x77,
    Z24V8_MS4_VC4_4CSZV = 0x78,
    Z24V8_MS8_VC8_4CSZV = 0x79,
    Z24V8_MS8_VC24_4CSZV = 0x7a,
    ZF32 = 0x7b,
    ZF32_1Z = 0x7C,
    ZF32_MS2_1Z = 0x7d,
    ZF32_MS4_1Z = 0x7e,
    ZF32_MS8_1Z = 0x7f,
    ZF32_MS16_1Z = 0x80,
    ZF32_2CS = 0x81,
    ZF32_MS2_2CS = 0x82,
    ZF32_MS4_2CS = 0x83,
    ZF32_MS8_2CS = 0x84,
    ZF32_MS16_2CS = 0x85,
    ZF32_2CZ = 0x86,
    ZF32_MS2_2CZ = 0x87,
    ZF32_MS4_2CZ = 0x88,
    ZF32_MS8_2CZ = 0x89,
    ZF32_MS16_2CZ = 0x8a,
    X8Z24_X16V8S8_MS4_VC12 = 0x8b,
    X8Z24_X16V8S8_MS4_VC4 = 0x8c,
    X8Z24_X16V8S8_MS8_VC8 = 0x8d,
    X8Z24_X16V8S8_MS8_VC24 = 0x8e,
    X8Z24_X16V8S8_MS4_VC12_1CS = 0x8f,
    X8Z24_X16V8S8_MS4_VC4_1CS = 0x90,
    X8Z24_X16V8S8_MS8_VC8_1CS = 0x91,
    X8Z24_X16V8S8_MS8_VC24_1CS = 0x92,
    X8Z24_X16V8S8_MS4_VC12_1ZV = 0x97,
    X8Z24_X16V8S8_MS4_VC4_1ZV = 0x98,
    X8Z24_X16V8S8_MS8_VC8_1ZV = 0x99,
    X8Z24_X16V8S8_MS8_VC24_1ZV = 0x9a,
    X8Z24_X16V8S8_MS4_VC12_1CZV = 0x9b,
    X8Z24_X16V8S8_MS4_VC4_1CZV = 0x9c,
    X8Z24_X16V8S8_MS8_VC8_1CZV = 0x9d,
    X8Z24_X16V8S8_MS8_VC24_1CZV = 0x9e,
    X8Z24_X16V8S8_MS4_VC12_2CS = 0x9f,
    X8Z24_X16V8S8_MS4_VC4_2CS = 0xa0,
    X8Z24_X16V8S8_MS8_VC8_2CS = 0xa1,
    X8Z24_X16V8S8_MS8_VC24_2CS = 0xa2,
    X8Z24_X16V8S8_MS4_VC12_2CSZV = 0xa3,
    X8Z24_X16V8S8_MS4_VC4_2CSZV = 0xa4,
    X8Z24_X16V8S8_MS8_VC8_2CSZV = 0xa5,
    X8Z24_X16V8S8_MS8_VC24_2CSZV = 0xa6,
    ZF32_X16V8S8_MS4_VC12 = 0xa7,
    ZF32_X16V8S8_MS4_VC4 = 0xa8,
    ZF32_X16V8S8_MS8_VC8 = 0xa9,
    ZF32_X16V8S8_MS8_VC24 = 0xaa,
    ZF32_X16V8S8_MS4_VC12_1CS = 0xab,
    ZF32_X16V8S8_MS4_VC4_1CS = 0xac,
    ZF32_X16V8S8_MS8_VC8_1CS = 0xad,
    ZF32_X16V8S8_MS8_VC24_1CS = 0xae,
    ZF32_X16V8S8_MS4_VC12_1ZV = 0xb3,
    ZF32_X16V8S8_MS4_VC4_1ZV = 0xb4,
    ZF32_X16V8S8_MS8_VC8_1ZV = 0xb5,
    ZF32_X16V8S8_MS8_VC24_1ZV = 0xb6,
    ZF32_X16V8S8_MS4_VC12_1CZV = 0xb7,
    ZF32_X16V8S8_MS4_VC4_1CZV = 0xb8,
    ZF32_X16V8S8_MS8_VC8_1CZV = 0xb9,
    ZF32_X16V8S8_MS8_VC24_1CZV = 0xba,
    ZF32_X16V8S8_MS4_VC12_2CS = 0xbb,
    ZF32_X16V8S8_MS4_VC4_2CS = 0xbc,
    ZF32_X16V8S8_MS8_VC8_2CS = 0xbd,
    ZF32_X16V8S8_MS8_VC24_2CS = 0xbe,
    ZF32_X16V8S8_MS4_VC12_2CSZV = 0xbf,
    ZF32_X16V8S8_MS4_VC4_2CSZV = 0xc0,
    ZF32_X16V8S8_MS8_VC8_2CSZV = 0xc1,
    ZF32_X16V8S8_MS8_VC24_2CSZV = 0xc2,
    ZF32_X24S8 = 0xc3,
    ZF32_X24S8_1CS = 0xc4,
    ZF32_X24S8_MS2_1CS = 0xc5,
    ZF32_X24S8_MS4_1CS = 0xc6,
    ZF32_X24S8_MS8_1CS = 0xc7,
    ZF32_X24S8_MS16_1CS = 0xc8,
    SmskedMessage = 0xca,
    SmhostMessage = 0xcb,
    C64_MS2_2CRA = 0xcd,
    ZF32_X24S8_2CSZV = 0xce,
    ZF32_X24S8_MS2_2CSZV = 0xcf,
    ZF32_X24S8_MS4_2CSZV = 0xd0,
    ZF32_X24S8_MS8_2CSZV = 0xd1,
    ZF32_X24S8_MS16_2CSZV = 0xd2,
    ZF32_X24S8_2CS = 0xd3,
    ZF32_X24S8_MS2_2CS = 0xd4,
    ZF32_X24S8_MS4_2CS = 0xd5,
    ZF32_X24S8_MS8_2CS = 0xd6,
    ZF32_X24S8_MS16_2CS = 0xd7,
    C32_2C = 0xd8,
    C32_2CBR = 0xd9,
    C32_2CBA = 0xda,
    C32_2CRA = 0xdb,
    C32_2BRA = 0xdc,
    C32_MS2_2C = 0xdd,
    C32_MS2_2CBR = 0xde,
    C32_MS2_2CRA = 0xcc,
    C32_MS4_2C = 0xdf,
    C32_MS4_2CBR = 0xe0,
    C32_MS4_2CBA = 0xe1,
    C32_MS4_2CRA = 0xe2,
    C32_MS4_2BRA = 0xe3,
    C32_MS8_MS16_2C = 0xe4,
    C32_MS8_MS16_2CRA = 0xe5,
    C64_2C = 0xe6,
    C64_2CBR = 0xe7,
    C64_2CBA = 0xe8,
    C64_2CRA = 0xe9,
    C64_2BRA = 0xea,
    C64_MS2_2C = 0xeb,
    C64_MS2_2CBR = 0xec,
    C64_MS4_2C = 0xed,
    C64_MS4_2CBR = 0xee,
    C64_MS4_2CBA = 0xef,
    C64_MS4_2CRA = 0xf0,
    C64_MS4_2BRA = 0xf1,
    C64_MS8_MS16_2C = 0xf2,
    C64_MS8_MS16_2CRA = 0xf3,
    C128_2C = 0xf4,
    C128_2CR = 0xf5,
    C128_MS2_2C = 0xf6,
    C128_MS2_2CR = 0xf7,
    C128_MS4_2C = 0xf8,
    C128_MS4_2CR = 0xf9,
    C128_MS8_MS16_2C = 0xfa,
    C128_MS8_MS16_2CR = 0xfb,
    X8C24 = 0xfc,
    PitchNoSwizzle = 0xfd,
    Generic_16BX2 = 0xfe,
    Invalid = 0xff,
}

/// Represents supported color formats
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u64)]
#[allow(missing_docs)]
pub enum ColorFormat {
    #[default]
    Unspecified = 0,
    NonColor8 = 0x0009200408,
    NonColor16 = 0x0009200A10,
    NonColor24 = 0x0009201A18,
    NonColor32 = 0x0009201C20,
    X4C4 = 0x0009210508,
    A4L4 = 0x0100490508,
    A8L8 = 0x0100490E10,
    Float_A16L16 = 0x0100495D20,
    A1B5G5R5 = 0x0100531410,
    A4B4G4R4 = 0x0100531510,
    A5B5G5R1 = 0x0100531810,
    A2B10G10R10 = 0x0100532020,
    A8B8G8R8 = 0x0100532120,
    A16B16G16R16 = 0x0100532740,
    Float_A16B16G16R16 = 0x0100536740,
    A1R5G5B5 = 0x0100D11410,
    A4R4G4B4 = 0x0100D11510,
    A5R1G5B5 = 0x0100D11610,
    A2R10G10B10 = 0x0100D12020,
    A8R8G8B8 = 0x0100D12120,
    A1 = 0x0101240101,
    A2 = 0x0101240202,
    A4 = 0x0101240304,
    A8 = 0x0101240408,
    A16 = 0x0101240A10,
    A32 = 0x0101241C20,
    Float_A16 = 0x0101244A10,
    L4A4 = 0x0102000508,
    L8A8 = 0x0102000E10,
    B4G4R4A4 = 0x01060A1510,
    B5G5R1A5 = 0x01060A1710,
    B5G5R5A1 = 0x01060A1810,
    B8G8R8A8 = 0x01060A2120,
    B10G10R10A2 = 0x01060A2320,
    R1G5B5A5 = 0x0106881410,
    R4G4B4A4 = 0x0106881510,
    R5G5B5A1 = 0x0106881810,
    R8G8B8A8 = 0x0106882120,
    R10G10B10A2 = 0x0106882320,
    L1 = 0x010A000101,
    L2 = 0x010A000202,
    L4 = 0x010A000304,
    L8 = 0x010A000408,
    L16 = 0x010A000A10,
    L32 = 0x010A001C20,
    Float_L16 = 0x010A004A10,
    B5G6R5 = 0x010A0A1210,
    B6G5R5 = 0x010A0A1310,
    B5G5R5X1 = 0x010A0A1810,
    B8_G8_R8 = 0x010A0A1918,
    B8G8R8X8 = 0x010A0A2120,
    Float_B10G11R11 = 0x010A0A5E20,
    X1B5G5R5 = 0x010A531410,
    X8B8G8R8 = 0x010A532120,
    X16B16G16R16 = 0x010A532740,
    Float_X16B16G16R16 = 0x010A536740,
    R3G3B2 = 0x010A880608,
    R5G5B6 = 0x010A881110,
    R5G6B5 = 0x010A881210,
    R5G5B5X1 = 0x010A881810,
    R8_G8_B8 = 0x010A881918,
    R8G8B8X8 = 0x010A882120,
    X1R5G5B5 = 0x010AD11410,
    X8R8G8B8 = 0x010AD12120,
    RG8 = 0x010B080E10,
    R16G16 = 0x010B081D20,
    Float_R16G16 = 0x010B085D20,
    R8 = 0x010B200408,
    R16 = 0x010B200A10,
    Float_R16 = 0x010B204A10,
    A2B10G10R10_sRGB = 0x0200532020,
    A8B8G8R8_sRGB = 0x0200532120,
    A16B16G16R16_sRGB = 0x0200532740,
    A2R10G10B10_sRGB = 0x0200D12020,
    B10G10R10A2_sRGB = 0x02060A2320,
    R10G10B10A2_sRGB = 0x0206882320,
    X8B8G8R8_sRGB = 0x020A532120,
    X16B16G16R16_sRGB = 0x020A532740,
    A2B10G10R10_709 = 0x0300532020,
    A8B8G8R8_709 = 0x0300532120,
    A16B16G16R16_709 = 0x0300532740,
    A2R10G10B10_709 = 0x0300D12020,
    B10G10R10A2_709 = 0x03060A2320,
    R10G10B10A2_709 = 0x0306882320,
    X8B8G8R8_709 = 0x030A532120,
    X16B16G16R16_709 = 0x030A532740,
    A2B10G10R10_709_Linear = 0x0400532020,
    A8B8G8R8_709_Linear = 0x0400532120,
    A16B16G16R16_709_Linear = 0x0400532740,
    A2R10G10B10_709_Linear = 0x0400D12020,
    B10G10R10A2_709_Linear = 0x04060A2320,
    R10G10B10A2_709_Linear = 0x0406882320,
    X8B8G8R8_709_Linear = 0x040A532120,
    X16B16G16R16_709_Linear = 0x040A532740,
    Float_A16B16G16R16_scRGB_Linear = 0x0500536740,
    A2B10G10R10_2020 = 0x0600532020,
    A8B8G8R8_2020 = 0x0600532120,
    A16B16G16R16_2020 = 0x0600532740,
    A2R10G10B10_2020 = 0x0600D12020,
    B10G10R10A2_2020 = 0x06060A2320,
    R10G10B10A2_2020 = 0x0606882320,
    X8B8G8R8_2020 = 0x060A532120,
    X16B16G16R16_2020 = 0x060A532740,
    A2B10G10R10_2020_Linear = 0x0700532020,
    A8B8G8R8_2020_Linear = 0x0700532120,
    A16B16G16R16_2020_Linear = 0x0700532740,
    Float_A16B16G16R16_2020_Linear = 0x0700536740,
    A2R10G10B10_2020_Linear = 0x0700D12020,
    B10G10R10A2_2020_Linear = 0x07060A2320,
    R10G10B10A2_2020_Linear = 0x0706882320,
    X8B8G8R8_2020_Linear = 0x070A532120,
    X16B16G16R16_2020_Linear = 0x070A532740,
    Float_A16B16G16R16_2020_PQ = 0x0800536740,
    A4I4 = 0x0901210508,
    A8I8 = 0x0901210E10,
    I4A4 = 0x0903200508,
    I8A8 = 0x0903200E10,
    I1 = 0x0909200101,
    I2 = 0x0909200202,
    I4 = 0x0909200304,
    I8 = 0x0909200408,
    A8Y8U8V8 = 0x0A00D12120,
    A16Y16U16V16 = 0x0A00D12740,
    Y8U8V8A8 = 0x0A06882120,
    V8_U8 = 0x0A080C0710,
    V8U8 = 0x0A080C0E10,
    V10U10 = 0x0A08142220,
    V12U12 = 0x0A08142420,
    V8 = 0x0A08240408,
    V10 = 0x0A08240F10,
    V12 = 0x0A08241010,
    U8_V8 = 0x0A08440710,
    U8V8 = 0x0A08440E10,
    U10V10 = 0x0A08842220,
    U12V12 = 0x0A08842420,
    U8 = 0x0A09040408,
    U10 = 0x0A09040F10,
    U12 = 0x0A09041010,
    Y8 = 0x0A09200408,
    Y10 = 0x0A09200F10,
    Y12 = 0x0A09201010,
    YVYU = 0x0A0A500810,
    VYUY = 0x0A0A500910,
    YUYV = 0x0A0A880810,
    UYVY = 0x0A0A880910,
    Y8_U8_V8 = 0x0A0A881918,
    V8_U8_RR = 0x0B080C0710,
    V8U8_RR = 0x0B080C0E10,
    V8_RR = 0x0B08240408,
    U8_V8_RR = 0x0B08440710,
    U8V8_RR = 0x0B08440E10,
    U8_RR = 0x0B09040408,
    Y8_RR = 0x0B09200408,
    V8_U8_ER = 0x0C080C0710,
    V8U8_ER = 0x0C080C0E10,
    V8_ER = 0x0C08240408,
    U8_V8_ER = 0x0C08440710,
    U8V8_ER = 0x0C08440E10,
    U8_ER = 0x0C09040408,
    Y8_ER = 0x0C09200408,
    V8_U8_709 = 0x0D080C0710,
    V8U8_709 = 0x0D080C0E10,
    V10U10_709 = 0x0D08142220,
    V12U12_709 = 0x0D08142420,
    V8_709 = 0x0D08240408,
    V10_709 = 0x0D08240F10,
    V12_709 = 0x0D08241010,
    U8_V8_709 = 0x0D08440710,
    U8V8_709 = 0x0D08440E10,
    U10V10_709 = 0x0D08842220,
    U12V12_709 = 0x0D08842420,
    U8_709 = 0x0D09040408,
    U10_709 = 0x0D09040F10,
    U12_709 = 0x0D09041010,
    Y8_709 = 0x0D09200408,
    Y10_709 = 0x0D09200F10,
    Y12_709 = 0x0D09201010,
    V8_U8_709_ER = 0x0E080C0710,
    V8U8_709_ER = 0x0E080C0E10,
    V10U10_709_ER = 0x0E08142220,
    V12U12_709_ER = 0x0E08142420,
    V8_709_ER = 0x0E08240408,
    V10_709_ER = 0x0E08240F10,
    V12_709_ER = 0x0E08241010,
    U8_V8_709_ER = 0x0E08440710,
    U8V8_709_ER = 0x0E08440E10,
    U10V10_709_ER = 0x0E08842220,
    U12V12_709_ER = 0x0E08842420,
    U8_709_ER = 0x0E09040408,
    U10_709_ER = 0x0E09040F10,
    U12_709_ER = 0x0E09041010,
    Y8_709_ER = 0x0E09200408,
    Y10_709_ER = 0x0E09200F10,
    Y12_709_ER = 0x0E09201010,
    V10U10_2020 = 0x0F08142220,
    V12U12_2020 = 0x0F08142420,
    V10_2020 = 0x0F08240F10,
    V12_2020 = 0x0F08241010,
    U10V10_2020 = 0x0F08842220,
    U12V12_2020 = 0x0F08842420,
    U10_2020 = 0x0F09040F10,
    U12_2020 = 0x0F09041010,
    Y10_2020 = 0x0F09200F10,
    Y12_2020 = 0x0F09201010,
    Bayer8RGGB = 0x1009200408,
    Bayer16RGGB = 0x1009200A10,
    BayerS16RGGB = 0x1009208A10,
    X2Bayer14RGGB = 0x1009210B10,
    X4Bayer12RGGB = 0x1009210C10,
    X6Bayer10RGGB = 0x1009210D10,
    Bayer8BGGR = 0x1109200408,
    Bayer16BGGR = 0x1109200A10,
    BayerS16BGGR = 0x1109208A10,
    X2Bayer14BGGR = 0x1109210B10,
    X4Bayer12BGGR = 0x1109210C10,
    X6Bayer10BGGR = 0x1109210D10,
    Bayer8GRBG = 0x1209200408,
    Bayer16GRBG = 0x1209200A10,
    BayerS16GRBG = 0x1209208A10,
    X2Bayer14GRBG = 0x1209210B10,
    X4Bayer12GRBG = 0x1209210C10,
    X6Bayer10GRBG = 0x1209210D10,
    Bayer8GBRG = 0x1309200408,
    Bayer16GBRG = 0x1309200A10,
    BayerS16GBRG = 0x1309208A10,
    X2Bayer14GBRG = 0x1309210B10,
    X4Bayer12GBRG = 0x1309210C10,
    X6Bayer10GBRG = 0x1309210D10,
    XYZ = 0x140A886640,
}

impl ColorFormat {
    /// Gets the bytes-per-pixel (`bpp`) of a [`ColorFormat`] value (bits 3-8).
    #[inline(always)]
    pub const fn bytes_per_pixel(&self) -> u32 {
        (((*self as u64) >> 3) & 0x1F) as u32
    }
}

/// Represents supported pixel formats. Defined in [AOSP's](https://android.googlesource.com) [graphics-base-v1.0.h](https://android.googlesource.com/platform/system/core/+/8186c6362183e88bc5254af457baa662b20ca1e8/libsystem/include/system/graphics-base-v1.0.h#12)
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum PixelFormat {
    #[default]
    Invalid = 0,
    RGBA_8888 = 1,
    RGBX_8888 = 2,
    RGB_888 = 3,
    RGB_565 = 4,
    BGRA_8888 = 5,
    RGBA_5551 = 6,
    RGBA_4444 = 7,
    YCRB_420_SP = 17,
    Raw16 = 32,
    Blob = 33,
    ImplementationDefined = 34,
    YCBCR_420_888 = 35,
    Y8 = 0x20203859,
    Y16 = 0x20363159,
    YV12 = 0x32315659,
}

define_bit_set! {
    /// Represents allocator usage flags
    GraphicsAllocatorUsage (u32) {
        SoftwareReadNever = 0,
        SoftwareReadRarely = 0x2,
        SoftwareReadOften = 0x3,
        SoftwareReadMask = 0xF,

        SoftwareWriteNever = 0,
        SoftwareWriteRarely = 0x20,
        SoftwareWriteOften = 0x30,
        SoftwareWriteMask = 0xF0,

        HardwareTexture = 0x100,
        HardwareRender = 0x200,
        Hardware2d = 0x400,
        HardwareComposer = 0x800,
        HardwareFramebuffer = 0x1000,
        HardwareExternalDisplay = 0x2000,
        HardwareProtected = 0x4000,
        HardwareCursor = 0x8000,
        HardwareVideoEncoder = 0x10000,
        HardwareCameraWrite = 0x20000,
        HardwareCameraRead = 0x40000,
        HardwareCameraZSL = 0x60000,
        HardwareCameraMask = 0x60000,
        HardwareMask = 0x71F00,
        RenderScript = 0x100000
    }

}

/// Represents connection APIs
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum ConnectionApi {
    /// Marker for invalid API values.
    #[default]
    Invalid = 0,
    /// Buffers will be queued by EGL via eglSwapBuffers after being filled using OpenGL ES.
    EGL = 1,
    /// Buffers will be queued after being filled using the CPU.
    Cpu = 2,
    /// Buffers will be queued by Stagefright after being filled by a video decoder.
    /// The video decoder can either be a software or hardware decoder.
    Media = 3,
    /// Buffers will be queued by the the camera HAL.
    Camera = 4,
}

/// Represents disconnect modes
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
pub enum DisconnectMode {
    /// Disconnect only the specified API.
    #[default]
    Api,
    /// Disconnect any API originally connected from the process calling disconnect.
    AllLocal,
}

/// Represents a queue buffer output layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct QueueBufferOutput {
    /// The width
    pub width: u32,
    /// The height
    pub height: u32,
    /// The transform hint
    pub transform_hint: u32,
    /// The pending buffer count
    pub pending_buffer_count: u32,
}

impl QueueBufferOutput {
    /// Creates a new, empty [`QueueBufferOutput`]
    pub const fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            transform_hint: 0,
            pending_buffer_count: 0,
        }
    }
}

/// Represents a plane layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct Plane {
    /// The width
    pub width: u32,
    /// The height
    pub height: u32,
    /// The color format
    pub color_format: ColorFormat,
    /// The layout
    pub layout: Layout,
    /// The pitch
    pub pitch: u32,
    /// The map handle
    pub map_handle: u32,
    /// The offset
    pub offset: u32,
    /// The kind
    pub kind: Kind,
    /// The base-2 log of the block height
    pub block_height_log2: BlockLinearHeights,
    /// The display scan format
    pub display_scan_format: DisplayScanFormat,
    /// The second field offset
    pub second_field_offset: u32,
    /// The flags
    pub flags: u64,
    /// The size
    pub size: usize,
    /// Unknown/unused
    pub unk: [u32; 6],
}

/// Represents a graphic buffer header layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct GraphicBufferHeader {
    /// The magic
    pub magic: u32,
    /// The width
    pub width: u32,
    /// The height
    pub height: u32,
    /// The stride
    pub stride: u32,
    /// The pixel format
    pub pixel_format: PixelFormat,
    /// The allocator usage
    pub gfx_alloc_usage: GraphicsAllocatorUsage,
    /// The PID
    pub pid: u32,
    /// The reference count
    pub refcount: u32,
    /// The FD count
    pub fd_count: u32,
    /// The buffer size
    pub buffer_size: u32,
}

impl GraphicBufferHeader {
    /// Represents the magic value of this layout
    pub const MAGIC: u32 = u32::from_be_bytes(*b"GBFR");
}

/// Represents a graphic buffer layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
#[repr(packed)]
pub struct GraphicBuffer {
    /// The header
    pub header: GraphicBufferHeader,
    /// Empty value
    pub unknown: i32,
    /// The map ID
    pub map_id: u32,
    /// Empty value
    pub zero: u32,
    /// The magic
    pub magic: u32,
    /// The PID
    pub pid: u32,
    /// The buffer type
    pub buffer_type: u32,
    /// The allocator usage
    pub gfx_alloc_usage: GraphicsAllocatorUsage,
    /// The pixel format
    pub pixel_format: PixelFormat,
    /// The external pixel format
    pub external_pixel_format: PixelFormat,
    /// The stride
    pub stride: u32,
    /// The full size
    pub full_size: u32,
    /// The plane count
    pub plane_count: u32,
    /// Empty value
    pub unk2: u32,
    /// The planes
    pub planes: [Plane; 3],
    /// Unused
    pub unused: u64,
}

impl GraphicBuffer {
    /// Represents the magic value of this layout
    pub const MAGIC: u32 = 0xDAFFCAFF;
}

/// Represents a fence layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct Fence {
    id: u32,
    value: u32,
}

/// Represents a multiple fence layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct MultiFence {
    fence_count: u32,
    fences: [Fence; 4],
}

/// Represents a rectangle layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

/// Represents a transform type
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum Transform {
    #[default]
    Invalid = 0,
    FlipH = 1,
    FlipV = 2,
    Rotate90 = 4,
    Rotate180 = 3,
    Rotate270 = 7,
}

/// Represents a queue buffer input layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
#[repr(packed)]
pub struct QueueBufferInput {
    timestamp: i64,
    is_auto_timestamp: i32,
    crop: Rect,
    scaling_mode: i32,
    transform: Transform,
    sticky_transform: u32,
    unk: u32,
    swap_interval: u32,
    fences: MultiFence,
}

#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u32)]
pub enum BlockLinearHeights {
    OneGob,
    TwoGobs,
    FourGobs,
    EightGobs,
    #[default]
    SixteenGobs,
    ThirtyTwoGobs,
}

impl BlockLinearHeights {
    #[inline]
    pub const fn block_height_log2(self) -> u32 {
        self as u32
    }

    #[inline]
    pub const fn block_height_bytes(self) -> u32 {
        8 * self.block_height()
    }

    #[inline]
    pub const fn block_height(self) -> u32 {
        1 << self.block_height_log2()
    }
}

const NVHOST_AS_GPU_PATH: &str = "/dev/nvhost-as-gpu\0";
const NVMAP_PATH: &str = "/dev/nvmap\0";
const NVHOST_CTRL_PATH: &str = "/dev/nvhost-ctrl\0";

/// Represents the screen width
pub const SCREEN_WIDTH: u32 = 1280;

/// Represents the screen height
pub const SCREEN_HEIGHT: u32 = 720;

//const SIZE_FACTOR: f32 = (SCREEN_WIDTH as f32) / (SCREEN_HEIGHT as f32);

/// Represents a layer Z value
///
/// This can contain the minimum/maximum possible values, or a custom Z value
pub enum LayerZ {
    /// Always inserts at the front
    Max,
    /// Always inserts at the back
    Min,
    /// Inserts with a specified Z value
    Value(i64),
}

/// Represents `nvdrv:*` service kinds
pub enum NvDrvServiceKind {
    /// "nvdrv"
    Application,
    /// "nvdrv:a"
    Applet,
    /// "nvdrv:s"
    System,
}

/// Represents `vi:*` service kinds
pub enum ViServiceKind {
    /// "vi:u"
    Application,
    // "vi:s"
    System,
    /// "vi:m"
    Manager,
}

/// Converts [`ErrorCode`][`nv::ErrorCode`] to a regular [`Result`]
///
/// # Arguments
///
/// * `err`: The [`ErrorCode`][`nv::ErrorCode`]
#[allow(unreachable_patterns)]
pub fn convert_nv_error_code(err: nv::ErrorCode) -> Result<()> {
    match err {
        nv::ErrorCode::Success => Ok(()),
        nv::ErrorCode::NotImplemented => rc::ResultNvErrorCodeNotImplemented::make_err(),
        nv::ErrorCode::NotSupported => rc::ResultNvErrorCodeNotSupported::make_err(),
        nv::ErrorCode::NotInitialized => rc::ResultNvErrorCodeNotInitialized::make_err(),
        nv::ErrorCode::InvalidParameter => rc::ResultNvErrorCodeInvalidParameter::make_err(),
        nv::ErrorCode::TimeOut => rc::ResultNvErrorCodeTimeOut::make_err(),
        nv::ErrorCode::InsufficientMemory => rc::ResultNvErrorCodeInsufficientMemory::make_err(),
        nv::ErrorCode::ReadOnlyAttribute => rc::ResultNvErrorCodeReadOnlyAttribute::make_err(),
        nv::ErrorCode::InvalidState => rc::ResultNvErrorCodeInvalidState::make_err(),
        nv::ErrorCode::InvalidAddress => rc::ResultNvErrorCodeInvalidAddress::make_err(),
        nv::ErrorCode::InvalidSize => rc::ResultNvErrorCodeInvalidSize::make_err(),
        nv::ErrorCode::InvalidValue => rc::ResultNvErrorCodeInvalidValue::make_err(),
        nv::ErrorCode::AlreadyAllocated => rc::ResultNvErrorCodeAlreadyAllocated::make_err(),
        nv::ErrorCode::Busy => rc::ResultNvErrorCodeBusy::make_err(),
        nv::ErrorCode::ResourceError => rc::ResultNvErrorCodeResourceError::make_err(),
        nv::ErrorCode::CountMismatch => rc::ResultNvErrorCodeCountMismatch::make_err(),
        nv::ErrorCode::SharedMemoryTooSmall => {
            rc::ResultNvErrorCodeSharedMemoryTooSmall::make_err()
        }
        nv::ErrorCode::FileOperationFailed => rc::ResultNvErrorCodeFileOperationFailed::make_err(),
        nv::ErrorCode::IoctlFailed => rc::ResultNvErrorCodeIoctlFailed::make_err(),
        _ => rc::ResultNvErrorCodeInvalid::make_err(),
    }
}

/// A holder for our `*RootService` objects, just to keep them alive for the lifetime of the `Context`
pub enum RootServiceHolder {
    /// Application Service
    Application(ApplicationDisplayRootService),
    /// Manager Service
    Manager(ManagerDisplayRootService),
    /// System Service
    System(SystemDisplayRootService),
}

/// A holder for our `nvdrv` service objects
pub enum NvDrvServiceHolder {
    /// Application Service
    Application(nv::ApplicationNvDrvService),
    /// Applet Service
    Applet(nv::AppletNvDrvService),
    /// System Service
    System(nv::SystemNvDrvService),
}

impl NvDrvServiceHolder {
    fn open(&self, path: sf::InMapAliasBuffer<'_, u8>) -> Result<(Fd, ErrorCode)> {
        match self {
            Self::Application(s) => s.open(path),
            Self::Applet(s) => s.open(path),
            Self::System(s) => s.open(path),
        }
    }
    fn ioctl(
        &self,
        fd: Fd,
        id: IoctlId,
        in_buf: sf::InOutAutoSelectBuffer<'_, u8>,
    ) -> Result<ErrorCode> {
        match self {
            Self::Application(s) => s.ioctl(fd, id, in_buf),
            Self::Applet(s) => s.ioctl(fd, id, in_buf),
            Self::System(s) => s.ioctl(fd, id, in_buf),
        }
    }
    fn close(&self, fd: Fd) -> Result<ErrorCode> {
        match self {
            Self::Application(s) => s.close(fd),
            Self::Applet(s) => s.close(fd),
            Self::System(s) => s.close(fd),
        }
    }
    fn initialize(
        &self,
        transfer_mem_size: u32,
        self_process_handle: sf::CopyHandle,
        transfer_mem_handle: sf::CopyHandle,
    ) -> Result<ErrorCode> {
        match self {
            Self::Application(s) => {
                s.initialize(transfer_mem_size, self_process_handle, transfer_mem_handle)
            }
            Self::Applet(s) => {
                s.initialize(transfer_mem_size, self_process_handle, transfer_mem_handle)
            }
            Self::System(s) => {
                s.initialize(transfer_mem_size, self_process_handle, transfer_mem_handle)
            }
        }
    }

    fn close_self(&mut self) {
        use crate::ipc::client::IClientObject;
        match self {
            Self::Application(s) => s.get_session_mut().close(),
            Self::Applet(s) => s.get_session_mut().close(),
            Self::System(s) => s.get_session_mut().close(),
        }
    }
}

/// Represents a graphics context
#[allow(dead_code)]
pub struct Context {
    vi_service: RootServiceHolder,
    nvdrv_service: NvDrvServiceHolder,
    application_display_service: ApplicationDisplay,
    hos_binder_driver: Arc<dispdrv::HOSBinderDriver>,
    transfer_mem: alloc::Buffer<u8>,
    transfer_mem_handle: svc::Handle,
    nvhost_fd: svc::Handle,
    nvmap_fd: svc::Handle,
    nvhostctrl_fd: svc::Handle,
}

impl Context {
    /// Creates a new [`Context`]
    ///
    /// This automatically accesses VI and NV [`INvDrvClient`] services (of the specified kinds) and creates NV transfer memory
    ///
    /// # Arguments
    ///
    /// * `nv_kind`: The [`NvDrvServiceKind`]
    /// * `vi_kind`: The [`ViServiceKind`]
    /// * `transfer_mem_size`: The transfer memory size to use
    pub fn new(
        nv_kind: NvDrvServiceKind,
        vi_kind: ViServiceKind,
        transfer_mem_size: usize,
    ) -> Result<Self> {
        let (vi_srv, application_display_srv) = match vi_kind {
            ViServiceKind::Manager => {
                use vi::IManagerDisplayRootClient;
                let vi_srv = service::new_service_object::<ManagerDisplayRootService>()?;
                let app_disp_srv =
                    vi_srv.get_display_service(vi::DisplayServiceMode::Privileged)?;

                (RootServiceHolder::Manager(vi_srv), app_disp_srv)
            }
            ViServiceKind::System => {
                use vi::ISystemDisplayRootClient;
                let vi_srv = service::new_service_object::<SystemDisplayRootService>()?;
                let app_disp_srv =
                    vi_srv.get_display_service(vi::DisplayServiceMode::Privileged)?;

                (RootServiceHolder::System(vi_srv), app_disp_srv)
            }
            ViServiceKind::Application => {
                use vi::IApplicationDisplayRootClient;
                let vi_srv = service::new_service_object::<ApplicationDisplayRootService>()?;
                let app_disp_srv = vi_srv.get_display_service(vi::DisplayServiceMode::User)?;

                (RootServiceHolder::Application(vi_srv), app_disp_srv)
            }
        };

        let nvdrv_srv = match nv_kind {
            NvDrvServiceKind::Application => {
                NvDrvServiceHolder::Application(service::new_service_object::<
                    nv::ApplicationNvDrvService,
                >()?)
            }
            NvDrvServiceKind::Applet => {
                NvDrvServiceHolder::Applet(service::new_service_object::<nv::AppletNvDrvService>()?)
            }
            NvDrvServiceKind::System => {
                NvDrvServiceHolder::System(service::new_service_object::<nv::SystemNvDrvService>()?)
            }
        };

        Self::from(
            vi_srv,
            application_display_srv,
            nvdrv_srv,
            transfer_mem_size,
            !matches!(nv_kind, NvDrvServiceKind::System),
        )
    }

    /// Creates a new [`Context`] with already existing service objects
    ///
    /// This automatically creates NV transfer memory
    ///
    /// # Arguments
    ///
    /// * `vi_srv`: The VI service object
    /// * `application_display_srv`: The vi [`IApplicationDisplayClient`] interface object
    /// * `nvdrv_srv`: The NV [`INvDrvClient`] service object
    /// * `transfer_mem_size`: The transfer memory size to use
    /// * `nv_host_as_gpu`: Flag whether to open a handle to the GPU for hardware accelerated rendering.
    fn from(
        vi_srv: RootServiceHolder,
        application_display_srv: ApplicationDisplay,
        mut nvdrv_srv: NvDrvServiceHolder,
        transfer_mem_size: usize,
        nv_host_as_gpu: bool,
    ) -> Result<Self> {
        let transfer_mem = alloc::Buffer::new(alloc::PAGE_ALIGNMENT, transfer_mem_size)?;
        let transfer_mem_handle = svc::create_transfer_memory(
            transfer_mem.ptr,
            transfer_mem_size,
            svc::MemoryPermission::None(),
        )?;
        if let Err(rc) = nvdrv_srv.initialize(
            transfer_mem_size as u32,
            sf::Handle::from(svc::CURRENT_PROCESS_PSEUDO_HANDLE),
            sf::Handle::from(transfer_mem_handle),
        ) {
            let _ = svc::close_handle(transfer_mem_handle);
            let _ = wait_for_permission(transfer_mem.ptr, MemoryPermission::Write(), None);
            return Err(rc);
        };

        // wrap this up in a try block so we don't need to call into a function for `?` flow control -
        // we need this to make sure we request the transfer memory back from the GPU.
        let (mut nvhost_fd, mut nvmap_fd, mut nvhostctrl_fd) = (0, 0, 0);
        let hos_binder_drv = match try {
            if nv_host_as_gpu {
                let (fd, err) =
                    nvdrv_srv.open(sf::Buffer::from_array(NVHOST_AS_GPU_PATH.as_bytes()))?;
                convert_nv_error_code(err)?;
                nvhost_fd = fd;
            }

            let (fd, err) = nvdrv_srv.open(sf::Buffer::from_array(NVMAP_PATH.as_bytes()))?;
            convert_nv_error_code(err)?;
            nvmap_fd = fd;

            let (fd, err) = nvdrv_srv.open(sf::Buffer::from_array(NVHOST_CTRL_PATH.as_bytes()))?;
            convert_nv_error_code(err)?;
            nvhostctrl_fd = fd;

            application_display_srv.get_relay_service()?
        } {
            Ok(binder) => binder,
            Err(rc) => {
                let _ = nvdrv_srv.close(nvhost_fd);
                let _ = nvdrv_srv.close(nvmap_fd);
                let _ = nvdrv_srv.close(nvhostctrl_fd);

                let _ = nvdrv_srv.close(transfer_mem_handle);
                nvdrv_srv.close_self();
                svc::close_handle(transfer_mem_handle).unwrap();
                let _ = wait_for_permission(transfer_mem.ptr, MemoryPermission::Write(), None);
                return Err(rc);
            }
        };

        Ok(Self {
            vi_service: vi_srv,
            nvdrv_service: nvdrv_srv,
            application_display_service: application_display_srv,
            hos_binder_driver: Arc::new(hos_binder_drv),
            transfer_mem,
            transfer_mem_handle,
            nvhost_fd,
            nvmap_fd,
            nvhostctrl_fd,
        })
    }

    /// Gets the wrapped NV [`INvDrvClient`] service object
    pub fn nvdrv_service(&self) -> &NvDrvServiceHolder {
        &self.nvdrv_service
    }

    /// Gets the wrapped NV [`INvDrvClient`] service object mutably
    pub fn nvdrv_service_mut(&mut self) -> &mut NvDrvServiceHolder {
        &mut self.nvdrv_service
    }

    /// Gets the underlying [`IApplicationDisplayClient`] object
    pub fn get_application_display_service(&self) -> &dyn vi::IApplicationDisplayClient {
        &self.application_display_service
    }

    /// Gets the underlying [`IApplicationDisplayClient`] object mutably
    pub fn get_application_display_service_mut(
        &mut self,
    ) -> &mut dyn vi::IApplicationDisplayClient {
        &mut self.application_display_service
    }

    /// Gets the underlying [`IHOSBinderDriverClient`][`dispdrv::IHOSBinderDriverClient`] object
    pub fn get_hos_binder_driver(&self) -> Arc<dyn dispdrv::IHOSBinderDriverClient> {
        self.hos_binder_driver.clone()
    }
}

impl Drop for Context {
    /// Destroys the [`Context`], closing everything it opened when it was created
    #[allow(unused_must_use)]
    fn drop(&mut self) {
        self.nvdrv_service.close(self.nvhost_fd);
        self.nvdrv_service.close(self.nvmap_fd);
        self.nvdrv_service.close(self.nvhostctrl_fd);

        self.nvdrv_service.close(self.transfer_mem_handle);
        self.nvdrv_service.close_self();
        svc::close_handle(self.transfer_mem_handle).unwrap();
        wait_for_permission(self.transfer_mem.ptr, MemoryPermission::Write(), None);
    }
}
