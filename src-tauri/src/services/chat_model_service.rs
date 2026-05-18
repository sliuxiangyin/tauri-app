//! Chat 模型选择服务
//!
//! 职责：
//! - 存储/获取 account_id 对应的模型选择
//! - 使用 sled 缓存持久化模型选择配置
//! - 提供获取当前账户模型的能力

use crate::provider::cache::Cache;
use crate::provider::mcp_v2::error::{McpManagerError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 账户模型选择配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountModelSelection {
    pub config_id: String,
    pub model_id: String,
}

const CHAT_MODEL_KEY_PREFIX: &str = "chat_model:";

/// Chat 模型服务
pub struct ChatModelService {
    cache: Arc<Cache>,
}

impl ChatModelService {
    /// 创建新的服务实例
    pub fn new(cache: Arc<Cache>) -> Self {
        Self { cache }
    }

    /// 保存账户的模型选择
    pub fn save_account_model(
        &self,
        account_id: &str,
        selection: &AccountModelSelection,
    ) -> Result<()> {
        let key = format!("{}{}", CHAT_MODEL_KEY_PREFIX, account_id);
        let value = serde_json::to_vec(selection)?;
        self.cache.put(&key, value)
    }

    /// 获取账户的模型选择
    pub fn get_account_model(&self, account_id: &str) -> Result<Option<AccountModelSelection>> {
        let key = format!("{}{}", CHAT_MODEL_KEY_PREFIX, account_id);
        match self.cache.get(&key)? {
            Some(v) => Ok(Some(serde_json::from_slice(&v)?)),
            None => Ok(None),
        }
    }
}