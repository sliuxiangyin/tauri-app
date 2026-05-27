//! 聊天工具服务层
//!
//! 职责：
//! - 管理特定 accountId + sessionId 组合下可使用的 MCP 工具列表
//! - 使用 cache 缓存机制存储和检索工具权限配置（不使用数据库）
//!
//! 缓存键格式：`chat_tools:{account_id}:{session_id}`
//! 缓存值格式：JSON 序列化的 ChatToolsConfig

use std::sync::Arc;

use crate::provider::cache::Cache;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// 聊天工具权限配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatToolsConfig {
    /// 被禁用的 MCP Server 列表
    pub disabled_servers: Vec<String>,
    /// 每个启用的 Server 中被禁用的工具列表
    /// Key: server_id, Value: 被禁用的工具 ID 列表
    pub disabled_tools: std::collections::HashMap<String, Vec<String>>,
}

impl ChatToolsConfig {
    /// 创建空配置（默认全部启用）
    pub fn new() -> Self {
        Self { disabled_servers: vec![], disabled_tools: std::collections::HashMap::new() }
    }

    /// 检查指定 server 是否被禁用
    pub fn is_server_disabled(&self, server_id: &str) -> bool {
        self.disabled_servers.contains(&server_id.to_string())
    }

    /// 检查指定 server 中的工具是否被禁用
    pub fn is_tool_disabled(&self, server_id: &str, tool_id: &str) -> bool {
        self.disabled_tools.get(server_id).map(|v| v.contains(&tool_id.to_string())).unwrap_or(false)
    }
}

/// 生成缓存键
fn cache_key(account_id: &str, session_id: &str) -> String {
    format!("chat_tools:{}:{}", account_id, session_id)
}

/// 从缓存加载配置，不存在时返回默认配置
pub fn load_config(cache: &Cache, account_id: &str, session_id: &str) -> ChatToolsConfig {
    let key = cache_key(account_id, session_id);
    match cache.get(&key) {
        Ok(Some(data)) => {
            match serde_json::from_slice::<ChatToolsConfig>(&data) {
                Ok(config) => {
                    debug!("[ChatToolsService] loaded config for {}/{}", account_id, session_id);
                    config
                }
                Err(e) => {
                    warn!("[ChatToolsService] failed to parse config for {}/{}: {}, using default",
                        account_id, session_id, e);
                    ChatToolsConfig::new()
                }
            }
        }
        Ok(None) => {
            debug!("[ChatToolsService] no config found for {}/{}, using default",
                account_id, session_id);
            ChatToolsConfig::new()
        }
        Err(e) => {
            warn!("[ChatToolsService] cache error for {}/{}: {}, using default",
                account_id, session_id, e);
            ChatToolsConfig::new()
        }
    }
}

/// 保存配置到缓存
pub fn save_config(cache: &Cache, account_id: &str, session_id: &str, config: &ChatToolsConfig) -> Result<(), String> {
    let key = cache_key(account_id, session_id);
    let data = serde_json::to_vec(config).map_err(|e| format!("serialize error: {}", e))?;
    cache.put(&key, data).map_err(|e| format!("cache error: {}", e))?;
    info!("[ChatToolsService] saved config for {}/{}", account_id, session_id);
    Ok(())
}

/// 删除指定 accountId + sessionId 的配置
pub fn delete_config(cache: &Cache, account_id: &str, session_id: &str) -> Result<(), String> {
    let key = cache_key(account_id, session_id);
    match cache.remove(&key) {
        Ok(_) => {
            info!("[ChatToolsService] deleted config for {}/{}", account_id, session_id);
            Ok(())
        }
        Err(e) => {
            warn!("[ChatToolsService] failed to delete config for {}/{}: {}", account_id, session_id, e);
            Err(format!("cache error: {}", e))
        }
    }
}

/// 获取所有已配置的 session 列表
/// 返回 Vec<(account_id, session_id)>
#[allow(dead_code)]
pub fn list_sessions(cache: &Cache) -> Result<Vec<(String, String)>, String> {
    // 由于 Cache 模块没有暴露迭代器，我们记录一个索引键
    let index_key = "chat_tools:__index__";
    let mut sessions = Vec::new();
    
    match cache.get(index_key) {
        Ok(Some(data)) => {
            let index: Vec<(String, String)> = serde_json::from_slice(&data)
                .map_err(|e| format!("index parse error: {}", e))?;
            sessions = index;
        }
        Ok(None) => {}
        Err(_) => {}
    }
    
    Ok(sessions)
}

/// 更新 session 索引
fn update_session_index(cache: &Cache, account_id: &str, session_id: &str) -> Result<(), String> {
    let index_key = "chat_tools:__index__";
    let mut sessions = match cache.get(index_key) {
        Ok(Some(data)) => {
            serde_json::from_slice::<Vec<(String, String)>>(&data)
                .unwrap_or_default()
        }
        _ => Vec::new()
    };
    
    let pair = (account_id.to_string(), session_id.to_string());
    if !sessions.contains(&pair) {
        sessions.push(pair);
    }
    
    let data = serde_json::to_vec(&sessions).map_err(|e| format!("serialize error: {}", e))?;
    cache.put(index_key, data).map_err(|e| format!("cache error: {}", e))?;
    Ok(())
}

// ─── 公开 API ─────────────────────────────────────────────

/// 获取指定 accountId + sessionId 的工具配置
pub fn get_tools_config(
    cache: Arc<Cache>,
    account_id: String,
    session_id: String,
) -> ChatToolsConfig {
    load_config(&cache, &account_id, &session_id)
}

/// 保存指定 accountId + sessionId 的工具配置
pub fn set_tools_config(
    cache: Arc<Cache>,
    account_id: String,
    session_id: String,
    config: ChatToolsConfig,
) -> Result<(), String> {
    save_config(&cache, &account_id, &session_id, &config)?;
    update_session_index(&cache, &account_id, &session_id)
}

/// 删除指定 accountId + sessionId 的工具配置
pub fn remove_tools_config(
    cache: Arc<Cache>,
    account_id: String,
    session_id: String,
) -> Result<(), String> {
    delete_config(&cache, &account_id, &session_id)
}

/// 批量获取多个 accountId + sessionId 的工具配置
pub fn batch_get_tools_config(
    cache: Arc<Cache>,
    requests: Vec<(String, String)>,
) -> Vec<ChatToolsConfig> {
    requests
        .into_iter()
        .map(|(account_id, session_id)| load_config(&cache, &account_id, &session_id))
        .collect()
}
