
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::io::Read;
use std::io::Write;

use crate::script::Variant;
use serde::{Serialize, Deserialize};
use anyhow::Result;

/// Global variables
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Global {
    global_table: HashMap<u16, Variant>,
    // tables: HashMap<u8, Table>,
    // cur_table_count: u32,
    // table_allocation: Vec<bool>,
}


impl Global {
    pub fn new() -> Self {
        // initialize 256 tables
        // let mut tables = HashMap::new();
        // for i in 0..256 {
        //     tables.insert(i, Table::new());
        // }

        Global {
            global_table: HashMap::new(),
            // tables,
            // cur_table_count: 0,
            // table_allocation: vec![false; 256],
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

    // pub fn alloc_table(&mut self) -> Result<(&mut Table, u32)> {
    //     if self.cur_table_count >= 256 {
    //         bail!("table count exceeded");
    //     }

    //     let table = self.tables.entry(self.cur_table_count as u8).or_insert(Table::new());
    //     self.table_allocation[self.cur_table_count as usize] = true;
    //     let value = self.cur_table_count;
    //     self.cur_table_count += 1;

    //     Ok((table, value))
    // }

    pub fn set(&mut self, key: u16, value: Variant) {
        self.global_table.insert(key, value);
    }
    
    pub fn init_with(&mut self, none_volatile: u16, volatile: u16) {
        for i in 0..none_volatile + volatile {
            self.global_table.insert(i, Variant::Nil);
        }
    }
}


