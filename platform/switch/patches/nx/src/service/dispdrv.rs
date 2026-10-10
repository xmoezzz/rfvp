use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::dispdrv::*;

/// This one gets special pribileges of not being suffixed with `Service` as it is the only implementor of `IService` that
/// can be received from a call to a different service.
impl service::IService for HOSBinderDriver {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("dispdrv")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
