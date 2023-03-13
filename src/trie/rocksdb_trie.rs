use serde::{de::DeserializeOwned, Serialize};
use std::{marker::PhantomData, path::PathBuf};

use super::{node::TrieNodeLink, Trie};
use crate::database::RocksdbDatabase;

/// Rocksdb Trie
pub struct RocksdbTrie<K, V> {
    root_node: TrieNodeLink,
    db: RocksdbDatabase,
    dirty: bool,
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
