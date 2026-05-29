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
use crate::provider::llm::types::ToolDefinition;
use crate::provider::mcp::McpManager;
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

// ─── 工具过滤与转换 ─────────────────────────────────────────

/// 获取指定会话的可用工具列表
///
/// 流程：
/// 1. 从 McpManager 获取所有运行中的连接
/// 2. 遍历每个连接，收集所有工具
/// 3. 根据 ChatToolsConfig 过滤被禁用的 server/tool
/// 4. 转换为统一的 ToolDefinition 格式
pub async fn get_enabled_tools(
    mcp_manager: &McpManager,
    cache: &Cache,
    account_id: &str,
    session_id: &str,
) -> Vec<ToolDefinition> {
    // 加载工具配置
    let config = load_config(cache, account_id, session_id);
    let mut tools = Vec::new();

    // 获取所有运行中的连接
    let connections = {
        // McpManager 的 connections 是 RwLock，通过 list_all_status 获取活跃的名称
        // 然后逐个获取工具
        let statuses = mcp_manager.list_all_status();
        statuses
            .into_iter()
            .filter(|s| s.health == "connected")
            .map(|s| s.name)
            .collect::<Vec<_>>()
    };

    for name in connections {
        // 检查 server 是否被禁用
        if config.is_server_disabled(&name) {
            debug!("[get_enabled_tools] server '{}' is disabled", name);
            continue;
        }

        // 获取该 server 的工具列表
        match mcp_manager.get_tools(&name).await {
            Ok(mcp_tools) => {
                for tool in mcp_tools {
                    let tool_name = tool.name.clone();
                    // 检查单个工具是否被禁用
                    if config.is_tool_disabled(&name, &tool_name) {
                        debug!("[get_enabled_tools] tool '{}/{}' is disabled", name, tool_name);
                        continue;
                    }

                    // MCP Tool 的 input_schema 是 Arc<serde_json::Map>
                    let input_schema = serde_json::Value::Object((*tool.input_schema).clone());
                    // 转换为统一格式（MCP 标准格式: "mcp__server__tool"）
                    // 例如: "mcp__browser__navigate", "mcp__playwright__click"
                    let mcp_tool_name = format!("mcp__{}__{}", name, tool_name);
                    tools.push(ToolDefinition::from_mcp(
                        &mcp_tool_name,
                        tool.description.as_deref(),
                        input_schema,
                    ));
                }
            }
            Err(e) => {
                warn!("[get_enabled_tools] failed to get tools from '{}': {}", name, e);
            }
        }
    }

    debug!(
        "[get_enabled_tools] total enabled tools: {} (account={}, session={})",
        tools.len(),
        account_id,
        session_id
    );
    tools
}
