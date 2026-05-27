#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use serde::{Deserialize, Serialize};

use crate::utils::stable_hash::StableHashMap;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct FlagManager {
    flags: StableHashMap<u8, u8>,
}

impl FlagManager {
    pub fn set_flag(&mut self, id: u8, pos: u8, on: bool) {
        let mut flag = if let Some(flag) = self.flags.get(&id) {
            *flag
        } else {
            0u8
        };

        if on {
            flag |= 1 << pos;
        } else {
            flag &= !(1 << pos)
        }

        self.flags.insert(id, flag);
    }

    pub fn get_flag(&mut self, id: u8, pos: u8) -> bool {
        if let Some(flag) = self.flags.get(&id) {
            let flag = *flag;
            if flag & (1 << pos) != 0 {
                return true;
            }
        }

        false
    }
}
