use crate::database::Database;
use crate::trie::node::{TrieNode, TrieNodeLink};
use crate::trie::util;
use crate::{Result, NibbleSlice};
use array_init::array_init;
use serde::{Deserialize, Serialize};

/// 分支节点
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Branch {
    pub children: [TrieNodeLink; 16],
    pub value: Option<Vec<u8>>,
}

impl Branch {
    pub fn new() -> Self {
        Self {
            children: array_init(|_| TrieNodeLink::Empty),
            value: None,
        }
    }

    /// 向分支节点插入数据
    pub fn insert(
        mut self,
        db: &mut impl Database,
        key_nb: &NibbleSlice,
        value: Vec<u8>,
    ) -> Result<TrieNode> {
        // 如果 key_nb 为空，那么我们只能将 value 放入 branch 的 value 属性中
        if key_nb.len() == 0 {
            self.value = value.into();
            return Ok(self.into());
        }

        // 将 key_nb 的第一个 nibble 取出来，用来决定将数据插入到哪个 child 中
        let (idx, key_nb) = key_nb.split_at(1);
        // 从 children 数组中取出对应的 trie_node_link
        let trie_node_link =
            std::mem::replace(&mut self.children[idx[0] as usize], TrieNodeLink::Empty);
        // 向 trie_node_link 中插入数据
        let child = trie_node_link.insert(db, key_nb.into(), value)?;
        // 将 child 放回 children 数组中
        self.set_child(idx[0] as usize, child);
        Ok(self.into())
    }

    /// 设置 children 数组中的某个元素
    pub fn set_child(&mut self, index: usize, child: TrieNodeLink) {
        self.children[index] = child;
    }

    /// 将分支节点压缩, 压缩的过程就是将节点存入数据库中, 并返回一个 TrieNodeLink::HashValue
    pub fn collapse(self, db: &mut impl Database) -> Result<TrieNodeLink> {
        // 使用解构语法将 self 分解成三个部分
        // 解构也可以直接写在函数的参数中，如: 
        // pub fn collapse(Branch { children, value }: Self, db: &mut impl Database) -> Result<TrieNodeLink> {
        // 这两种都可以，按自己的喜好及团队的要求决定
        let Branch {
            children,
            value,
        } = self;

        // 创建一个新的 branch
        let mut branch = Branch::new();
        // branch value 属性直接使用 self 的
        branch.value = value;

        // 遍历 children 数组, 将其中的 TrieNodeLink::Branch 节点压缩
        for (i, child) in children.into_iter().enumerate() {
            branch.set_child(i, child.collapse(db).unwrap());
        }

        // 将 branch 转换成 Vec<u8>
        let data: Vec<u8> = branch.try_into()?;
        // 计算 hash 值
        let hash_value = util::hash(&data);

        // 将数据存入数据库中
        db.insert(hash_value, data)?;
        // 返回 TrieNodeLink::HashValue
        Ok(TrieNodeLink::HashValue(hash_value))
    }

    /// 从 branch 中获取数据
    pub fn get_value(&self, db: &impl Database, key_nb: &NibbleSlice) -> Result<Option<Vec<u8>>> {
        // 如果 key_nb 为空，那么我们只能从 branch 的 value 属性中获取数据
        if key_nb.len() == 0 {
            return Ok(self.value.clone());
        }

        // 将 key_nb 的第一个 nibble 取出来，用来决定从哪个 child 中获取数据
        let (idx, key_nb) = key_nb.split_at(1);
        // 从 children 数组中取出对应的 trie_node_link
        let child = &self.children[idx[0] as usize];
        // 从 trie_node_link 中获取数据
        child.get_value(db, &key_nb)
    }

    /// 从 branch 中获取 proof
    pub fn get_proof(
        &self,
        db: &impl Database,
        proof_db: &mut impl Database,
        key_nb: &NibbleSlice,
    ) -> Result<bool> {
        // 如果 key_nb 为空，那么我们只能从 branch 的 value 属性中获取数据
        if key_nb.len() == 0 {
            // 如果 value 为 None，那么返回 false
            return match self.value {
                Some(_) => Ok(true),
                None => Ok(false),
            };
        }

        // 将 key_nb 的第一个 nibble 取出来，用来决定从哪个 child 中获取数据
        let (idx, key_nb) = key_nb.split_at(1);
        // 从 children 数组中取出对应的 trie_node_link
        let child = &self.children[idx[0] as usize];
        // 从 trie_node_link 中获取数据
        let exists = child.get_proof(db, proof_db, &key_nb)?;
        Ok(exists)
    }
}

/// 将 Branch 转换成 Vec<u8>
impl TryFrom<Branch> for Vec<u8> {
    type Error = bincode::Error;

    fn try_from(value: Branch) -> std::result::Result<Self, Self::Error> {
        bincode::serialize(&value)
    }
}
