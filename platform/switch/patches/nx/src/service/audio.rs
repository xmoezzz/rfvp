use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::audio::*;

ipc_client_define_client_default!(AudioOutManagerService);
impl IAudioOutManagerClient for AudioOutManagerService {}

impl service::IService for AudioOutManagerService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("audout:u")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

ipc_client_define_client_default!(AudioInManagerService);
impl IAudioInManagerClient for AudioInManagerService {}

impl service::IService for AudioInManagerService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("audin:u")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
