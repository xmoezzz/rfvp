use crate::ipc::sf;

use super::FileSystem;

#[nx_derive::ipc_trait]
pub trait FileSystemProxy {
    #[ipc_rid(1)]
    fn set_current_process(&self, process_id: sf::ProcessId);
    #[ipc_rid(18)]
    fn open_sd_card_filesystem(&self) -> FileSystem;
    #[ipc_rid(1006)]
    fn output_access_log_to_sd_card(&self, log_buf: sf::InMapAliasBuffer<'_, u8>);
}
