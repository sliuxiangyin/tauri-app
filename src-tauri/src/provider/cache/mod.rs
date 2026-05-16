use tracing::debug;

use crate::provider::mcp_v2::error::{McpManagerError, Result};

/// 通用键值缓存管理器
///
/// 基于 sled 嵌入式数据库，内部使用 `Mutex` 包装确保线程安全。
/// 存储原始字节（`Vec<u8>`），序列化/反序列化由调用方负责。
///
/// 单例模式：在 Tauri setup 中初始化一次，通过 `app.manage()` 注入全局状态。
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
