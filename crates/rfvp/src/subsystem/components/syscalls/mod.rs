use crate::{script::Variant, subsystem::world::GameData};

pub mod color;
pub mod cursor;
pub mod flag;
pub mod generated;
pub mod graph;
pub mod history;
pub mod input;
pub mod legacy;
pub mod motion;
#[cfg(all(not(target_os = "uefi"), not(target_arch = "wasm32")))]
pub mod movie;
#[cfg(any(target_os = "uefi", target_arch = "wasm32"))]
#[path = "movie_wasm.rs"]
pub mod movie;
pub mod other_anm;
pub mod parts;
pub mod saveload;
pub mod sound;
pub mod text;
pub mod thread;
pub mod timer;
pub mod utils;

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
