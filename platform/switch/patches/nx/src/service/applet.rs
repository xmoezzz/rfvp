use crate::hbl::AppletType;
use crate::ipc::sf::sm;
use crate::result::*;
use crate::{hbl, service};

pub use crate::ipc::sf::applet::*;

ipc_client_define_client_default!(AllSystemAppletProxiesService);
impl IAllSystemAppletProxiesClient for AllSystemAppletProxiesService {}

impl service::IService for AllSystemAppletProxiesService {
    fn get_name() -> sm::ServiceName {
        // we only want to
        let applet_type = hbl::get_applet_type();
        sm::ServiceName::new(
            if applet_type == AppletType::Application || applet_type == AppletType::Default {
                "appletOE"
            } else {
                "appletAE"
            },
        )
    }

    fn as_domain() -> bool {
        true
    }

    fn post_initialize(&mut self) -> Result<()> {
        Ok(())
    }
}
