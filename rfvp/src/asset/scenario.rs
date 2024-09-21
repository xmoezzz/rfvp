use rfvp_core::format::scenario::Scenario;

use crate::asset::Asset;

impl Asset for Scenario {
    fn load_from_bytes(data: Vec<u8>) -> anyhow::Result<Self> {
        Scenario::new(data.into())
    }
}
