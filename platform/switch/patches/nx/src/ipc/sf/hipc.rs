use crate::ipc::sf;
use crate::ipc::sf::sm;
use crate::result::*;
use crate::version;

pub mod rc;

// Interfaces related to core serverside IPC (for control requests and MitM support)

ipc_sf_define_control_interface_trait! {
    trait IHipcManager {
        convert_current_object_to_domain [0, version::VersionInterval::all()]: () => (domain_object_id: u32);
        copy_from_current_domain [1, version::VersionInterval::all()]: (domain_object_id: u32) =>  (handle: sf::MoveHandle);
        clone_current_object [2, version::VersionInterval::all()]: () => (cloned_handle: sf::MoveHandle);
        query_pointer_buffer_size [3, version::VersionInterval::all()]: () => (pointer_buffer_size: u16);
        clone_current_object_ex [4, version::VersionInterval::all()]: (tag: u32) =>  (cloned_handle: sf::MoveHandle);
    }
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait MitmQueryService {
    #[ipc_rid(65000)]
    fn should_mitm(&self, info: sm::mitm::MitmProcessInfo) -> bool;
}
