use serde::{de::DeserializeOwned, Serialize};
use std::{marker::PhantomData, path::PathBuf};

use super::{node::TrieNodeLink, Trie};
use crate::database::RocksdbDatabase;

/// Rocksdb Trie
pub struct RocksdbTrie<K, V> {
    root_node: TrieNodeLink,
    db: RocksdbDatabase,
    dirty: bool,
    // K, V 是 Trie trait 的方法里使用的, RocksdbTrie 里没有使用
    // 使用 PhantomData 来避免编译器报错
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

impl<K, V> RocksdbTrie<K, V> {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            root_node: TrieNodeLink::Empty,
            db: RocksdbDatabase::new(db_path),
            dirty: false,
            _k: PhantomData,
            _v: PhantomData,
        }
    }
}

// 通常只需要在实现时才约束泛型，定义结构体的时候不需要
// 这样保持结构体的灵活性，同时我们也可以针对不同的约束给出不同的实现
// 当然此处为了实现 Trie trait，我们必须要约束 K, V, 
// 所以这里的约束是必须的
impl<K, V> Trie<K, V> for RocksdbTrie<K, V>
where
    K: AsRef<[u8]>,
    V: Serialize + DeserializeOwned,
{
    type Database = RocksdbDatabase;

    fn dirty(&self) -> bool {
        self.dirty
    }

    fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    fn root_node(&self) -> &TrieNodeLink {
        &self.root_node
    }

    fn take_root_node(&mut self) -> TrieNodeLink {
        std::mem::take(&mut self.root_node)
    }

    fn set_root_node(&mut self, node: TrieNodeLink) {
        self.root_node = node;
    }

    fn db_ref(&self) -> &Self::Database {
        &self.db
    }

    fn db_mut(&mut self) -> &mut Self::Database {
        &mut self.db
    }
}
