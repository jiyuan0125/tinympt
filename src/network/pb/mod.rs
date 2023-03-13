mod abi;

pub use abi::*;

use crate::{database::MemoryDatabase, HashValue, TrieError};

/// 将 ProofRequest 转换为 (HashValue, String)
impl TryFrom<ProofRequest> for (HashValue, String) {
    type Error = TrieError;

    fn try_from(v: ProofRequest) -> Result<Self, Self::Error> {
        let hash_value: HashValue = v
            .root_hash
            .try_into()
            .map_err(|_| TrieError::InvalidHashValue)?;

        let key = String::from_utf8(v.key).map_err(|_| TrieError::InvalidKey)?;
        Ok((hash_value, key))
    }
}

/// 将 (HashValue, String) 转换为 ProofRequest
impl From<(HashValue, String)> for ProofRequest {
    fn from(v: (HashValue, String)) -> Self {
        ProofRequest {
            root_hash: v.0.to_vec(),
            key: v.1.into_bytes(),
        }
    }
}

/// 将 ProofResponse 转换为 (bool, MemoryDatabase)
impl TryFrom<ProofResponse> for (bool, MemoryDatabase) {
    type Error = TrieError;

    fn try_from(v: ProofResponse) -> Result<(bool, MemoryDatabase), Self::Error> {
        let memory_db = bincode::deserialize(v.proof_db.as_slice())?;
        Ok((v.exists, memory_db))
    }
}

/// 将 (bool, MemoryDatabase) 转换为 ProofResponse
impl TryFrom<(bool, MemoryDatabase)> for ProofResponse {
    type Error = TrieError;

    fn try_from(v: (bool, MemoryDatabase)) -> Result<Self, Self::Error> {
        Ok(ProofResponse {
            exists: v.0,
            proof_db: bincode::serialize(&v.1)?,
        })
    }
}
