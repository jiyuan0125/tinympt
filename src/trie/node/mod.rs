use serde::{Deserialize, Serialize};

use super::util;
use crate::database::Database;
use crate::{HashValue, NibbleSlice, Result, TrieError};

mod branch;
mod extension;
mod node;

pub use branch::*;
pub use extension::*;
pub use node::*;

/// 表现一个 Trie 节点
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub enum TrieNode {
    Extension(Extension),
    Node(Node),
    Branch(Branch),
}

impl TrieNode {
    /// 向 TrieNode 中插入数据
    pub fn insert(
        self,
        db: &mut impl Database,
        key_nb: &NibbleSlice,
        value: Vec<u8>,
    ) -> Result<Self> {
        match self {
            TrieNode::Node(node) => Ok(node.insert(db, key_nb, value)?.into()),
            TrieNode::Extension(extension) => Ok(extension.insert(db, key_nb, value)?.into()),
            TrieNode::Branch(branch) => Ok(branch.insert(db, key_nb, value)?.into()),
        }
    }

    /// 从 TrieNode 中获得数据
    pub fn get_value(&self, db: &impl Database, key_nb: &NibbleSlice) -> Result<Option<Vec<u8>>> {
        match self {
            TrieNode::Node(node) => node.get_value(key_nb),
            TrieNode::Extension(extension) => extension.get_value(db, key_nb),
            TrieNode::Branch(branch) => branch.get_value(db, key_nb),
        }
    }

    /// 从 TrieNode 中获得 proof
    pub fn get_proof(
        &self,
        db: &impl Database,
        proof_db: &mut impl Database,
        key_nb: &NibbleSlice,
    ) -> Result<bool> {
        match self {
            TrieNode::Node(node) => Ok(node.rest_of_key == *key_nb),
            TrieNode::Extension(extension) => extension.get_proof(db, proof_db, key_nb),
            TrieNode::Branch(branch) => branch.get_proof(db, proof_db, key_nb),
        }
    }

    /// 将 TridNode 压缩，压缩的过程就是将节点存入数据库中, 并返回一个 TrieNodeLink::HashValue
    pub fn collapse(self, db: &mut impl Database) -> Result<TrieNodeLink> {
        let trie_node = match self {
            // 如果是 TrieNode::Node, 那么直接返回
            TrieNode::Node(_) => TrieNode::from(self),
            // 如果是 TrieNode::Extension, 那么将其分支节点进行压缩
            TrieNode::Extension(Extension {
                partial_key,
                branch,
            }) => Extension {
                partial_key,
                branch: branch.collapse(db)?,
            }
            .into(),
            // 如果是 TrieNode::Branch, 那么将其分支节点进行压缩
            TrieNode::Branch(Branch {
                children: old_children,
                value,
            }) => {
                let mut children: [TrieNodeLink; 16] =
                    array_init::array_init(|_| TrieNodeLink::Empty);
                for (idx, child) in old_children.into_iter().enumerate() {
                    children[idx] = child.collapse(db)?;
                }
                Branch { children, value }.into()
            }
        };

        // 使用 bincode 序列化 TrieNode
        let bin_node = bincode::serialize(&trie_node)?;
        // 使用 util::hash 计算 TrieNode 的 hash 值
        let hash_value = util::hash(&bin_node);
        // 将 TrieNode 存入数据库中
        db.insert(hash_value, bin_node)?;

        Ok(TrieNodeLink::HashValue(hash_value))
    }

}

/// 将 Extension 转换为 TrieNode
impl From<Extension> for TrieNode {
    fn from(value: Extension) -> Self {
        TrieNode::Extension(value)
    }
}

/// 将 Node 转换为 TrieNode
impl From<Node> for TrieNode {
    fn from(value: Node) -> Self {
        TrieNode::Node(value)
    }
}

/// 将 Branch 转换为 TrieNode
impl From<Branch> for TrieNode {
    fn from(value: Branch) -> Self {
        TrieNode::Branch(value)
    }
}

/// 表现一个 TrieNode 的链接
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub enum TrieNodeLink {
    TrieNode(Box<TrieNode>),
    HashValue(HashValue),
    Empty,
}

/// 默认值为 TrieNodeLink::Empty
impl Default for TrieNodeLink {
    fn default() -> Self {
        Self::Empty
    }
}

impl TrieNodeLink {
    /// 从 TrieNodeLink 中获得数据
    pub fn get_value(&self, db: &impl Database, key_nb: &NibbleSlice) -> Result<Option<Vec<u8>>> {
        match self {
            TrieNodeLink::TrieNode(trie_node) => trie_node.get_value(db, key_nb),
            TrieNodeLink::HashValue(hash_value) => {
                let bin_node = db.get(hash_value)?.ok_or(TrieError::Database(format!(
                    "value for `{}` not found",
                    hex::encode(hash_value)
                )))?;
                let trie_node: TrieNode = bincode::deserialize(&bin_node)?;
                trie_node.get_value(db, key_nb)
            }
            TrieNodeLink::Empty => Ok(None),
        }
    }

    /// 从 TrieNodeLink 中获得 proof
    pub fn get_proof(
        &self,
        db: &impl Database,
        proof_db: &mut impl Database,
        key_nb: &NibbleSlice,
    ) -> Result<bool> {
        match self {
            TrieNodeLink::TrieNode(trie_node) => trie_node.get_proof(db, proof_db, key_nb),
            TrieNodeLink::HashValue(hash_value) => {
                let bin_node = db.get(hash_value)?.ok_or(TrieError::Database(format!(
                    "value for `{}` not found",
                    hex::encode(hash_value)
                )))?;
                let trie_node: TrieNode = bincode::deserialize(&bin_node)?;
                proof_db.insert(*hash_value, bin_node)?;
                trie_node.get_proof(db, proof_db, key_nb)
            }
            TrieNodeLink::Empty => Ok(false),
        }
    }

    /// 向 TrieNodeLink 中插入一个键值对
    /// 注意: 值的类型是 Vec<u8>, 并且在递归传递中使用了移动语义, 没有引入额外的堆分配
    pub fn insert(
        self,
        db: &mut impl Database,
        key_nb: &NibbleSlice,
        value: Vec<u8>,
    ) -> Result<Self> {
        match self {
            // 如果是 TrieNodeLink::TrieNode, 那么直接调用 TrieNode::insert
            TrieNodeLink::TrieNode(trie_node) => Ok(trie_node.insert(db, key_nb, value)?.into()),
            // 如果是 TrieNodeLink::HashValue, 那么先从数据库中读取 TrieNode, 然后调用 TrieNode::insert
            TrieNodeLink::HashValue(hash_value) => {
                let bin_node = db.get(&hash_value)?.ok_or(TrieError::Database(format!(
                    "Value for `{}` not found",
                    hex::encode(hash_value)
                )))?;
                let trie_node: TrieNode = bincode::deserialize(&bin_node)?;
                Ok(trie_node.insert(db, key_nb, value)?.into())
            }
            // 如果是 TrieNodeLink::Empty, 那么直接创建一个 Node
            TrieNodeLink::Empty => {
                let node = Node::new(key_nb.to_owned(), value);
                Ok(node.into())
            }
        }
    }

    /// 压缩 TrieNodeLink 
    pub fn collapse(self, db: &mut impl Database) -> Result<TrieNodeLink> {
        match self {
            // 如果是 TrieNodeLink::TrieNode, 那么直接调用 TrieNode::collapse
            TrieNodeLink::TrieNode(trie_node) => Ok(trie_node.collapse(db)?),
            // 其他情况, HashValue 或 Empty, 直接返回
            _ => Ok(self),
        }
    }
}

/// 将 TrieNode 转换为 Vec<u8>
impl TryFrom<TrieNode> for Vec<u8> {
    type Error = TrieError;

    fn try_from(value: TrieNode) -> std::result::Result<Self, Self::Error> {
        match value {
            TrieNode::Extension(extension) => Ok(extension.try_into()?),
            TrieNode::Node(node) => Ok(node.try_into()?),
            TrieNode::Branch(branch) => Ok(branch.try_into()?),
        }
    }
}

/// 将 Extension 转换为 TrieNodeLink
impl From<Extension> for TrieNodeLink {
    fn from(value: Extension) -> Self {
        TrieNodeLink::TrieNode(Box::new(value.into()))
    }
}

/// 将 Node 转换为 TrieNodeLink
impl From<Node> for TrieNodeLink {
    fn from(value: Node) -> Self {
        TrieNodeLink::TrieNode(Box::new(value.into()))
    }
}

/// 将 Branch 转换为 TrieNodeLink
impl From<Branch> for TrieNodeLink {
    fn from(value: Branch) -> Self {
        TrieNodeLink::TrieNode(Box::new(value.into()))
    }
}

/// 将 TrieNode 转换为 TrieNodeLink
impl From<TrieNode> for TrieNodeLink {
    fn from(value: TrieNode) -> Self {
        match value {
            TrieNode::Extension(extension) => extension.into(),
            TrieNode::Node(node) => node.into(),
            TrieNode::Branch(branch) => branch.into(),
        }
    }
}
