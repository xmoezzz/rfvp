use crate::ipc::sf;
use crate::util;

use sf::dispdrv::{HOSBinderDriver, IHOSBinderDriverServer};

use sf::applet::AppletResourceUserId;

use nx_derive::{Request, Response};

pub type DisplayName = util::ArrayString<0x40>;

define_bit_set! {
    LayerFlags (u32) {
        None = 0,
        Default = bit!(0)
    }
}

/// Tells the display service how to scale spawned layers.
#[derive(Request, Response, Copy, Clone, Debug, Default)]
#[repr(u64)]
pub enum ScalingMode {
    None = 0,
    #[default]
    FitToLayer = 2,
    PreserveAspectRatio = 4,
}

pub type DisplayId = u64;

pub type LayerId = u64;

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum DisplayServiceMode {
    User = 0,
    Privileged = 1,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
pub enum LayerStackId {
    #[default]
    Default,
    Lcd,
    Screenshot,
    Recording,
    LastFrame,
    Arbitrary,
    ApplicationForDebug,
    Null,
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait ManagerDisplay {
    #[ipc_rid(2010)]
    fn create_managed_layer(
        &mut self,
        flags: LayerFlags,
        display_id: DisplayId,
        raw_aruid: u64,
    ) -> LayerId;
    #[ipc_rid(2011)]
    fn destroy_managed_layer(&self, id: LayerId);
    #[ipc_rid(6000)]
    fn add_to_layer_stack(&self, stack: LayerStackId, layer: LayerId);
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait SystemDisplay {
    #[ipc_rid(1200)]
    fn get_z_order_count_min(&self, display_id: DisplayId) -> i64;
    #[ipc_rid(1202)]
    fn get_z_order_count_max(&self, display_id: DisplayId) -> i64;
    #[ipc_rid(2201)]
    fn set_layer_position(&mut self, x: f32, y: f32, id: LayerId);
    #[ipc_rid(2203)]
    fn set_layer_size(&mut self, id: LayerId, width: u64, height: u64);
    #[ipc_rid(2205)]
    fn set_layer_z(&mut self, id: LayerId, z: i64);
    #[ipc_rid(2207)]
    fn set_layer_visibility(&mut self, visible: bool, id: LayerId);
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait ApplicationDisplay {
    #[ipc_rid(100)]
    #[return_session]
    fn get_relay_service(&self) -> HOSBinderDriver;
    #[ipc_rid(101)]
    #[return_session]
    fn get_system_display_service(&self) -> SystemDisplay;
    #[ipc_rid(102)]
    #[return_session]
    fn get_manager_display_service(&self) -> ManagerDisplay;
    #[ipc_rid(1010)]
    fn open_display(&mut self, name: DisplayName) -> DisplayId;
    #[ipc_rid(1020)]
    fn close_display(&mut self, id: DisplayId);
    #[ipc_rid(2020)]
    fn open_layer(
        &mut self,
        name: DisplayName,
        id: LayerId,
        aruid: AppletResourceUserId,
        out_native_window: sf::OutMapAliasBuffer<'_, u8>,
    ) -> usize;
    #[ipc_rid(2021)]
    fn destroy_managed_layer(&mut self, id: LayerId);
    #[ipc_rid(2030)]
    fn create_stray_layer(
        &mut self,
        flags: LayerFlags,
        display_id: DisplayId,
        out_native_window: sf::OutMapAliasBuffer<'_, u8>,
    ) -> (LayerId, usize);
    #[ipc_rid(2031)]
    fn destroy_stray_layer(&mut self, id: LayerId);
    #[ipc_rid(2101)]
    fn set_scaling_mode(&mut self, scaling_mode: ScalingMode, layer_id: LayerId);
    #[ipc_rid(5202)]
    fn get_display_vsync_event(&self, id: DisplayId) -> sf::CopyHandle;
}

#[nx_derive::ipc_trait]
pub trait ApplicationDisplayRoot {
    #[ipc_rid(0)]
    #[return_session]
    fn get_display_service(&self, mode: DisplayServiceMode) -> ApplicationDisplay;
}

#[nx_derive::ipc_trait]
pub trait SystemDisplayRoot {
    #[ipc_rid(1)]
    #[return_session]
    fn get_display_service(&self, mode: DisplayServiceMode) -> ApplicationDisplay;
}

#[nx_derive::ipc_trait]
pub trait ManagerDisplayRoot {
    #[ipc_rid(2)]
    #[return_session]
    fn get_display_service(&self, mode: DisplayServiceMode) -> ApplicationDisplay;
}
