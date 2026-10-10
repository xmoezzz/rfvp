use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::fatal::*;

ipc_client_define_client_default!(FatalService);
impl IFatalClient for FatalService {}

impl service::IService for FatalService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("fatal:u")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
