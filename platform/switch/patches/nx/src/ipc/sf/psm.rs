#[nx_derive::ipc_trait]
pub trait Psm {
    #[ipc_rid(0)]
    fn get_battery_charge_percentage(&self) -> u32;
}
