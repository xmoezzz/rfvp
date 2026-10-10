use crate::ipc::sf::sm;
use crate::result::Result;
use crate::service;

pub use crate::ipc::sf::bsd::*;

ipc_client_define_client_default!(SystemBsdService);
impl IBsdClient for SystemBsdService {}

impl service::IService for SystemBsdService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("bsd:s")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

ipc_client_define_client_default!(AppletBsdService);
impl IBsdClient for AppletBsdService {}

impl service::IService for AppletBsdService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("bsd:a")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

ipc_client_define_client_default!(UserBsdService);
impl IBsdClient for UserBsdService {}

impl service::IService for UserBsdService {
    fn get_name() -> sm::ServiceName {
        sm::ServiceName::new("bsd:u")
    }

    fn as_domain() -> bool {
        false
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

pub enum BsdSrvkind {
    /// "bsd:u"
    User,
    /// "bsd:a"
    Applet,
    /// "bsd:s"
    System,
}
