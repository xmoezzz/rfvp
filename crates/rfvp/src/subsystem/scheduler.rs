use crate::subsystem::world::GameData;
#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use std::collections::LinkedList;

#[derive(Default)]
pub(crate) struct Scheduler {
    systems: LinkedList<fn(&mut GameData)>,
}

impl Scheduler {
    pub(crate) fn add_system(&mut self, system: fn(&mut GameData)) {
        self.systems.push_back(system);
    }
    pub(crate) fn execute(&mut self, data: &mut GameData) {
        self.systems.iter().for_each(|s| s(data))
    }
}
