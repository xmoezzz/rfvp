use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::hid::*;

ipc_client_define_client_default!(HidService);
ipc_client_define_client_default!(HidSysService);

impl IHidClient for HidService {}
impl IHidSysClient for HidSysService {}

impl service::IService for HidService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("hid")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

impl service::IService for HidSysService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("hid:sys")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
