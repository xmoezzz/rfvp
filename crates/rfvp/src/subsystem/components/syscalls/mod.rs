use crate::{script::Variant, subsystem::world::GameData};
#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

pub mod color;
pub mod cursor;
pub mod flag;
pub mod generated;
pub mod graph;
pub mod history;
pub mod input;
pub mod legacy;
pub mod motion;
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
    fn call(&self, game_data: &mut GameData, args: &[Variant]) -> anyhow::Result<Variant>;
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
