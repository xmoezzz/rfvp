use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::psm::*;

ipc_client_define_client_default!(PsmService);

impl IPsmClient for PsmService {}

impl service::IService for PsmService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("psm")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
