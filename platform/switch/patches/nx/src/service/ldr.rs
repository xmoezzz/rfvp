use crate::ipc::sf::sm;
use crate::result::*;
use crate::service;

pub use crate::ipc::sf::ldr::*;

ipc_client_define_client_default!(ShellInterfaceService);
impl IShellInterfaceClient for ShellInterfaceService {}

impl service::IService for ShellInterfaceService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("ldr:shel")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
