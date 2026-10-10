use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::spl::*;

ipc_client_define_client_default!(RandomService);

impl IRandomClient for RandomService {}

impl service::IService for RandomService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("csrng")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
