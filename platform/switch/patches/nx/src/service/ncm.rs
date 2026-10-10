use super::*;
use crate::service;

pub use crate::ipc::sf::ncm::*;

ipc_client_define_client_default!(ContentManagerService);
impl IContentManagerClient for ContentManagerService {}

impl service::IService for ContentManagerService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("ncm")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
