use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::nv::*;

ipc_client_define_client_default!(ApplicationNvDrvService);

impl INvDrvClient for ApplicationNvDrvService {}

impl service::IService for ApplicationNvDrvService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("nvdrv")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

ipc_client_define_client_default!(AppletNvDrvService);

impl INvDrvClient for AppletNvDrvService {}

impl service::IService for AppletNvDrvService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("nvdrv:a")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

ipc_client_define_client_default!(SystemNvDrvService);

impl INvDrvClient for SystemNvDrvService {}

impl service::IService for SystemNvDrvService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("nvdrv:s")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
