use crate::ipc::sf;
use crate::version;
use crate::{result::*, util};

use nx_derive::{Request, Response};

define_bit_set! {
    DeviceFilterFlags (u16) {
        IdVendor = bit!(0),
        IdProduct = bit!(1),
        DeviceMin = bit!(2),
        DeviceMax = bit!(3),
        DeviceClass = bit!(4),
        DeviceSubClass = bit!(5),
        DeviceProtocol = bit!(6),
        InterfaceClass = bit!(7),
        InterfaceSubClass = bit!(8),
        InterfaceProtocol = bit!(9)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct DeviceFilter {
    pub flags: DeviceFilterFlags,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_min_bcd: u16,
    pub device_max_bcd: u16,
    pub device_class: super::ClassCode,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub interface_class: super::ClassCode,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
}
const_assert!(core::mem::size_of::<DeviceFilter>() == 0x10);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum InterfaceAvailableEventId {
    Unk0 = 0,
    Unk1 = 1,
    Unk2 = 2,
}
#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C, packed)]
pub struct InterfaceProfile {
    pub id: u32,
    pub device_id: u32,
    pub unk: [u8; 0x4],
    pub interface_descriptor: super::InterfaceDescriptor,
    pub pad_1: [u8; 0x7],
    pub output_endpoint_descriptors: [super::EndPointDescriptor; 15],
    pub pad_2: [u8; 0x7],
    pub input_endpoint_descriptors: [super::EndPointDescriptor; 15],
    pub pad_3: [u8; 0x6],
    pub output_ss_endpoint_companion_descriptors: [super::SsEndPointCompanionDescriptor; 15],
    pub pad_4: [u8; 0x6],
    pub input_ss_endpoint_companion_descriptors: [super::SsEndPointCompanionDescriptor; 15],
    pub pad_5: [u8; 0x3],
}
const_assert!(core::mem::size_of::<InterfaceProfile>() == 0x1B8);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct InterfaceInfo {
    pub unk_str: util::ArrayString<0x40>,
    pub bus_id: u32,
    pub device_id: u32,
    pub device_descriptor: super::DeviceDescriptor,
    pub config_descriptor: super::ConfigDescriptor,
    pub pad: [u8; 0x5],
    pub unk_maybe_timestamp: u64,
}
const_assert!(core::mem::size_of::<InterfaceInfo>() == 0x70);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct InterfaceQueryOutput {
    pub profile: InterfaceProfile,
    pub info: InterfaceInfo,
}
const_assert!(core::mem::size_of::<InterfaceQueryOutput>() == 0x228);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct XferReport {
    pub xfer_id: u32,
    pub result: ResultCode,
    pub requested_size: u32,
    pub transferred_size: u32,
    pub unk: [u8; 8],
}
const_assert!(core::mem::size_of::<XferReport>() == 0x18);

#[nx_derive::ipc_trait]
#[default_client]
pub trait ClientEpSession {
    #[ipc_rid(0)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn submit_out_request(&self, size: u32, unk: u32, buf: sf::InMapAliasBuffer<'_, u8>) -> u32;
    #[ipc_rid(0)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn re_open(&self);
    #[ipc_rid(1)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn submit_in_request(&self, size: u32, unk: u32, out_buf: sf::OutMapAliasBuffer<'_, u8>)
    -> u32;
    #[ipc_rid(1)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn close(&self);
    #[ipc_rid(2)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn reset(&self);
    #[ipc_rid(2)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn get_completion_event(&self) -> sf::CopyHandle;
    #[ipc_rid(3)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn close_deprecated(&self);
    #[ipc_rid(3)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn populate_ring(&self);
    #[ipc_rid(4)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn post_buffer_async(&self, size: u32, buf_addr: u64, unk: u64) -> u32;
    #[ipc_rid(5)]
    #[version(version::VersionInterval::from_to(
        version::Version::new(2, 0, 0),
        version::Version::new(2, 3, 0)
    ))]
    fn get_xfer_report_deprecated(
        &self,
        count: u32,
        out_reports_buf: sf::OutMapAliasBuffer<'_, XferReport>,
    ) -> u32;
    #[ipc_rid(5)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn get_xfer_report(
        &self,
        count: u32,
        out_reports_buf: sf::OutAutoSelectBuffer<'_, XferReport>,
    ) -> u32;
    #[ipc_rid(6)]
    #[version(version::VersionInterval::from_to(
        version::Version::new(2, 0, 0),
        version::Version::new(2, 3, 0)
    ))]
    fn batch_buffer_async_deprecated(
        &self,
        urb_count: u32,
        unk_1: u32,
        unk_2: u32,
        buf_addr: u64,
        unk_3: u64,
        urb_sizes_buf: sf::InMapAliasBuffer<'_, u32>,
    ) -> u32;
    #[ipc_rid(6)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn batch_buffer_async(
        &self,
        urb_count: u32,
        unk_1: u32,
        unk_2: u32,
        buf_addr: u64,
        unk_3: u64,
        urb_sizes_buf: sf::InAutoSelectBuffer<'_, u32>,
    ) -> u32;
    #[ipc_rid(7)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn create_smmu_space(&self, unk: [u8; 0x10]);
    #[ipc_rid(8)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn share_report_ring(&self, unk: [u8; 0x4], unk_handle: sf::CopyHandle);
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait ClientIfSession {
    #[ipc_rid(0)]
    fn get_state_change_event(&self) -> sf::CopyHandle;
    #[ipc_rid(1)]
    fn set_interface(&self, unk: u8, profile_buf: sf::InMapAliasBuffer<'_, InterfaceProfile>);
    #[ipc_rid(2)]
    fn get_interface(&self, out_profile_buf: sf::OutMapAliasBuffer<'_, InterfaceProfile>);
    #[ipc_rid(3)]
    fn get_alternate_interface(
        &self,
        unk: u8,
        out_profile_buf: sf::OutMapAliasBuffer<'_, InterfaceProfile>,
    );
    #[ipc_rid(5)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn get_current_frame_deprecated(&self) -> u32;
    #[ipc_rid(4)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn get_current_frame(&self) -> u32;
    #[ipc_rid(5)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn ctrl_xfer_async(
        &self,
        request_type: u8,
        request: u8,
        val: u16,
        idx: u16,
        length: u16,
        buf_addr: u64,
    );
    #[ipc_rid(6)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn submit_control_in_request(
        &self,
        request: u8,
        request_type: u8,
        val: u16,
        idx: u16,
        length: u16,
        timeout_ms: u32,
        out_buf: sf::OutMapAliasBuffer<'_, u8>,
    ) -> u32;
    #[ipc_rid(6)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn get_ctrl_xfer_completion_event(&self) -> sf::CopyHandle;
    #[ipc_rid(7)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn submit_control_out_request(
        &self,
        request: u8,
        request_type: u8,
        val: u16,
        idx: u16,
        length: u16,
        timeout_ms: u32,
        buf: sf::InMapAliasBuffer<'_, u8>,
    ) -> u32;
    #[ipc_rid(7)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn get_ctrl_xfer_report(&self, out_report_buf: sf::OutMapAliasBuffer<'_, XferReport>);
    #[ipc_rid(8)]
    fn reset_device(&self, unk: u32);
    #[ipc_rid(4)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn open_usb_ep_deprecated(
        &self,
        max_urb_count: u16,
        ep_type: u32,
        ep_number: u32,
        ep_direction: u32,
        max_xfer_size: u32,
    ) -> (super::EndPointDescriptor, ClientEpSession);
    #[ipc_rid(9)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn open_usb_ep(
        &self,
        max_urb_count: u16,
        ep_type: u32,
        ep_number: u32,
        ep_direction: u32,
        max_xfer_size: u32,
    ) -> (super::EndPointDescriptor, ClientEpSession);
}

#[nx_derive::ipc_trait]
pub trait ClientRootSession {
    #[ipc_rid(0)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn bind_client_process(&self, self_process_handle: sf::CopyHandle);
    #[ipc_rid(0)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn query_all_interfaces_deprecated(
        &self,
        filter: DeviceFilter,
        out_intfs: sf::OutMapAliasBuffer<'_, InterfaceQueryOutput>,
    ) -> u32;
    #[ipc_rid(1)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn query_all_interfaces(
        &self,
        filter: DeviceFilter,
        out_intfs: sf::OutMapAliasBuffer<'_, InterfaceQueryOutput>,
    ) -> u32;
    #[ipc_rid(1)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn query_available_interfaces_deprecated(
        &self,
        filter: DeviceFilter,
        out_intfs: sf::OutMapAliasBuffer<'_, InterfaceQueryOutput>,
    ) -> u32;
    #[ipc_rid(2)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn query_available_interfaces(
        &self,
        filter: DeviceFilter,
        out_intfs: sf::OutMapAliasBuffer<'_, InterfaceQueryOutput>,
    ) -> u32;
    #[ipc_rid(2)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn query_acquired_interfaces_deprecated(
        &self,
        out_intfs: sf::OutMapAliasBuffer<'_, InterfaceQueryOutput>,
    ) -> u32;
    #[ipc_rid(3)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn query_acquired_interfaces(
        &self,
        out_intfs: sf::OutMapAliasBuffer<'_, InterfaceQueryOutput>,
    ) -> u32;
    #[ipc_rid(3)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn create_interface_available_event_deprecated(
        &self,
        event_id: InterfaceAvailableEventId,
        filter: DeviceFilter,
    ) -> sf::CopyHandle;
    #[ipc_rid(4)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn create_interface_available_event(
        &self,
        event_id: InterfaceAvailableEventId,
        filter: DeviceFilter,
    ) -> sf::CopyHandle;
    #[ipc_rid(4)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn destroy_interface_available_event_deprecated(&self, event_id: InterfaceAvailableEventId);
    #[ipc_rid(5)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn destroy_interface_available_event(&self, event_id: InterfaceAvailableEventId);
    #[ipc_rid(5)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn get_interface_state_change_event_deprecated(&self) -> sf::CopyHandle;
    #[ipc_rid(6)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn get_interface_state_change_event(&self) -> sf::CopyHandle;
    #[ipc_rid(6)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn acquire_usb_if_deprecated(
        &self,
        id: u32,
        out_profile_buf: sf::OutMapAliasBuffer<'_, InterfaceProfile>,
    ) -> ClientIfSession;
    #[ipc_rid(7)]
    #[version(version::VersionInterval::from(version::Version::new(2, 0, 0)))]
    fn acquire_usb_if(
        &self,
        id: u32,
        out_info_buf: sf::OutMapAliasBuffer<'_, InterfaceInfo>,
        out_profile_buf: sf::OutMapAliasBuffer<'_, InterfaceProfile>,
    ) -> ClientIfSession;
    #[ipc_rid(7)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn get_descriptor_string(
        &self,
        unk_1: u8,
        unk_2: bool,
        unk_maybe_id: u32,
        out_desc_buf: sf::OutMapAliasBuffer<'_, u8>,
    ) -> u32;
    #[ipc_rid(8)]
    #[version(version::VersionInterval::to(version::Version::new(1, 0, 0)))]
    fn reset_device(&self, unk: u32);
}
