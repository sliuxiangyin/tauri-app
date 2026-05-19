use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::provider::cache::Cache;
use crate::provider::mcp::config::McpServerConfig;
use crate::provider::mcp::connection::McpConnection;
use crate::provider::mcp::error::{McpError, Result};

/// 工具缓存的存储 key 前缀
const TOOLS_CACHE_KEY_PREFIX: &str = "mcp_tools:";

/// 带来源标识的工具信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolWithSource {
    /// 服务器 ID
    pub server_id: String,
    /// 服务器名称
    pub server_name: String,
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: Option<String>,
    /// 输入 schema
    pub input_schema: Value,
}

impl ToolWithSource {
    pub fn from_tool(server_id: &str, server_name: &str, tool: &Tool) -> Self {
        Self {
            server_id: server_id.to_string(),
            server_name: server_name.to_string(),
            name: tool.name.to_string(),
            description: tool.description.as_deref().map(|d| d.to_string()),
            input_schema: tool.schema_as_json_value(),
        }
    }
}

/// 服务器信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// 服务器配置
    pub config: McpServerConfig,
    /// 连接状态
    pub state: ServerState,
    /// 工具数量
    pub tool_count: usize,
}

/// 服务器状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ServerState {
    /// 等待队列处理
    Pending,
    /// 正在安装
    Installing,
    /// 正在连接
    Connecting,
    /// 已连接
    Connected,
    /// 连接断开
    Disconnected { reason: String },
    /// 连接失败
    Failed { error: String },
    /// 已停止
    Stopped,
}

impl Default for ServerState {
    fn default() -> Self {
        ServerState::Pending
    }
}

/// 服务器管理器
/// 负责管理所有 MCP 服务器的连接、工具缓存（持久化到 Cache）
pub struct ServerManager {
    /// 持久化缓存（sled）
    cache: Arc<Cache>,
    /// 服务器配置
    configs: RwLock<HashMap<String, McpServerConfig>>,
    /// 服务器连接
    connections: RwLock<HashMap<String, Arc<McpConnection>>>,
    /// 服务器状态
    states: RwLock<HashMap<String, ServerState>>,
    /// 工具缓存（内存）
    tool_cache: RwLock<HashMap<String, Vec<Tool>>>,
}

impl ServerManager {
    /// 创建新的服务器管理器
    pub fn new(cache: Arc<Cache>) -> Self {
        Self {
            cache,
            configs: RwLock::new(HashMap::new()),
            connections: RwLock::new(HashMap::new()),
            states: RwLock::new(HashMap::new()),
            tool_cache: RwLock::new(HashMap::new()),
        }
    }

    /// 从 Cache 恢复工具缓存到内存（初始化时调用）
    pub async fn restore_tools_cache(&self, server_ids: Vec<String>) {
        for server_id in server_ids {
            if let Some(tools) = self.load_tools_from_cache(&server_id).await {
                let mut cache = self.tool_cache.write().await;
                cache.insert(server_id, tools);
            }
        }
    }

    /// 从 Cache 加载工具缓存
    async fn load_tools_from_cache(&self, server_id: &str) -> Option<Vec<Tool>> {
        let key = format!("{}{}", TOOLS_CACHE_KEY_PREFIX, server_id);
        self.cache
            .get(&key)
            .ok()
            .flatten()
            .and_then(|data| serde_json::from_slice(&data).ok())
    }

    /// 保存工具到 Cache（持久化）
    async fn save_tools_to_cache(&self, server_id: &str, tools: &[Tool]) {
        let key = format!("{}{}", TOOLS_CACHE_KEY_PREFIX, server_id);
        if let Ok(data) = serde_json::to_vec(tools) {
            self.cache.put(&key, data).ok();
        }
    }

    /// 清除 Cache 中的工具缓存
    async fn clear_tools_cache(&self, server_id: &str) {
        let key = format!("{}{}", TOOLS_CACHE_KEY_PREFIX, server_id);
        self.cache.remove(&key).ok();
    }

    /// 初始化服务器管理器
    pub async fn init(&self, configs: Vec<McpServerConfig>) {
        let mut config_map = self.configs.write().await;
        let mut states = self.states.write().await;
        for config in configs {
            let id = config.id.clone();
            config_map.insert(id.clone(), config);
            // 初始化状态为 Pending（等待队列处理）
            states.insert(id, ServerState::Pending);
        }
        info!("ServerManager initialized with {} servers", config_map.len());
    }

    /// 更新服务器配置
    pub async fn update_config(&self, config: McpServerConfig) {
        let mut configs = self.configs.write().await;
        configs.insert(config.id.clone(), config);
    }

    /// 获取服务器配置
    pub async fn get_config(&self, server_id: &str) -> Option<McpServerConfig> {
        let configs = self.configs.read().await;
        configs.get(server_id).cloned()
    }

    /// 获取所有服务器配置
    pub async fn list_configs(&self) -> Vec<McpServerConfig> {
        let configs = self.configs.read().await;
        configs.values().cloned().collect()
    }

    /// 获取服务器状态
    pub async fn get_state(&self, server_id: &str) -> Option<ServerState> {
        let states = self.states.read().await;
        states.get(server_id).cloned()
    }

    /// 更新服务器状态
    pub async fn update_state(&self, server_id: &str, state: ServerState) {
        let mut states = self.states.write().await;
        states.insert(server_id.to_string(), state);
    }

    /// 获取所有服务器信息
    pub async fn list_servers(&self) -> Vec<ServerInfo> {
        let configs = self.configs.read().await;
        let states = self.states.read().await;
        let tool_cache = self.tool_cache.read().await;

        configs
            .values()
            .map(|config| {
                let state = states.get(&config.id).cloned().unwrap_or_default();
                let tool_count = tool_cache.get(&config.id).map(|t| t.len()).unwrap_or(0);
                ServerInfo {
                    config: config.clone(),
                    state,
                    tool_count,
                }
            })
            .collect()
    }

    /// 添加或更新连接（同时持久化 tools 到 Cache）
    pub async fn set_connection(&self, server_id: String, connection: McpConnection, tools: Vec<Tool>) {
        let conn = Arc::new(connection);
        let mut connections = self.connections.write().await;
        let mut tool_cache = self.tool_cache.write().await;

        connections.insert(server_id.clone(), conn);
        tool_cache.insert(server_id.clone(), tools.clone());

        // 持久化到 Cache
        self.save_tools_to_cache(&server_id, &tools).await;
    }

    /// 移除连接（同时清除 Cache 缓存）
    pub async fn remove_connection(&self, server_id: &str) -> Option<Arc<McpConnection>> {
        let mut connections = self.connections.write().await;
        let mut tool_cache = self.tool_cache.write().await;

        // 清除 Cache 缓存
        self.clear_tools_cache(server_id).await;
        
        tool_cache.remove(server_id);
        connections.remove(server_id)
    }

    /// 停止连接（但保留配置）
    pub async fn stop_connection(&self, server_id: &str) -> Result<()> {
        if let Some(conn) = self.remove_connection(server_id).await {
            conn.close().await?;
            let mut states = self.states.write().await;
            states.insert(server_id.to_string(), ServerState::Stopped);
        }
        Ok(())
    }

    /// 获取连接
    pub async fn get_connection(&self, server_id: &str) -> Option<Arc<McpConnection>> {
        let connections = self.connections.read().await;
        connections.get(server_id).cloned()
    }

    /// 获取工具列表（从缓存）
    pub async fn get_cached_tools(&self, server_id: &str) -> Option<Vec<Tool>> {
        let tool_cache = self.tool_cache.read().await;
        tool_cache.get(server_id).cloned()
    }

    /// 获取所有工具（带来源）
    pub async fn list_tools(&self, server_id: Option<&str>) -> Result<Vec<ToolWithSource>> {
        match server_id {
            Some(id) => {
                let configs = self.configs.read().await;
                let tool_cache = self.tool_cache.read().await;

                let name = configs
                    .get(id)
                    .map(|c| c.name.as_str())
                    .unwrap_or(id);
                let tools = tool_cache.get(id).cloned().unwrap_or_default();

                Ok(tools
                    .iter()
                    .map(|t| ToolWithSource::from_tool(id, name, t))
                    .collect())
            }
            None => {
                let configs = self.configs.read().await;
                let tool_cache = self.tool_cache.read().await;
                let mut all_tools = Vec::new();

                for id in configs.keys() {
                    let name = configs.get(id).map(|c| c.name.as_str()).unwrap_or(id);
                    if let Some(tools) = tool_cache.get(id) {
                        for tool in tools {
                            all_tools.push(ToolWithSource::from_tool(id, name, tool));
                        }
                    }
                }

                Ok(all_tools)
            }
        }
    }

    /// 调用工具
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(server_id)
            .ok_or_else(|| McpError::ServerNotFound {
                server_id: server_id.to_string(),
            })?;

        conn.call_tool(tool_name, arguments).await
    }

    /// 检查连接是否有效
    pub async fn is_connected(&self, server_id: &str) -> bool {
        if let Some(conn) = self.get_connection(server_id).await {
            conn.is_alive().await
        } else {
            false
        }
    }

    /// 删除服务器（移除配置和连接）
    pub async fn remove_server(&self, server_id: &str) {
        self.remove_connection(server_id).await;
        let mut configs = self.configs.write().await;
        let mut states = self.states.write().await;
        configs.remove(server_id);
        states.remove(server_id);
    }

    /// 关闭所有连接
    pub async fn shutdown(&self) {
        let connections: Vec<_> = {
            let mut conns = self.connections.write().await;
            conns.drain().collect()
        };

        for (id, conn) in connections {
            if let Err(e) = conn.close().await {
                warn!("Error closing connection for server '{}': {}", id, e);
            }
        }

        self.tool_cache.write().await.clear();
        info!("ServerManager shutdown complete");
    }
}

