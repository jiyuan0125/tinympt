use std::path::PathBuf;

use crate::{HashValue, Result};
use rocksdb::DB;

use super::Database;

#[derive(Debug)]
pub struct RocksdbDatabase {
    db: DB,
}

impl RocksdbDatabase {
    pub fn new(db_path: PathBuf) -> Self {
        let db = DB::open_default(db_path).unwrap();
        Self { db }
    }
}

impl Database for RocksdbDatabase {
    fn get(&self, key: &HashValue) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get(key)?)
    }

    fn insert(&mut self, key: HashValue, value: Vec<u8>) -> Result<()> {
        Ok(self.db.put(key, value)?)
    }

    fn exists(&self, key: &HashValue) -> Result<bool> {
        Ok(self.db.key_may_exist(key))
    }
}
