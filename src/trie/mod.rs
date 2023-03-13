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
    /// 数据库的类型
    type Database: Database;

    /// 如果 trie 是 dirty 的，那么意味着数据还没有被提交
    fn dirty(&self) -> bool;

    /// 设置 dirty 标志
    fn set_dirty(&mut self, dirty: bool);

    /// 获得 trie 的根节点
    fn root_node(&self) -> &TrieNodeLink;

    /// 用来从 trie 里移除根节点
    fn take_root_node(&mut self) -> TrieNodeLink;

    /// 设置 tire 的根节点
    fn set_root_node(&mut self, node: TrieNodeLink);

    /// 获得数据库的可变引用
    fn db_mut(&mut self) -> &mut Self::Database;

    /// 获得数据库的不可变引用
    fn db_ref(&self) -> &Self::Database;

    /// 向 trie 里插入一个 key-value
    fn insert(&mut self, key: K, value: V) -> Result<()> {
        // 将 key 转换为 nibble 形式
        let key_nb: NibbleVec = util::convert_bytes_to_nibbles(key.as_ref());
        // 取得 trie 的根节点
        let root_node = self.take_root_node();
        // 将 value 序列化
        let bin_node = bincode::serialize(&value)?;
        // 将 key-value 插入到 trie 里，并返回新的根节点
        let root_node = root_node.insert(self.db_mut(), &key_nb, bin_node)?;
        // 将新的根节点设置到 trie 里
        self.set_root_node(root_node);
        // 设置 dirty 标志
        self.set_dirty(true);

        Ok(())
    }

    /// 获得 trie 里的一个 key-value
    fn get_value(&self, key: &K) -> Result<Option<V>> {
        // Convert the key to nibble
        let key_nb: NibbleVec = util::convert_bytes_to_nibbles(key.as_ref());
        Ok(self
            .root_node()
            .get_value(self.db_ref(), &key_nb)?
            .map(|bin_node| bincode::deserialize(&bin_node).unwrap()))
    }

    /// 把数据提交到数据库里，提交之后，节点数据会变成 hash，然后返回根 hash
    fn commit(&mut self) -> Result<Option<HashValue>> {
        // 获得根节点
        let root_node = self.take_root_node();
        // 压缩根节点
        let root_node = root_node.collapse(self.db_mut())?;
        // 重新设置根节点
        self.set_root_node(root_node);
        // 设置 dirty 标志
        self.set_dirty(false);

        match self.root_node() {
            TrieNodeLink::HashValue(hash_value) => Ok(Some(hash_value.clone())),
            TrieNodeLink::Empty => Ok(None),
            // 压缩以后的 trie, 要么是Empty，要么是HashValue，不可能到这里
            _ => unreachable!(),
        }
    }

    /// 恢复到一个版本
    fn revert(&mut self, root_hash: HashValue) -> Result<()> {
        // 设置根节点
        self.set_root_node(TrieNodeLink::HashValue(root_hash));
        // 设置 dirty 标志
        self.set_dirty(false);
        Ok(())
    }

    /// 获得 proof，proof 里包含了 key 的路径上的所有节点, bool 表示 key 是否存在， MemoryDatabase 是保存 proof 的数据库
    fn get_proof(&mut self, root_hash: &HashValue, key: &K) -> Result<(bool, MemoryDatabase)> {
        // 如果 trie 是 dirty 的，那么先提交
        if self.dirty() {
            self.commit()?;
        }
        // 创建一个 MemoryDatabase
        let mut proof_db = MemoryDatabase::new();
        // 从数据库里获得根节点的二进制数据
        let bin_node_opt = self.db_ref().get(root_hash)?;
        match bin_node_opt {
            Some(bin_node) => {
                // 反序列化根节点
                let trie_node: TrieNode = bincode::deserialize(&bin_node)?;
                // 将 key 转换为 nibble 形式
                let key_nb = util::convert_bytes_to_nibbles(key.as_ref());
                // 将根节点插入到 proof_db 里
                proof_db.insert(*root_hash, bin_node)?;
                // 通过查找key,将沿途路径上的节点收集到 proof_db 里
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

/// 验证 proof, 返回 key 对应的 value
/// 如果 key 存在，那么返回 Some(value)，表示验证成功
/// 如果 key 不存在，那么返回 None, 表明验证失败
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
            // 反序列化根节点
            let trie_node: TrieNode = bincode::deserialize(&bin_node)?;
            // 将 key 转换为 nibble 形式
            let key_nb = util::convert_bytes_to_nibbles(key.as_ref());
            // 从根节点里获得 key 对应的 value
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
    /// 1、能够以 trait 的视角，在测试时重点关注 trait 行为定义是否合理，比如参数，返回值等，是否满足好的用户体验
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
