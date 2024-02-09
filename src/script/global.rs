
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::io::Read;
use std::io::Write;

use crate::script::Variant;
use serde::{Serialize, Deserialize};
use anyhow::Result;

/// Global variables
#[derive(Debug, Serialize, Deserialize)]
pub struct Global {
    global_table: HashMap<u16, Variant>,
}

impl Global {
    pub fn new() -> Self {
        Global {
            global_table: HashMap::new(),
        }
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let mut rdr = File::open(path)?;
        let mut buffer = Vec::new();
        rdr.read_to_end(&mut buffer)?;

        let global: Global = bincode::deserialize(&buffer)?;
        Ok(global)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let buffer = bincode::serialize(&self)?;
        let mut wtr = File::create(path)?;
        wtr.write_all(&buffer)?;
        Ok(())
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
}


