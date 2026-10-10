use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::pm::*;

ipc_client_define_client_default!(InformationInterfaceService);
impl IInformationInterfaceClient for InformationInterfaceService {}

impl service::IService for InformationInterfaceService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("pm:info")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

ipc_client_define_client_default!(DebugMonitorInterfaceService);
impl IDebugMonitorInterfaceClient for DebugMonitorInterfaceService {}

impl service::IService for DebugMonitorInterfaceService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("pm:dmnt")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
