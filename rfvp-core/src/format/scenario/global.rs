use std::{collections::HashMap, sync::Mutex};

use crate::format::scenario::variant::Variant;
use serde::{Serialize, Deserialize};

/// Global variables
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Global {
    global_table: HashMap<u16, Variant>,
    none_volatile_count: u16, 
    volatile_count: u16
}


impl Global {
    pub fn new() -> Self {

        Global {
            global_table: HashMap::new(),
            none_volatile_count: 0,
            volatile_count: 0
        }
    }

    pub fn get(&self, key: u16) -> Option<&Variant> {
        self.global_table.get(&key)
    }

    pub fn get_mut(&mut self, key: u16) -> Option<&mut Variant> {
        self.global_table.get_mut(&key)
    }

    pub fn set(&mut self, key: u16, value: Variant) {
        self.global_table.insert(key, value);
    }
    
    pub fn init_with(&mut self, none_volatile: u16, volatile: u16) {

        self.none_volatile_count = none_volatile;
        self.volatile_count = volatile;

        for i in 0..none_volatile + volatile {
            self.global_table.insert(i, Variant::Nil);
        }
    }

    pub fn get_int_var(&self, key: u16) -> i32 {
        let key = key + self.none_volatile_count;
        if let Some(Variant::Int(val)) = self.global_table.get(&key) {
            return *val;
        }
        0
    }
}

lazy_static::lazy_static! {
    pub static ref GLOBAL: Mutex<Global> =  Mutex::new(Global::new());
}


pub fn get_int_var(key: u16) -> i32 {
    GLOBAL.lock().unwrap().get_int_var(key)
}

