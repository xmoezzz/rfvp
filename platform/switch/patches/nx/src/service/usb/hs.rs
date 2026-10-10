use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::usb::hs::*;

ipc_client_define_client_default!(ClientRootSessionService);
impl IClientRootSessionClient for ClientRootSessionService {}

impl service::IService for ClientRootSessionService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("usb:hs")
    }

    fn as_domain() -> bool {
        true
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
