mod memory;
#[cfg(feature = "rocksdb")]
mod rocksdb;

#[cfg(feature = "rocksdb")]
pub use crate::database::rocksdb::RocksdbDatabase;
pub use memory::MemoryDatabase;

use crate::{HashValue, Result};

/// Database trait
pub trait Database {
    /// 从数据里获得指定 key 的值
    fn get(&self, key: &HashValue) -> Result<Option<Vec<u8>>>;

    /// 插入 key-value 到数据库
    fn insert(&mut self, key: HashValue, value: Vec<u8>) -> Result<()>;

    /// 检查数据库里是否存在指定的 key
    fn exists(&self, key: &HashValue) -> Result<bool>;
}
