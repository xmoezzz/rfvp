//! Base service/named port support and wrappers

use core::ffi::CStr;

use sm::IUserInterfaceClient;

use crate::ipc::client;
use crate::ipc::sf;
use crate::result::*;
use crate::svc;

pub mod sm;

/// Represents a named port interface
///
/// Interfaces which wrap named ports (see [`manage_named_port`][`svc::manage_named_port`] or [`connect_to_named_port`][`svc::connect_to_named_port`]) must implement this trait
pub trait INamedPort: client::IClientObject {
    /// Gets the name to be used to connect to the named port (via [`connect_to_named_port`][`svc::connect_to_named_port`])
    fn get_name() -> &'static CStr;
    /// This will get executed after connecting to the named port in [`new_named_port_object`], allowing for extra initialization
    ///
    /// Some interfaces may have initialization commands (check [SM's case][`sm::UserInterface::register_client`]) which can be automatically called this way
    fn post_initialize(&mut self) -> Result<()>;
}

/// Represents a service interface
///
/// Interfaces which wrap services (see [SM][`sm::UserInterface`]) must implement this trait
pub trait IService: client::IClientObject {
    /// Gets the service's name
    fn get_name() -> sm::ServiceName;
    /// Gets whether the service should be used as a domain
    ///
    /// If this is [`true`], the service will be converted to a domain after being accessed (see [`convert_to_domain`][`sf::Session::convert_to_domain`]) in [`new_service_object`]
    fn as_domain() -> bool;
    /// This will get executed after accessing the service in [`new_service_object`], allowing for extra initialization
    ///
    /// Some interfaces may have initialization commands (check [SM's case][`sm::UserInterface::register_client`]) which can be automatically called this way
    fn post_initialize(&mut self) -> Result<()>;
}

/// Wrapper for connecting to a named port and instantiating the wrapper interface over the specified named port
///
/// For more information about this, check [`INamedPort`]
pub fn new_named_port_object<T: INamedPort + 'static>() -> Result<T> {
    let handle = unsafe { svc::connect_to_named_port(T::get_name()) }?;
    let mut object = T::new(sf::Session::from_handle(handle));
    object.post_initialize()?;
    Ok(object)
}

/// Wrapper for accessing a service and instantiating the wrapper interface over the specified service
///
/// For more information about this, check [`IService`]
pub fn new_service_object<T: IService>() -> Result<T> {
    let sm = new_named_port_object::<sm::UserInterface>()?;
    let session_handle = sm.get_service_handle(T::get_name())?;
    // rfvp patch: no `sm.detach_client` here. Ryujinx does not implement that command and
    // stops the emulator when it is called; on hardware it changes nothing, as `sm` goes
    // away (and its client registration with it) when this function returns.
    let mut object = T::new(sf::Session::from_handle(session_handle.handle));
    if T::as_domain() {
        object.convert_to_domain()?;
    }
    object.post_initialize()?;
    Ok(object)
}

/// "psm" service definitions.
pub mod psm;

/// "fsp-srv" service definitions.
pub mod fsp;

/// "lm" service definitions.
pub mod lm;

/// "vi:*" service definitions.
pub mod vi;

/// "nvdrv" and "nvdrv:*" service definitions.
pub mod nv;

/// "dispdrv" service definitions.
pub mod dispdrv;

/// "fatal:u" service definitions.
pub mod fatal;

/// "hid" service definitions.
pub mod hid;

/// "appletAE" service definitions.
pub mod applet;

/// "psc:m" service definitions.
pub mod psc;

/// "pm:*" service definitions.
pub mod pm;

/// "set:sys" service definitions.
pub mod set;

/// "mii:e" service definitions.
pub mod mii;

/// "csrng" service definitions.
pub mod spl;

/// "usb:hs" service definitions.
pub mod usb;

/// "ldr:shel" service definitions.
pub mod ldr;

/// "nfp:*" service definitions.
pub mod nfp;

/// "ncm" service definitions.
pub mod ncm;

/// "lr" service definitions.
pub mod lr;

/// "bsd" socket service definitions
pub mod bsd;

/// "aud*" auudio service definitions
pub mod audio;
