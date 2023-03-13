use thiserror::Error;

#[derive(Error, Debug)]
pub enum TrieError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),

    #[cfg(feature = "rocksdb")]
    #[error("Rocksdb error: {0}")]
    Rocksdb(#[from] rocksdb::Error),

    #[error("InvalidHashValue")]
    InvalidHashValue,
    #[error("InvalidKey")]
    InvalidKey,
}
