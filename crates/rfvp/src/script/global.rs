use std::{collections::HashMap, sync::Mutex};

use crate::script::Variant;
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

pub fn non_volatile_count(&self) -> u16 {
    self.none_volatile_count
}

pub fn volatile_count(&self) -> u16 {
    self.volatile_count
}

pub fn snapshot_non_volatile(&self) -> Vec<Variant> {
    let mut out: Vec<Variant> = Vec::with_capacity(self.none_volatile_count as usize);
    for i in 0..self.none_volatile_count {
        out.push(self.global_table.get(&i).cloned().unwrap_or(Variant::Nil));
    }
    out
}

pub fn restore_non_volatile(&mut self, vals: &[Variant]) {
    let n = self.none_volatile_count as usize;
    let take = vals.len().min(n);
    for i in 0..take {
        self.global_table.insert(i as u16, vals[i].clone());
    }
    // Missing entries remain unchanged.
}

pub fn snapshot_volatile_globals(&self) -> Vec<Variant> {
    let mut out: Vec<Variant> = Vec::with_capacity(self.volatile_count as usize);
    let base = self.none_volatile_count;
    for i in 0..self.volatile_count {
        let key = base.saturating_add(i);
        out.push(self.global_table.get(&key).cloned().unwrap_or(Variant::Nil));
    }
    out
}

pub fn restore_volatile_globals(
    &mut self,
    expected_non_volatile: u16,
    expected_volatile: u16,
    vars: &[Variant],
) {
    let base = self.none_volatile_count;

    if expected_non_volatile != self.none_volatile_count || expected_volatile != self.volatile_count {
        log::warn!(
            "GlobalSaveData: global counts mismatch: saved non_volatile={} volatile={} but current non_volatile={} volatile={}",
            expected_non_volatile,
            expected_volatile,
            self.none_volatile_count,
            self.volatile_count
        );
    }

    let n = vars.len().min(self.volatile_count as usize);
    for i in 0..n {
        let key = base.saturating_add(i as u16);
        self.global_table.insert(key, vars[i].clone());
    }
}
}


lazy_static::lazy_static! {
    pub static ref GLOBAL: Mutex<Global> =  Mutex::new(Global::new());
}


pub fn get_int_var(key: u16) -> i32 {
    GLOBAL.lock().unwrap().get_int_var(key)
}

