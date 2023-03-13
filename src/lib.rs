mod database;
mod error;
#[cfg(feature = "network")]
mod network;
mod trie;

pub use error::TrieError;
pub type Result<T> = std::result::Result<T, TrieError>;
/// 为 hash 值定义一个类型
pub type HashValue = [u8; 32];
/// 为 nibble slice 定义一个类型
pub type NibbleSlice = [u8];
/// 为 nibble vec 定义一个类型
pub type NibbleVec = Vec<u8>;

#[cfg(feature = "network")]
pub use network::{ProofRequest, ProofResponse};

#[cfg(feature = "rocksdb")]
pub use database::RocksdbDatabase;
pub use database::{Database, MemoryDatabase};
#[cfg(feature = "rocksdb")]
pub use trie::rocksdb_trie::RocksdbTrie;
pub use trie::verify_proof;
pub use trie::{memory_trie::MemoryTrie, Trie};
