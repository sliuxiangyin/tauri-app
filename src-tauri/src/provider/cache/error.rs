//! Cache 模块错误类型定义

/// 缓存操作错误类型
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("sled db error: {0}")]
    Sled(#[from] sled::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("mutex poisoned: {0}")]
    Mutex(String),

    #[error("cache not initialized")]
    NotInitialized,

    #[error("global instance already set")]
    AlreadySet,
}

/// 缓存操作结果
pub type Result<T> = std::result::Result<T, CacheError>;