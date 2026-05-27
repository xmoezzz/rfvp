use crate::app::AppBuilder;
use crate::subsystem::world::GameData;
#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

pub trait Package {
    fn prepare(&self, _data: &mut GameData) {}

    fn load(self, builder: AppBuilder) -> AppBuilder;
}
