use super::*;
use crate::service;

pub use crate::ipc::sf::lr::*;

ipc_client_define_client_default!(LocationResolverManagerService);
impl ILocationResolverManagerClient for LocationResolverManagerService {}

impl service::IService for LocationResolverManagerService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("lr")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
