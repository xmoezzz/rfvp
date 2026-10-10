use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::psc::*;

ipc_client_define_client_default!(PmService);
impl IPmClient for PmService {}

impl service::IService for PmService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("psc:m")
    }

    fn as_domain() -> bool {
        true
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
