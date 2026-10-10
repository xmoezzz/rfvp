use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::vi::*;

ipc_client_define_client_default!(ApplicationDisplayRootService);
ipc_client_define_client_default!(ManagerDisplayRootService);
ipc_client_define_client_default!(SystemDisplayRootService);

impl IApplicationDisplayRootClient for ApplicationDisplayRootService {}
impl IManagerDisplayRootClient for ManagerDisplayRootService {}
impl ISystemDisplayRootClient for SystemDisplayRootService {}

impl service::IService for ApplicationDisplayRootService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("vi:u")
    }

    fn as_domain() -> bool {
        true
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

impl service::IService for SystemDisplayRootService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("vi:s")
    }

    fn as_domain() -> bool {
        true
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

impl service::IService for ManagerDisplayRootService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("vi:m")
    }

    fn as_domain() -> bool {
        true
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
