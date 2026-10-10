use crate::ipc::sf;
use crate::version;

use nx_derive::{Request, Response};

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ModuleId {
    Lm = 0x29,
    // TODO: more
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum State {
    FullAwake = 0,
    MinimumAwake = 1,
    SleepReady = 2,
    EssentialServicesSleepReady = 3,
    EssentialServicesAwake = 4,
    ShutdownReady = 5,
    Invalid = 6,
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait PmModule {
    #[ipc_rid(0)]
    fn initialize(
        &self,
        id: ModuleId,
        dependencies: sf::InMapAliasBuffer<'_, ModuleId>,
    ) -> sf::CopyHandle;
    #[ipc_rid(1)]
    fn get_request(&self) -> (State, u32);
    #[ipc_rid(2)]
    fn acknowledge(&self);
    #[ipc_rid(3)]
    fn finalize(&self);
    #[ipc_rid(4)]
    #[version(version::VersionInterval::from(version::Version::new(5, 1, 0)))]
    fn acknowledge_ex(&self, state: State);
}

#[nx_derive::ipc_trait]
pub trait Pm {
    #[ipc_rid(0)]
    #[return_session]
    fn get_pm_module(&self) -> PmModule;
}
