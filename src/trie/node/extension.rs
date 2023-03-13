use crate::database::Database;
use crate::trie::node::{Branch, TrieNode, TrieNodeLink};
use crate::trie::util;
use crate::TrieError;
use crate::{NibbleSlice, NibbleVec, Result};
use serde::{Deserialize, Serialize};

/// 扩展节点
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Extension {
    pub partial_key: NibbleVec,
    pub branch: TrieNodeLink,
}

impl Extension {
    /// 向扩展节点中插入一个新的键值对
    pub fn insert(
        self,
        db: &mut impl Database,
        key_nb: &NibbleSlice,
        value: Vec<u8>,
    ) -> Result<TrieNode> {
        // 解构扩展节点
        let Extension {
            partial_key,
            branch: old_branch,
        } = self;
        // 解析出共同的前缀
        let (shared, rest_of_partial_key, rest_of_key_nb) =
            util::parse_nibble_slices_shared_portion(&partial_key, &key_nb);

        let trie_node = match rest_of_partial_key.len() {
            // 如果扩展节点的 rest_of_partial_key 为空, 说明扩展节点的 partial_key 是 key_nb 的子集
            // 委托给 extension 的 branch 来处理
            0 => Extension {
                partial_key: shared.to_owned(),
                branch: old_branch.insert(db, rest_of_key_nb, value)?,
            }
            // 将 extension 转换为 TrieNode
            .into(),
            // 如果 rest_of_partial_key 只有一个 nibble 了，则直接将 old_branch 添加到新 Branch 对应的索引下
            1 => {
                // 构建一个新的 Branch
                let mut branch = Branch::new();
                // 将原来的 Branch 放在新 Branch 的对应索引下
                branch.set_child(rest_of_partial_key[0] as usize, old_branch);
                // 将新的键值对插入到新 Branch 中
                let branch = branch.insert(db, rest_of_key_nb, value)?;
                // 将 branch 转换为 TrieNode
                branch.into()
            }
            // 如果 rest_of_partial_key 不只有一个 nibble
            _ => {
                // 创建新的 Branch
                let mut branch = Branch::new();
                // 获得 rest_of_partial_key 的第一个 nibble
                let (idx, rest_of_partial_key) = rest_of_partial_key.split_at(1);
                // 创建新的 Extension, partial_key 为 rest_of_partial_key, branch 为原来的 Branch
                let extension = Extension {
                    partial_key: rest_of_partial_key.to_owned(),
                    branch: old_branch.into(),
                };
                // 将新的 Extension 放在新的 Branch 下面
                branch.set_child(idx[0] as usize, extension.into());
                // 将新的键值对插入到新的 Branch 中
                let branch = branch.insert(db, rest_of_key_nb, value)?;

                // 如果 shared 为空, 则直接返回 branch
                if shared.len() == 0 {
                    branch.into()
                } else {
                    // 否则返回另一个新的 Extension
                    // 新的 Extension 的 partial_key 为 shared, branch 为新的 Branch
                    Extension {
                        partial_key: shared.to_owned(),
                        branch: branch.into(),
                    }
                    .into()
                }
            }
        };

        Ok(trie_node)
    }

    /// 将扩展节点压缩，压缩的过程就是将分支节点的数据存入数据库中, 并返回一个 TrieNodeLink::HashValue
    pub fn collapse(self, db: &mut impl Database) -> Result<TrieNodeLink> {
        // 解构
        let Extension {
            partial_key,
            branch,
        } = self;
        // 构建一个新的 Extension
        let extension = Extension {
            partial_key,
            branch: branch.collapse(db)?,
        };

        // 将 Extension 转换为 Vec<u8>
        let data: Vec<u8> = extension.try_into()?;
        // 计算 hash 值
        let hash_value = util::hash(&data);
        // 将数据存入数据库中
        db.insert(hash_value, data)?;
        // 返回 TrieNodeLink::HashValue
        Ok(TrieNodeLink::HashValue(hash_value))
    }

    /// 从扩展节点中获得值
    pub fn get_value(&self, db: &impl Database, key_nb: &NibbleSlice) -> Result<Option<Vec<u8>>> {
        // 解析出共同的前缀
        match util::parse_nibble_slices_shared_portion(&self.partial_key, key_nb) {
            // 如果共同的前缀长度等于扩展节点的 partial_key 长度
            (shared, _, rest_of_key_nb) if shared.len() == self.partial_key.len() => {
                // 委托给 branch 来处理
                self.branch.get_value(db, rest_of_key_nb)
            }
            // 如果共同的前缀长度不等于扩展节点的 partial_key 长度，则说明没有找到
            _ => Ok(None),
        }
    }

    /// 从扩展节点获得 proof
    pub fn get_proof(
        &self,
        db: &impl Database,
        proof_db: &mut impl Database,
        key_nb: &NibbleSlice,
    ) -> Result<bool> {
        // 解析出共同的前缀
        let (shared, _, rest_of_key_nb) =
            util::parse_nibble_slices_shared_portion(&self.partial_key, key_nb);

        // 如果共同的前缀长度不等于扩展节点的 partial_key 长度，则说明没有找到
        if shared != self.partial_key {
            return Ok(false);
        }
        // 委托给 branch 来处理
        self.branch.get_proof(db, proof_db, rest_of_key_nb)
    }
}

/// 将 Extension 转换为 Vec<u8>
impl TryFrom<Extension> for Vec<u8> {
    type Error = TrieError;

    fn try_from(value: Extension) -> std::result::Result<Self, Self::Error> {
        Ok(bincode::serialize(&value)?)
    }
}
