use crate::{
    database::{Database, MemoryDatabase},
    trie::node::{TrieNode, TrieNodeLink},
    HashValue, Result, NibbleVec,
};
use serde::{de::DeserializeOwned, Serialize};

pub mod memory_trie;
mod node;
mod util;

#[cfg(feature = "rocksdb")]
pub mod rocksdb_trie;

/// Trie trait
pub trait Trie<K, V>
where
    K: AsRef<[u8]>,
    V: Serialize + DeserializeOwned,
{
    /// The database type
    type Database: Database;

    /// If the trie is dirty, it means that the data has not been committed
    fn dirty(&self) -> bool;

    /// Set the dirty flag
    fn set_dirty(&mut self, dirty: bool);

    /// Get the root of the trie
    fn root_node(&self) -> &TrieNodeLink;

    /// Used to move the root node out of the trie
    fn take_root_node(&mut self) -> TrieNodeLink;

    /// Set the root of the trie
    fn set_root_node(&mut self, node: TrieNodeLink);

    /// Get the database mut reference
    fn db_mut(&mut self) -> &mut Self::Database;

    /// Get the database mut reference
    fn db_ref(&self) -> &Self::Database;

    /// Insert a key-value into the trie
    fn insert(&mut self, key: K, value: V) -> Result<()> {
        // Convert the key to nibble
        let key_nb: NibbleVec = util::convert_bytes_to_nibbles(key.as_ref());
        // Take the root node out of the trie
        let root_node = self.take_root_node();
        // Serialize the value
        let bin_node = bincode::serialize(&value)?;
        // Insert the key-value into the trie, and return the new root node
        let root_node = root_node.insert(self.db_mut(), &key_nb, bin_node)?;
        // Set the new root node to the trie
        self.set_root_node(root_node);
        // Set the dirty flag
        self.set_dirty(true);

        Ok(())
    }

    /// Get a value from the trie
    fn get_value(&self, key: &K) -> Result<Option<V>> {
        // Convert the key to nibble
        let key_nb: NibbleVec = util::convert_bytes_to_nibbles(key.as_ref());
        Ok(self
            .root_node()
            .get_value(self.db_ref(), &key_nb)?
            .map(|bin_node| bincode::deserialize(&bin_node).unwrap()))
    }

    /// Commit the data to the database,
    /// after the commit, the node data will become hash
    /// and return the root hash
    fn commit(&mut self) -> Result<Option<HashValue>> {
        let root_node = self.take_root_node();
        let root_node = root_node.collapse(self.db_mut())?;
        self.set_root_node(root_node);
        self.set_dirty(false);

        match self.root_node() {
            TrieNodeLink::HashValue(hash_value) => Ok(Some(hash_value.clone())),
            TrieNodeLink::Empty => Ok(None),
            _ => unreachable!(),
        }
    }

    /// Revert to a version
    fn revert(&mut self, root_hash: HashValue) -> Result<()> {
        self.set_root_node(TrieNodeLink::HashValue(root_hash));
        self.set_dirty(false);
        Ok(())
    }

    /// Get the proof of the key
    fn get_proof(&mut self, root_hash: &HashValue, key: &K) -> Result<(bool, MemoryDatabase)> {
        if self.dirty() {
            self.commit()?;
        }
        let mut proof_db = MemoryDatabase::new();
        let bin_node = self.db_ref().get(root_hash)?;
        match bin_node {
            Some(bin_node) => {
                let trie_node: TrieNode = bincode::deserialize(&bin_node)?;
                let key_nb = util::convert_bytes_to_nibbles(key.as_ref());
                proof_db.insert(*root_hash, bin_node)?;
                let exists = trie_node.get_proof(
                    self.db_ref(),
                    &mut proof_db,
                    &key_nb,
                )?;
                Ok((exists, proof_db))
            }
            None => return Ok((false, proof_db)),
        }
    }
}

#[allow(dead_code)]
pub fn verify_proof<K, V>(
    root_hash: &HashValue,
    proof_db: &impl Database,
    key: &K,
) -> Result<Option<V>>
where
    K: AsRef<[u8]>,
    V: Serialize + DeserializeOwned,
{
    match proof_db.get(&root_hash)? {
        Some(bin_node) => {
            let trie_node: TrieNode = bincode::deserialize(&bin_node)?;
            let key_nb = util::convert_bytes_to_nibbles(key.as_ref());
            let bin_value_opt = trie_node.get_value(proof_db, &key_nb)?;
            match bin_value_opt {
                Some(bin_value) => Ok(Some(bincode::deserialize(&bin_value)?)),
                None => Ok(None),
            }
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{memory_trie::MemoryTrie, *};

    #[test]
    fn memory_trie_works() {
        let mut trie = MemoryTrie::<&'static str, String>::new();
        trie_works(&mut trie);
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn rocksdb_trie_works() {
        use super::rocksdb_trie::RocksdbTrie;
        let db_path = "/tmp/tinympt_db".into();
        let mut trie = RocksdbTrie::<&'static str, String>::new(db_path);
        trie_works(&mut trie);
    }

    /// 这里面向 trait 测试，带来了两点好处：
    /// 1、能够以 trait 的视角，在测试时重点关注 trait 行为定义是否合理，比如参数等，是否满足好的用户体验
    /// 2、每个实现的测试都可以复用这个方法
    fn trie_works<'a, T>(trie: &mut T)
    where
        T: Trie<&'a str, String>,
    {
        // 准备数据
        // Substrate 的 key 是由多个部分组成的
        // https://docs.substrate.io/fundamentals/state-transitions-and-storage/
        let kv1 = ("0000", "value01".to_string());
        let kv2 = ("00001111", "value02".to_string());

        // 插入数据一
        trie.insert(kv1.0, kv1.1.clone()).unwrap();
        // 提交，并获得 root hash
        let root_hash1 = trie.commit().unwrap().unwrap();
        // 根据数据一的 key 获取 value
        let value = trie.get_value(&kv1.0).unwrap().unwrap();
        // 检查 value 是否正确
        assert_eq!(value, kv1.1);

        // 插入数据二
        trie.insert(kv2.0, kv2.1.clone()).unwrap();
        // 提交
        let _ = trie.commit().unwrap().unwrap();
        // 根据数据二的 key 获取 value
        let value = trie.get_value(&kv2.0).unwrap().unwrap();
        // 检查 value 是否正确
        assert_eq!(value, kv2.1);

        // 将 trie 恢复到 root hash 1
        trie.revert(root_hash1).unwrap();

        // 根据数据二的 key 获取 value
        let value = trie.get_value(&kv2.0).unwrap();
        // 检查 value 是否存在, 因为 trie 已经恢复到 root hash 1，所以数据二不存在
        assert!(value.is_none());
        // 根据数据一的 key 获取 value
        let value = trie.get_value(&kv1.0).unwrap();
        // 检查 value 是否存在
        assert!(value.is_some());
    }

    #[test]
    fn memory_proof_works() {
        let mut trie = MemoryTrie::<&'static str, String>::new();
        proof_works(&mut trie);
    }

    fn proof_works<'a, T>(trie: &mut T)
    where
        T: Trie<&'a str, String>,
    {
        // 准备数据
        let kv1 = ("0000", "value01".to_string());
        let kv2 = ("00001111", "value02".to_string());

        // 插入数据一
        trie.insert(kv1.0, kv1.1.clone()).unwrap();
        // 提交，得到 root hash
        let root_hash1 = trie.commit().unwrap().unwrap();

        trie.insert(kv2.0, kv2.1.clone()).unwrap();
        // 插入数据二
        let root_hash2 = trie.commit().unwrap().unwrap();
        // 提交，再次得到 root hash

        // 获得 proof
        let (exists, proof_db) = trie.get_proof(&root_hash2, &kv1.0).unwrap();
        // 检查数据是否存在
        assert!(exists);
        // 验证 proof
        let value = verify_proof::<_, String>(&root_hash2, &proof_db, &kv1.0)
            .unwrap()
            .unwrap();
        // 检查 value 是否正确
        assert_eq!(value, kv1.1);

        let (exists, proof_db) = trie.get_proof(&root_hash2, &kv2.0).unwrap();
        assert!(exists);
        let value = verify_proof::<_, String>(&root_hash2, &proof_db, &kv2.0)
            .unwrap()
            .unwrap();
        assert_eq!(value, kv2.1);

        let (exists, proof_db) = trie.get_proof(&root_hash1, &kv1.0).unwrap();
        assert!(exists);
        let value = verify_proof::<_, String>(&root_hash1, &proof_db, &kv1.0)
            .unwrap()
            .unwrap();
        assert_eq!(value, kv1.1);

        let (exists, _) = trie.get_proof(&root_hash1, &kv2.0).unwrap();
        assert!(!exists);
    }
}
