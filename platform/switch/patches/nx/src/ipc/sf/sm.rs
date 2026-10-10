use crate::ipc::sf;
use crate::util;
use crate::version;

pub mod mitm;

pub mod rc;

use nx_derive::{Request, Response};

#[derive(Request, Response, Copy, Clone, Eq)]
#[repr(C)]
pub union ServiceName {
    pub value: u64,
    pub name: [u8; 8],
}
const_assert!(core::mem::size_of::<ServiceName>() == 0x8);

impl ServiceName {
    pub const fn from(value: u64) -> Self {
        Self { value }
    }

    pub const fn new(name: &str) -> Self {
        let mut raw_name: [u8; 8] = [0; 8];

        let name_bytes = name.as_bytes();
        let copy_len = util::const_usize_min(8, name_bytes.len());

        unsafe {
            core::ptr::copy_nonoverlapping(name_bytes.as_ptr(), raw_name.as_mut_ptr(), copy_len)
        }

        Self { name: raw_name }
    }

    pub const fn empty() -> Self {
        Self::from(0)
    }

    pub const fn is_empty(&self) -> bool {
        unsafe { self.value == 0 }
    }
}

impl PartialEq for ServiceName {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.value == other.value }
    }
}

impl core::fmt::Debug for ServiceName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        unsafe { self.name.fmt(f) }
    }
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait UserInterface {
    #[ipc_rid(0)]
    fn register_client(&self, process_id: sf::ProcessId);
    #[ipc_rid(1)]
    fn get_service_handle(&self, name: ServiceName) -> sf::MoveHandle;
    #[ipc_rid(2)]
    fn register_service(
        &self,
        name: ServiceName,
        is_light: bool,
        max_sessions: i32,
    ) -> sf::MoveHandle;
    #[ipc_rid(3)]
    fn unregister_service(&self, name: ServiceName);
    #[ipc_rid(4)]
    #[version(version::VersionInterval::from(version::Version::new(11, 0, 0)))]
    fn detach_client(&self, process_id: sf::ProcessId);
    #[ipc_rid(65000)]
    fn atmosphere_install_mitm(&self, name: ServiceName) -> (sf::MoveHandle, sf::MoveHandle);
    #[ipc_rid(65001)]
    fn atmosphere_uninstall_mitm(&self, name: ServiceName);
    #[ipc_rid(65003)]
    fn atmosphere_acknowledge_mitm_session(
        &self,
        name: ServiceName,
    ) -> (mitm::MitmProcessInfo, sf::MoveHandle);
    #[ipc_rid(65004)]
    fn atmosphere_has_mitm(&self, name: ServiceName) -> bool;
    #[ipc_rid(65005)]
    fn atmosphere_wait_mitm(&self, name: ServiceName);
    #[ipc_rid(65006)]
    fn atmosphere_declare_future_mitm(&self, name: ServiceName);
    #[ipc_rid(65007)]
    fn atmosphere_clear_future_mitm(&self, name: ServiceName);
    #[ipc_rid(65100)]
    fn atmosphere_has_service(&self, name: ServiceName) -> bool;
    #[ipc_rid(65101)]
    fn atmosphere_wait_service(&self, name: ServiceName);
}
