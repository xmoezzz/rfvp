use crate::ipc::sf;

#[nx_derive::ipc_trait]
pub trait Random {
    #[ipc_rid(0)]
    fn generate_random_bytes(&self, out_buf: sf::OutMapAliasBuffer<'_, u8>);
}
