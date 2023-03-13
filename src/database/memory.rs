use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{HashValue, Result};

use super::Database;

/// 内存数据库
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryDatabase {
    data: HashMap<HashValue, Vec<u8>>,
}

impl MemoryDatabase {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

/// 实现 Database trait
impl Database for MemoryDatabase {
    fn get(&self, key: &HashValue) -> Result<Option<Vec<u8>>> {
        Ok(self.data.get(key).cloned())
    }

    fn insert(&mut self, key: HashValue, value: Vec<u8>) -> Result<()> {
        self.data.insert(key, value);
        Ok(())
    }

    fn exists(&self, key: &HashValue) -> Result<bool> {
        Ok(self.data.contains_key(key))
    }
}
