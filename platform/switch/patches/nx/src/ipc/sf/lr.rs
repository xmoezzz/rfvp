use super::ncm;
use crate::ipc::sf;
use crate::version;

#[nx_derive::ipc_trait]
#[default_client]
pub trait LocationResolver {
    #[ipc_rid(1)]
    fn redirect_program_path(
        &self,
        program_id: ncm::ProgramId,
        path_buf: sf::InPointerBuffer<'_, u8>,
    );
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait RegisteredLocationResolver {
    #[ipc_rid(1)]
    #[version(version::VersionInterval::to(version::Version::new(8, 1, 1)))]
    fn register_program_path_deprecated(
        &self,
        program_id: ncm::ProgramId,
        path_buf: sf::InPointerBuffer<'_, u8>,
    );
    #[ipc_rid(1)]
    #[version(version::VersionInterval::from(version::Version::new(9, 0, 0)))]
    fn register_program_path(
        &self,
        program_id: ncm::ProgramId,
        owner_id: ncm::ProgramId,
        path_buf: sf::InPointerBuffer<'_, u8>,
    );
    #[ipc_rid(3)]
    #[version(version::VersionInterval::to(version::Version::new(8, 1, 1)))]
    fn redirect_program_path_deprecated(
        &self,
        program_id: ncm::ProgramId,
        path_buf: sf::InPointerBuffer<'_, u8>,
    );
    #[ipc_rid(3)]
    #[version(version::VersionInterval::from(version::Version::new(9, 0, 0)))]
    fn redirect_program_path(
        &self,
        program_id: ncm::ProgramId,
        owner_id: ncm::ProgramId,
        path_buf: sf::InPointerBuffer<'_, u8>,
    );
}

#[nx_derive::ipc_trait]
pub trait LocationResolverManager {
    #[ipc_rid(0)]
    #[return_session]
    fn open_location_resolver(&self, storage_id: ncm::StorageId) -> LocationResolver;
    #[ipc_rid(1)]
    #[return_session]
    fn open_registered_location_resolver(&self) -> RegisteredLocationResolver;
}
