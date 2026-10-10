use crate::ipc::sf;
use crate::result::*;
use crate::svc::Handle;
use crate::version;

pub use super::AppletResourceUserId;

use nx_derive::{Request, Response};

#[derive(Request, Response, Copy, Clone)]
#[repr(C)]
pub struct AppletAttribute {
    flag: u8,
    reserved: [u8; 0x7F],
}

impl AppletAttribute {
    pub const fn zero() -> Self {
        Self {
            flag: 0,
            reserved: [0u8; 0x7F],
        }
    }
}

impl Default for AppletAttribute {
    fn default() -> Self {
        Self::zero()
    }
}

const_assert!(core::mem::size_of::<AppletAttribute>() == 0x80);

#[derive(Request, Response, Copy, Clone)]
#[repr(C)]
pub struct AppletProcessLaunchReason {
    flag: u8,
    _zero: [u8; 2],
    _zero2: u8,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ScreenShotPermission {
    Inherit,
    Enable,
    Disable,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum AppletId {
    None = 0x00,
    Application = 0x01,
    OverlayApplet = 0x02,
    SystemAppletMenu = 0x03,
    SystemApplication = 0x04,
    LibraryAppletAuth = 0x0A,
    LibraryAppletCabinet = 0x0B,
    LibraryAppletController = 0x0C,
    LibraryAppletDataErase = 0x0D,
    LibraryAppletError = 0x0E,
    LibraryAppletNetConnect = 0x0F,
    LibraryAppletPlayerSelect = 0x10,
    LibraryAppletSwkbd = 0x11,
    LibraryAppletMiiEdit = 0x12,
    LibAppletWeb = 0x13,
    LibAppletShop = 0x14,
    LibraryAppletPhotoViewer = 0x15,
    LibraryAppletSet = 0x16,
    LibraryAppletOfflineWeb = 0x17,
    LibraryAppletLoginShare = 0x18,
    LibraryAppletWifiWebAuth = 0x19,
    LibraryAppletMyPage = 0x1A,
    LibraryAppletGift = 0x1B,
    LibraryAppletUserMigration = 0x1C,
    LibraryAppletPreomiaSys = 0x1D,
    LibraryAppletStory = 0x1E,
    LibraryAppletPreomiaUsr = 0x1F,
    LibraryAppletPreomiaUsrDummy = 0x20,
    LibraryAppletSample = 0x21,
    DevlopmentTool = 0x3E8,
    CombinationLA = 0x3F1,
    AeSystemApplet = 0x3F2,
    AeOverlayApplet = 0x3F3,
    AeStarter = 0x3F4,
    AeLibraryAppletAlone = 0x3F5,
    AeLibraryApplet1 = 0x3F6,
    AeLibraryApplet2 = 0x3F7,
    AeLibraryApplet3 = 0x3F8,
    AeLibraryApplet4 = 0x3F9,
    AppletISA = 0x3FA,
    AppletIOA = 0x3FB,
    AppletISTA = 0x3FC,
    AppletILA1 = 0x3FD,
    AppletILA2 = 0x3FE,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub enum OperationMode {
    Handheld = 0,
    Console = 1,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub enum PerformanceMode {
    Invalid = -1,
    Normal = 0,
    Boost = 1,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub enum LibraryAppletMode {
    AllForeground,
    Background,
    NoUi,
    BackgroundIndirectDisplay,
    AllForegroundInitiallyHidden,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub enum AppletMessage {
    /// The applet received an exit request
    ExitRequest = 4,
    /// The FocusState of the applet changed
    FocusStateChanged = 15,
    /// The applet was resumed
    Resume = 16,
    /// The OperationMode of the applet changed
    OperationModeChanged = 30,
    /// The PerformanceMode of the applet changed
    PerformanceMode = 31,
    /// The applet was requested to display
    DisplayRequested = 51,
    /// Capture button was pressed (short)
    CaptureButtonPressedShort = 90,
    /// A screenshot was taken
    ScreenShotTaken = 92,
    /// A screen recoding was saved
    RecordingSaved = 93,
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait StorageAccessor {
    #[ipc_rid(0)]
    fn get_size(&self) -> usize;
    #[ipc_rid(10)]
    fn write(&self, offset: usize, buf: sf::InAutoSelectBuffer<'_, u8>);
    #[ipc_rid(11)]
    fn read(&self, offset: usize, buf: sf::OutAutoSelectBuffer<'_, u8>);
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait Storage {
    #[ipc_rid(0)]
    fn open(&self) -> StorageAccessor;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait LibraryAppletAccessor {
    #[ipc_rid(0)]
    fn get_applet_state_changed_event(&mut self) -> sf::CopyHandle;
    #[ipc_rid(10)]
    fn start(&mut self);
    #[ipc_rid(100)]
    fn push_in_data(&mut self, storage: Storage);
    #[ipc_rid(101)]
    fn pop_out_data(&mut self) -> Storage;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait LibraryAppletCreator {
    #[ipc_rid(0)]
    #[return_session]
    fn create_library_applet(
        &self,
        applet_id: AppletId,
        applet_mode: LibraryAppletMode,
    ) -> LibraryAppletAccessor;
    #[ipc_rid(10)]
    fn create_storage(&self, size: usize) -> Storage;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait WindowController {
    #[ipc_rid(1)]
    fn get_applet_resource_user_id(&self) -> u64;
    #[ipc_rid(10)]
    fn acquire_foreground_rights(&self);
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait SelfController {
    #[ipc_rid(10)]
    fn set_screenshot_permission(&self, permission: ScreenShotPermission);
    #[ipc_rid(40)]
    fn create_managed_display_layer(&self) -> u64;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait AudioController {
    #[ipc_rid(0)]
    fn set_expected_master_volume(&self, main_applet_level: f32, library_applet_level: f32);
    #[ipc_rid(1)]
    fn get_main_applet_volume(&self) -> f32;
    #[ipc_rid(2)]
    fn get_library_applet_volume(&self) -> f32;
    #[ipc_rid(3)]
    fn change_main_applet_volume(&self, main_applet_level: f32, unknown: u64);
    #[ipc_rid(4)]
    fn set_transparent_volume_rate(&self, rate: f32);
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait DisplayController {
    #[ipc_rid(3)]
    fn update_caller_applet_capture_image(&self);
}
#[nx_derive::ipc_trait]
#[default_client]
pub trait ProcessWindingController {
    #[ipc_rid(0)]
    fn get_launch_reason(&self) -> AppletProcessLaunchReason;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait CommonStateGetter {
    #[ipc_rid(0)]
    fn get_event_handle(&self) -> Handle;
    #[ipc_rid(1)]
    fn receive_message(&self) -> AppletMessage;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait LibraryAppletProxy {
    #[ipc_rid(0)]
    #[return_session]
    fn get_common_state_getter(&self) -> CommonStateGetter;
    #[ipc_rid(1)]
    #[return_session]
    fn get_self_controller(&self) -> SelfController;
    #[ipc_rid(2)]
    #[return_session]
    fn get_window_controller(&self) -> WindowController;
    #[ipc_rid(3)]
    #[return_session]
    fn get_audio_controller(&self) -> AudioController;
    #[ipc_rid(4)]
    #[return_session]
    fn get_display_controller(&self) -> DisplayController;
    #[ipc_rid(10)]
    #[return_session]
    fn get_process_winding_controller(&self) -> ProcessWindingController;
    #[ipc_rid(11)]
    #[return_session]
    fn get_library_applet_creator(&self) -> LibraryAppletCreator;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait ApplicationProxy {
    #[ipc_rid(0)]
    #[return_session]
    fn get_common_state_getter(&self) -> CommonStateGetter;
    #[ipc_rid(1)]
    #[return_session]
    fn get_self_controller(&self) -> SelfController;
    #[ipc_rid(2)]
    #[return_session]
    fn get_window_controller(&self) -> WindowController;
    #[ipc_rid(3)]
    #[return_session]
    fn get_audio_controller(&self) -> AudioController;
    #[ipc_rid(4)]
    #[return_session]
    fn get_display_controller(&self) -> DisplayController;
    #[ipc_rid(10)]
    #[return_session]
    fn get_process_winding_controller(&self) -> ProcessWindingController;
    #[ipc_rid(11)]
    #[return_session]
    fn get_library_applet_creator(&self) -> LibraryAppletCreator;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait SystemAppletProxy {
    #[ipc_rid(0)]
    #[return_session]
    fn get_common_state_getter(&self) -> CommonStateGetter;
    #[ipc_rid(1)]
    #[return_session]
    fn get_self_controller(&self) -> SelfController;
    #[ipc_rid(2)]
    #[return_session]
    fn get_window_controller(&self) -> WindowController;
    #[ipc_rid(3)]
    #[return_session]
    fn get_audio_controller(&self) -> AudioController;
    #[ipc_rid(4)]
    #[return_session]
    fn get_display_controller(&self) -> DisplayController;
    #[ipc_rid(10)]
    #[return_session]
    fn get_process_winding_controller(&self) -> ProcessWindingController;
    #[ipc_rid(11)]
    #[return_session]
    fn get_library_applet_creator(&self) -> LibraryAppletCreator;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait OverlayAppletProxy {
    #[ipc_rid(0)]
    #[return_session]
    fn get_common_state_getter(&self) -> CommonStateGetter;
    #[ipc_rid(1)]
    #[return_session]
    fn get_self_controller(&self) -> SelfController;
    #[ipc_rid(2)]
    #[return_session]
    fn get_window_controller(&self) -> WindowController;
    #[ipc_rid(3)]
    #[return_session]
    fn get_audio_controller(&self) -> AudioController;
    #[ipc_rid(4)]
    #[return_session]
    fn get_display_controller(&self) -> DisplayController;
    #[ipc_rid(10)]
    #[return_session]
    fn get_process_winding_controller(&self) -> ProcessWindingController;
    #[ipc_rid(11)]
    #[return_session]
    fn get_library_applet_creator(&self) -> LibraryAppletCreator;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait SystemApplicationProxy {
    #[ipc_rid(0)]
    #[return_session]
    fn get_common_state_getter(&self) -> CommonStateGetter;
    #[ipc_rid(1)]
    #[return_session]
    fn get_self_controller(&self) -> SelfController;
    #[ipc_rid(2)]
    #[return_session]
    fn get_window_controller(&self) -> WindowController;
    #[ipc_rid(3)]
    #[return_session]
    fn get_audio_controller(&self) -> AudioController;
    #[ipc_rid(4)]
    #[return_session]
    fn get_display_controller(&self) -> DisplayController;
    #[ipc_rid(10)]
    #[return_session]
    fn get_process_winding_controller(&self) -> ProcessWindingController;
    #[ipc_rid(11)]
    #[return_session]
    fn get_library_applet_creator(&self) -> LibraryAppletCreator;
}

#[nx_derive::ipc_trait]
pub trait AllSystemAppletProxies {
    #[ipc_rid(0)]
    #[return_session]
    fn open_application_proxy(
        &self,
        process_id: sf::ProcessId,
        self_process_handle: sf::CopyHandle,
    ) -> ApplicationProxy;
    #[ipc_rid(100)]
    #[return_session]
    fn open_system_applet_proxy(
        &self,
        process_id: sf::ProcessId,
        self_process_handle: sf::CopyHandle,
    ) -> SystemAppletProxy;
    #[ipc_rid(200)]
    #[return_session]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn open_library_applet_proxy_old(
        &self,
        process_id: sf::ProcessId,
        self_process_handle: sf::CopyHandle,
    ) -> LibraryAppletProxy;
    #[ipc_rid(201)]
    #[return_session]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn open_library_applet_proxy(
        &self,
        process_id: sf::ProcessId,
        self_process_handle: sf::CopyHandle,
        applet_attribute: sf::InMapAliasBuffer<'_, AppletAttribute>,
    ) -> LibraryAppletProxy;
    #[ipc_rid(300)]
    #[return_session]
    fn open_overlay_applet_proxy(
        &self,
        process_id: sf::ProcessId,
        self_process_handle: sf::CopyHandle,
    ) -> OverlayAppletProxy;
    #[ipc_rid(350)]
    #[return_session]
    fn open_system_application_proxy(
        &self,
        process_id: sf::ProcessId,
        self_process_handle: sf::CopyHandle,
    ) -> SystemApplicationProxy;
}

pub trait ProxyCommon {
    fn get_common_state_getter(&self) -> Result<CommonStateGetter>;
    fn get_self_controller(&self) -> Result<SelfController>;
    fn get_window_controller(&self) -> Result<WindowController>;
    fn get_audio_controller(&self) -> Result<AudioController>;
    fn get_display_controller(&self) -> Result<DisplayController>;
    fn get_process_winding_controller(&self) -> Result<ProcessWindingController>;
    fn get_library_applet_creator(&self) -> Result<LibraryAppletCreator>;
}
