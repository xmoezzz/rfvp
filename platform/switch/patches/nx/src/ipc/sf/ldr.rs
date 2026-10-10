use crate::ipc::sf;
use crate::version;

use super::ncm;

#[nx_derive::ipc_trait]
pub trait ShellInterface {
    #[ipc_rid(0)]
    #[version(version::VersionInterval::to(version::Version::new(10, 2, 0)))]
    fn set_program_argument_deprecated(
        &self,
        program_id: ncm::ProgramId,
        args_size: u32,
        args_buf: sf::InPointerBuffer<'_, u8>,
    );
    #[ipc_rid(0)]
    #[version(version::VersionInterval::from(version::Version::new(11, 0, 0)))]
    fn set_program_argument(
        &self,
        program_id: ncm::ProgramId,
        args_buf: sf::InPointerBuffer<'_, u8>,
    );
    #[ipc_rid(1)]
    fn flush_arguments(&self);
    #[ipc_rid(65000)]
    fn atmosphere_register_external_code(&self, program_id: ncm::ProgramId) -> sf::MoveHandle;
    #[ipc_rid(65001)]
    fn atmosphere_unregister_external_code(&self, program_id: ncm::ProgramId);
}
