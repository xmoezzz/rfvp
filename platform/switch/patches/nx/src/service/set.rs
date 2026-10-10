use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::set::*;

ipc_client_define_client_default!(SystemSettingsService);

impl ISystemSettingsClient for SystemSettingsService {}

impl service::IService for SystemSettingsService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("set:sys")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
