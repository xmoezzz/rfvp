use crate::{script::Variant, subsystem::world::GameData};

pub mod sound;
pub mod utils;
pub mod thread;
pub mod graph;
pub mod history;
pub mod flag;
pub mod motion;

pub trait Syscaller {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> anyhow::Result<Variant>;
}

macro_rules! get_var {
    ($args:expr, $i:expr) => {
        if let Some(variant) = $args.get($i) {
            variant
        } else {
            &Variant::Nil
        }
    };
}

pub(crate) use get_var;
