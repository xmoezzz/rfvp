use crate::ipc::sf;
use crate::version;

use nx_derive::{Request, Response};

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum RefcountType {
    Weak,
    Strong,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ParcelTransactionId {
    RequestBuffer = 1,
    SetBufferCount = 2,
    DequeueBuffer = 3,
    DetachBuffer = 4,
    DetachNextBuffer = 5,
    AttachBuffer = 6,
    QueueBuffer = 7,
    CancelBuffer = 8,
    Query = 9,
    Connect = 10,
    Disconnect = 11,
    SetSidebandStream = 12,
    AllocateBuffers = 13,
    SetPreallocatedBuffer = 14,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum NativeHandleType {
    BufferEvent = 0xF,
}

pub type BinderHandle = i32;

#[nx_derive::ipc_trait]
#[default_client]
pub trait HOSBinderDriver {
    #[ipc_rid(0)]
    fn transact_parcel(
        &self,
        binder_handle: BinderHandle,
        transaction_id: ParcelTransactionId,
        flags: u32,
        in_parcel: sf::InMapAliasBuffer<'_, u8>,
        out_parcel: sf::OutMapAliasBuffer<'_, u8>,
    );
    #[ipc_rid(1)]
    fn adjust_refcount(
        &self,
        binder_handle: BinderHandle,
        add_value: i32,
        refcount_type: RefcountType,
    );
    #[ipc_rid(2)]
    fn get_native_handle(
        &self,
        binder_handle: BinderHandle,
        handle_type: NativeHandleType,
    ) -> sf::CopyHandle;
    #[ipc_rid(3)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn transact_parcel_auto(
        &self,
        binder_handle: BinderHandle,
        transaction_id: ParcelTransactionId,
        flags: u32,
        in_parcel: sf::InAutoSelectBuffer<'_, u8>,
        out_parcel: sf::OutAutoSelectBuffer<'_, u8>,
    );
}
