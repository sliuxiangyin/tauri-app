use once_cell::sync::OnceCell;
use std::sync::Arc;
use tracing::debug;

use crate::provider::mcp_v2::error::{McpManagerError, Result};

/// 全局 Cache 单例（使用 Arc 包装）
static CACHE_INSTANCE: OnceCell<Arc<Cache>> = OnceCell::new();

/// 通用键值缓存管理器
///
/// 基于 sled 嵌入式数据库，内部使用 `Mutex` 包装确保线程安全。
/// 存储原始字节（`Vec<u8>`），序列化/反序列化由调用方负责。
pub struct Cache {
    db: std::sync::Mutex<sled::Db>,
}

impl Cache {
    /// 在指定路径创建或打开缓存数据库
    pub fn open(path: &str) -> Result<Self> {
        let db = sled::open(path).map_err(|e| {
            McpManagerError::CacheError(format!("failed to open sled db at '{}': {}", path, e))
        })?;
        Ok(Self {
            db: std::sync::Mutex::new(db),
        })
    }

    /// 设置全局单例（仅可在 Tauri setup 中调用一次）
    pub fn set_global(cache: Cache) -> Result<Arc<Cache>> {
        let arc = Arc::new(cache);
        CACHE_INSTANCE
            .set(arc.clone())
            .map_err(|_| McpManagerError::CacheError("Cache global instance already set".to_string()))?;
        Ok(arc)
    }

    /// 获取全局单例（必须在初始化后调用）
    pub fn get_global() -> Result<Arc<Cache>> {
        CACHE_INSTANCE
            .get()
            .cloned()
            .ok_or_else(|| McpManagerError::CacheError("Cache not initialized".to_string()))
    }

    /// 存入键值对（覆盖已有记录）
    pub fn put(&self, key: &str, value: Vec<u8>) -> Result<()> {
        let db = self
            .db
            .lock()
            .map_err(|e| McpManagerError::CacheError(format!("cache mutex poisoned: {}", e)))?;
        db.insert(key, value)
            .map_err(|e| McpManagerError::CacheError(format!("write cache failed: {}", e)))?;
        db.flush()
            .map_err(|e| McpManagerError::CacheError(format!("flush cache failed: {}", e)))?;

        debug!("cached entry for key '{}'", key);
        Ok(())
    }

    /// 根据键读取值，不存在时返回 `None`
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let db = self
            .db
            .lock()
            .map_err(|e| McpManagerError::CacheError(format!("cache mutex poisoned: {}", e)))?;
        match db.get(key) {
            Ok(Some(v)) => Ok(Some(v.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(McpManagerError::CacheError(format!(
                "read cache failed: {}",
                e
            ))),
        }
    }

    /// 删除指定键的缓存项
    pub fn remove(&self, key: &str) -> Result<()> {
        let db = self
            .db
            .lock()
            .map_err(|e| McpManagerError::CacheError(format!("cache mutex poisoned: {}", e)))?;
        db.remove(key)
            .map_err(|e| McpManagerError::CacheError(format!("remove cache failed: {}", e)))?;
        let _ = db.flush();
        debug!("removed cache entry for key '{}'", key);
        Ok(())
    }
}
