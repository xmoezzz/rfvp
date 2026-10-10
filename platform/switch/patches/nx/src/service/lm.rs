use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::lm::*;

ipc_client_define_client_default!(LoggingService);

impl ILoggingClient for LoggingService {}

impl service::IService for LoggingService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("lm")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
