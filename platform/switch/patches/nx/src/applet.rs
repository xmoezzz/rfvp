//! AppletAE/AppletOE Support

use crate::hbl::{AppletType, get_applet_type};
use crate::ipc::sf;
use crate::result::*;
use crate::service;
use crate::svc;
use crate::sync::{ReadGuard, RwLock};
use crate::version::{Version, get_version};

use core::sync::atomic::AtomicU64;

pub use crate::service::applet::*;

static ALL_SYSTEM_APPLET_PROXY_SERVICE: RwLock<Option<AllSystemAppletProxiesService>> =
    RwLock::new(None);
static LIBRARY_APPLET_PROXY: RwLock<Option<AppletProxy>> = RwLock::new(None);
static WINDOW_CONTROLLER: RwLock<Option<WindowController>> = RwLock::new(None);

/// Global AppletResourceUserID.
/// Stored as part of `applet::initialize()`
pub static GLOBAL_ARUID: AtomicU64 = AtomicU64::new(0);

/// Proxy type to avoid passing boxed trait objects for Applet Proxy actions.
pub enum AppletProxy {
    /// AppletType::Application | AppletType::Default
    Application(ApplicationProxy),
    /// AppletType::SystemApplet
    SystemApplet(SystemAppletProxy),
    /// AppletType::LibraryApplet
    LibraryApplet(LibraryAppletProxy),
    /// AppletType::OverlayApplet
    OverlayApplet(OverlayAppletProxy),
    /// AppletType::SystemApplication
    SystemApplication(SystemApplicationProxy),
}

macro_rules! applet_proxy_match_to_fn {
    ($self:ident, $func:ident) => {
        match $self {
            AppletProxy::Application(p) => IApplicationProxyClient::$func(p),
            AppletProxy::SystemApplet(p) => ISystemAppletProxyClient::$func(p),
            AppletProxy::LibraryApplet(p) => ILibraryAppletProxyClient::$func(p),
            AppletProxy::OverlayApplet(p) => IOverlayAppletProxyClient::$func(p),
            AppletProxy::SystemApplication(p) => ISystemApplicationProxyClient::$func(p),
        }
    };
}
impl ProxyCommon for AppletProxy {
    fn get_common_state_getter(&self) -> Result<CommonStateGetter> {
        applet_proxy_match_to_fn!(self, get_common_state_getter)
    }

    fn get_self_controller(&self) -> Result<SelfController> {
        applet_proxy_match_to_fn!(self, get_self_controller)
    }

    fn get_window_controller(&self) -> Result<WindowController> {
        applet_proxy_match_to_fn!(self, get_window_controller)
    }

    fn get_audio_controller(&self) -> Result<AudioController> {
        applet_proxy_match_to_fn!(self, get_audio_controller)
    }

    fn get_display_controller(&self) -> Result<DisplayController> {
        applet_proxy_match_to_fn!(self, get_display_controller)
    }

    fn get_process_winding_controller(&self) -> Result<ProcessWindingController> {
        applet_proxy_match_to_fn!(self, get_process_winding_controller)
    }

    fn get_library_applet_creator(&self) -> Result<LibraryAppletCreator> {
        applet_proxy_match_to_fn!(self, get_library_applet_creator)
    }
}

/// global AppletAttribute used for openning the applet proxy for the program
///
/// TODO - make a better way to override this value
#[linkage = "weak"]
#[unsafe(export_name = "__nx_applet_attribute")]
pub static APPLET_ATTRIBUTE: AppletAttribute = AppletAttribute::zero();

/// Attempts to initialize the module, or returns if the module has already been initialized.
#[inline]
pub fn initialize() -> Result<()> {
    let mut app_proxy_service_guard = ALL_SYSTEM_APPLET_PROXY_SERVICE.write();
    if app_proxy_service_guard.is_some() {
        //already initialized
        return Ok(());
    }

    let app_proxy_service = service::new_service_object::<AllSystemAppletProxiesService>()?;

    let app_proxy = loop {
        let proxy_result: Result<AppletProxy> = try {
            match get_applet_type() {
                AppletType::Application | AppletType::Default => {
                    AppletProxy::Application(app_proxy_service.open_application_proxy(
                        sf::ProcessId::new(),
                        sf::CopyHandle::from(svc::CURRENT_PROCESS_PSEUDO_HANDLE),
                    )?)
                }
                AppletType::OverlayApplet => {
                    AppletProxy::OverlayApplet(app_proxy_service.open_overlay_applet_proxy(
                        sf::ProcessId::new(),
                        sf::CopyHandle::from(svc::CURRENT_PROCESS_PSEUDO_HANDLE),
                    )?)
                }
                AppletType::SystemApplet => {
                    AppletProxy::SystemApplet(app_proxy_service.open_system_applet_proxy(
                        sf::ProcessId::new(),
                        sf::CopyHandle::from(svc::CURRENT_PROCESS_PSEUDO_HANDLE),
                    )?)
                }
                AppletType::LibraryApplet if get_version() >= Version::new(3, 0, 0) => {
                    AppletProxy::LibraryApplet(app_proxy_service.open_library_applet_proxy(
                        sf::ProcessId::new(),
                        sf::CopyHandle::from(svc::CURRENT_PROCESS_PSEUDO_HANDLE),
                        sf::InMapAliasBuffer::from_var(&APPLET_ATTRIBUTE),
                    )?)
                }
                AppletType::LibraryApplet => {
                    AppletProxy::LibraryApplet(app_proxy_service.open_library_applet_proxy_old(
                        sf::ProcessId::new(),
                        sf::CopyHandle::from(svc::CURRENT_PROCESS_PSEUDO_HANDLE),
                    )?)
                }
                AppletType::SystemApplication => AppletProxy::SystemApplication(
                    app_proxy_service.open_system_application_proxy(
                        sf::ProcessId::new(),
                        sf::CopyHandle::from(svc::CURRENT_PROCESS_PSEUDO_HANDLE),
                    )?,
                ),
                AppletType::None => {
                    panic!(
                        "Initialized applet service with applet type disabled (`None` applet type)."
                    )
                }
            }
        };

        match proxy_result {
            Ok(p) => break Ok(p),
            Err(rc) if rc.get_value() == 0x19280 => {
                // behaviour from libnx, though we don't check for a global timeout
                let _ = svc::sleep_thread(100000000);
                continue;
            }
            Err(rc) => break Err(rc),
        }
    }?;

    let window_controller = app_proxy.get_window_controller()?;

    let aruid = window_controller.get_applet_resource_user_id()?;

    *app_proxy_service_guard = Some(app_proxy_service);
    *LIBRARY_APPLET_PROXY.write() = Some(app_proxy);
    *WINDOW_CONTROLLER.write() = Some(window_controller);
    GLOBAL_ARUID.store(aruid, core::sync::atomic::Ordering::Release);

    Ok(())
}

/// Returns whether the module has been successfully initialized.
pub fn is_initialized() -> bool {
    ALL_SYSTEM_APPLET_PROXY_SERVICE.read().is_some()
}

/// Finalizes library applet support, dropping the shared resources. pub(crate) as it should only run in rrt0.rs
pub(crate) fn finalize() {
    let mut app_proxy_service_guard = ALL_SYSTEM_APPLET_PROXY_SERVICE.write();

    *WINDOW_CONTROLLER.write() = None;
    *LIBRARY_APPLET_PROXY.write() = None;
    *app_proxy_service_guard = None;
}

/// Gets the registered global Window Controller
pub fn get_window_controller<'a>() -> ReadGuard<'a, Option<WindowController>> {
    WINDOW_CONTROLLER.read()
}

/// Gets the registered global AppletProxy
pub fn get_applet_proxy<'a>() -> ReadGuard<'a, Option<AppletProxy>> {
    LIBRARY_APPLET_PROXY.read()
}
/// Gets the registered global System Proxy Service
pub fn get_system_proxy_service<'a>() -> ReadGuard<'a, Option<AllSystemAppletProxiesService>> {
    ALL_SYSTEM_APPLET_PROXY_SERVICE.read()
}
