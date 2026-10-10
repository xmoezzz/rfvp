use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::mii::*;

ipc_client_define_client_default!(StaticService);
impl IStaticClient for StaticService {}

impl service::IService for StaticService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("mii:e")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
