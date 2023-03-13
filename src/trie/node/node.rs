use crate::database::Database;
use crate::trie::node::{Branch, Extension, TrieNode, TrieNodeLink};
use crate::trie::util;
use crate::{NibbleSlice, Result};
use crate::{NibbleVec, TrieError};
use serde::{Deserialize, Serialize};

/// 叶子节点
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Node {
    pub rest_of_key: NibbleVec,
    pub value: Vec<u8>,
}

impl Node {
    pub fn new(rest_of_key: NibbleVec, value: Vec<u8>) -> Self {
        Self { rest_of_key, value }
    }

    /// 向叶子节点插入数据
    pub fn insert(
        self,
        db: &mut impl Database,
        key_nb: &NibbleSlice,
        value: Vec<u8>,
    ) -> Result<TrieNode> {
        // 如果叶子节点的key与要插入的key相同，则直接替换
        if self.rest_of_key == key_nb {
            return Ok(Node {
                rest_of_key: self.rest_of_key,
                value,
            }
            .into());
        }

        // 获得叶子节点的key与要插入的key的公共前缀
        let (shared, rest_of_key, rest_of_key_nb) =
            util::parse_nibble_slices_shared_portion(&self.rest_of_key, &key_nb);
        // 构建一个新的 Branch
        let branch = Branch::new();

        // 将原来的叶子节点插入到新 Branch 中
        let branch = branch.insert(db, rest_of_key, self.value)?;
        // 将新的键值对插入到新 Branch 中
        let branch = branch.insert(db, rest_of_key_nb, value)?;

        // 如果 shared 为空，则直接返回 branch
        Ok(if shared.len() == 0 {
            branch.into()
        } else {
            // 否则将 shared 作为 partial_key, 新的 branch 作为 branch, 构建一个新的 Extension
            Extension {
                partial_key: shared.to_owned(),
                branch: TrieNodeLink::TrieNode(Box::new(branch)),
            }
            .into()
        })
    }

    // 从叶子节点中获取数据
    pub fn get_value(&self, key_nb: &NibbleSlice) -> Result<Option<Vec<u8>>> {
        // 如果叶子节点的key与要获取的key相同，则返回叶子节点的value
        if self.rest_of_key == *key_nb {
            return Ok(Some(self.value.clone()));
        }

        // 否则返回 None
        Ok(None)
    }
}

/// 将 Node 转换为 Vec<u8>
impl TryFrom<Node> for Vec<u8> {
    type Error = TrieError;

    fn try_from(value: Node) -> std::result::Result<Self, Self::Error> {
        Ok(bincode::serialize(&value)?)
    }
}
