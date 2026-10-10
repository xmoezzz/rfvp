use crate::version;

use super::ncm;

#[nx_derive::ipc_trait]
pub trait InformationInterface {
    #[ipc_rid(0)]
    fn get_program_id(&self, process_id: u64) -> ncm::ProgramId;
}

#[nx_derive::ipc_trait]
pub trait DebugMonitorInterface {
    #[ipc_rid(5)]
    #[version(version::VersionInterval::to(version::Version::new(4, 1, 0)))]
    fn get_application_process_id_deprecated(&self) -> u64;
    #[ipc_rid(4)]
    #[version(version::VersionInterval::from(version::Version::new(5, 0, 0)))]
    fn get_application_process_id(&self) -> u64;
    #[ipc_rid(7)]
    #[version(version::VersionInterval::from(version::Version::new(14, 0, 0)))]
    fn get_program_id(&self, raw_process_id: u64) -> ncm::ProgramId;
}
