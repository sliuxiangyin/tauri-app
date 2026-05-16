use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::provider::mcp_v2::config::McpServerConfig;
use crate::provider::mcp_v2::connection::McpConnection;
use crate::provider::mcp_v2::error::{McpManagerError, Result};
use crate::provider::cache::Cache;
use crate::provider::mcp_v2::transport;

/// 缓存条目结构（用于持久化工具清单）
#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    tools: Vec<Tool>,
    #[serde(default)]
    prompts: Vec<Value>,
    updated_at: String,
}

fn cache_key(server_id: &str) -> String {
    format!("mcp_tools_{}", server_id)
}

/// 带来源标识的工具信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolWithSource {
    pub server_id: String,
    pub server_name: String,
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

impl ToolWithSource {
    fn from_tool(server_id: &str, server_name: &str, tool: &Tool) -> Self {
        Self {
            server_id: server_id.to_string(),
            server_name: server_name.to_string(),
            name: tool.name.to_string(),
            description: tool.description.as_deref().map(|d| d.to_string()),
            input_schema: tool.schema_as_json_value(),
        }
    }
}

/// 全局 MCP 服务器管理器（单例模式）
///
/// 负责管理所有 MCP 服务器的连接、工具清单缓存及文件持久化。
/// 设计要点：
/// - 全量初始化：根据配置创建所有连接，但不立即加载工具清单
/// - 懒加载：首次 list_tools 时从 MCP 服务器获取并缓存
/// - 添加/移除/更新操作同步更新全局状态和文件缓存
pub struct ServerManager {
    /// 服务器连接句柄
    connections: RwLock<HashMap<String, Arc<McpConnection>>>,
    /// 工具清单内存缓存
    tool_cache: RwLock<HashMap<String, Vec<Tool>>>,
    /// 文件缓存持久化
    file_cache: Arc<Cache>,
    /// 服务器配置
    configs: RwLock<HashMap<String, McpServerConfig>>,
}

impl ServerManager {
    /// 全量初始化：为所有配置创建连接，但不加载工具清单
    pub async fn new(configs: Vec<McpServerConfig>, file_cache: Arc<Cache>) -> Result<Self> {
        let mut connections = HashMap::new();
        let config_map: HashMap<String, McpServerConfig> = configs
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();

        // 尝试从缓存恢复工具清单，同时建立连接
        let mut tool_cache_map = HashMap::new();
        for (id, config) in &config_map {
            // 尝试从文件缓存加载
            let key = cache_key(id);
            match file_cache.get(&key) {
                Ok(Some(raw)) => {
                    match serde_json::from_slice::<CacheEntry>(&raw) {
                        Ok(entry) => {
                            println!("Loaded {} tools from cache for server '{}'", entry.tools.len(), id);
                            tool_cache_map.insert(id.clone(), entry.tools);
                        }
                        Err(e) => {
                            println!("Failed to deserialize cache for server '{}': {}", id, e);
                        }
                    }
                }
                Ok(None) => {
                    println!("No cache found for server '{}', will lazy-load", id);
                }
                Err(e) => {
                    println!("Failed to load cache for server '{}': {}", id, e);
                }
            }

            // 建立连接
            match transport::build_peer(&config.transport).await {
                Ok(running) => {
                    let conn = Arc::new(McpConnection::new(running));
                    connections.insert(id.clone(), conn);
                    println!("Connection established for server '{}'", id);
                }
                Err(e) => {
                    error!("Failed to connect to server '{}': {}", id, e);
                    // 继续处理其他服务器
                }
            }
        }

        Ok(Self {
            connections: RwLock::new(connections),
            tool_cache: RwLock::new(tool_cache_map),
            file_cache,
            configs: RwLock::new(config_map),
        })
    }

    /// 添加 MCP 服务器：建立连接并立即加载工具清单
    pub async fn add_server(&self, config: McpServerConfig) -> Result<()> {
        let id = config.id.clone();

        // 检查是否已存在
        {
            let configs = self.configs.read().await;
            if configs.contains_key(&id) {
                return Err(McpManagerError::ServerAlreadyExists {
                    server_id: id,
                });
            }
        }

        // 建立连接
        let running = transport::build_peer(&config.transport).await?;
        let conn = Arc::new(McpConnection::new(running));

        // 立即加载工具清单
        let tools = conn.list_tools().await?;
        println!("Loaded {} tools for server '{}'", tools.len(), id);
        // 更新全局状态
        {
            let mut connections = self.connections.write().await;
            let mut tool_cache = self.tool_cache.write().await;
            let mut configs = self.configs.write().await;

            connections.insert(id.clone(), conn);
            tool_cache.insert(id.clone(), tools.clone());
            configs.insert(id.clone(), config);
        }

        // 写入文件缓存
        self.cache_tools_to_disk(&id, &tools);

        println!("Server '{}' added with {} tools", id, tools.len());
        Ok(())
    }

    /// 移除 MCP 服务器：关闭连接并清除缓存
    pub async fn remove_server(&self, id: &str) -> Result<()> {
        // 关闭连接
        if let Some(conn) = self.connections.write().await.remove(id) {
            conn.close().await;
        }

        // 清除内存缓存
        self.tool_cache.write().await.remove(id);
        self.configs.write().await.remove(id);

        // 清除文件缓存
        let cache_key = cache_key(id);
        if let Err(e) = self.file_cache.remove(&cache_key) {
            println!("Failed to invalidate cache for server '{}': {}", id, e);
        }

        println!("Server '{}' removed", id);
        Ok(())
    }

    /// 更新 MCP 服务器（等价于先 remove 再 add）
    pub async fn update_server(&self, id: &str, config: McpServerConfig) -> Result<()> {
        // 检查是否存在
        {
            let configs = self.configs.read().await;
            if !configs.contains_key(id) {
                return Err(McpManagerError::ServerNotFound {
                    server_id: id.to_string(),
                });
            }
        }

        // 先移除
        self.remove_server(id).await?;

        // 再添加（新配置的 id 可能与旧的不同）
        self.add_server(config).await?;

        println!("Server '{}' updated", id);
        Ok(())
    }

    /// 获取工具列表（支持懒加载和按服务器过滤）
    ///
    /// - `server_id` 为 `Some(s)` 时，仅返回指定服务器的工具
    /// - `server_id` 为 `None` 时，返回所有已连接服务器的工具（带来源标识）
    pub async fn list_tools(&self, server_id: Option<&str>) -> Result<Vec<ToolWithSource>> {
        match server_id {
            Some(id) => {
                // 单服务器：先检查内存缓存，没有则懒加载
                let tools = self.get_or_load_tools(id).await?;
                let configs = self.configs.read().await;
                let name = configs
                    .get(id)
                    .map(|c| c.name.as_str())
                    .unwrap_or(id);
                Ok(tools
                    .iter()
                    .map(|t| ToolWithSource::from_tool(id, name, t))
                    .collect())
            }
            None => {
                // 所有服务器
                let ids: Vec<String> = self.connections.read().await.keys().cloned().collect();
                let mut all_tools = Vec::new();

                for id in ids {
                    match self.get_or_load_tools(&id).await {
                        Ok(tools) => {
                            let configs = self.configs.read().await;
                            let name = configs
                                .get(&id)
                                .map(|c| c.name.as_str())
                                .unwrap_or(&id);
                            let mapped: Vec<ToolWithSource> = tools
                                .iter()
                                .map(|t| ToolWithSource::from_tool(&id, name, t))
                                .collect();
                            all_tools.extend(mapped);
                        }
                        Err(e) => {
                            println!("Failed to list tools for server '{}': {}", id, e);
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
        let conn = connections.get(server_id).ok_or_else(|| {
            McpManagerError::ServerNotFound {
                server_id: server_id.to_string(),
            }
        })?;

        conn.call_tool(tool_name, arguments).await
    }

    /// 获取所有已连接服务器的信息
    pub async fn list_servers(&self) -> Vec<McpServerConfig> {
        self.configs.read().await.values().cloned().collect()
    }

    /// 检查指定服务器是否已连接
    pub async fn is_connected(&self, server_id: &str) -> bool {
        self.connections.read().await.contains_key(server_id)
    }

    /// 刷新指定服务器的工具缓存（强制从 MCP 服务器重新加载）
    pub async fn refresh_tools(&self, server_id: &str) -> Result<Vec<ToolWithSource>> {
        let connections = self.connections.read().await;
        let conn = connections.get(server_id).ok_or_else(|| {
            McpManagerError::ServerNotFound {
                server_id: server_id.to_string(),
            }
        })?;

        let tools = conn.refresh_tools().await?;

        // 更新内存缓存
        {
            let mut cache = self.tool_cache.write().await;
            cache.insert(server_id.to_string(), tools.clone());
        }

        // 更新文件缓存
        self.cache_tools_to_disk(server_id, &tools);

        let configs = self.configs.read().await;
        let name = configs
            .get(server_id)
            .map(|c| c.name.as_str())
            .unwrap_or(server_id);

        Ok(tools
            .iter()
            .map(|t| ToolWithSource::from_tool(server_id, name, t))
            .collect())
    }

    /// 后台定期刷新所有服务器的工具缓存
    pub async fn refresh_all(&self) {
        let ids: Vec<String> = self.connections.read().await.keys().cloned().collect();
        for id in ids {
            if let Err(e) = self.refresh_tools(&id).await {
                println!("Failed to refresh tools for server '{}': {}", id, e);
            }
        }
    }

    /// 优雅关闭所有连接
    pub async fn shutdown(&self) {
        let mut connections = self.connections.write().await;
        for (id, conn) in connections.drain() {
            conn.close().await;
            println!("Shutdown connection for server '{}'", id);
        }
        self.tool_cache.write().await.clear();
        println!("ServerManager shutdown complete");
    }

    // ========== 内部辅助方法 ==========

    /// 保存工具清单到文件缓存
    fn cache_tools_to_disk(&self, server_id: &str, tools: &[Tool]) {
        let key = cache_key(server_id);
        match serde_json::to_vec(&CacheEntry {
            tools: tools.to_vec(),
            prompts: vec![],
            updated_at: chrono::Utc::now().to_rfc3339(),
        }) {
            Ok(json) => {
                if let Err(e) = self.file_cache.put(&key, json) {
                    println!("Failed to cache tools for server '{}': {}", server_id, e);
                }
            }
            Err(e) => {
                println!("Failed to serialize cache for server '{}': {}", server_id, e);
            }
        }
    }

    /// 获取或懒加载指定服务器的工具清单
    async fn get_or_load_tools(&self, server_id: &str) -> Result<Vec<Tool>> {
        // 先检查内存缓存
        {
            let cache = self.tool_cache.read().await;
            if let Some(tools) = cache.get(server_id) {
                return Ok(tools.clone());
            }
        }

        // 懒加载：从 MCP 服务器获取
        let connections = self.connections.read().await;
        let conn = connections.get(server_id).ok_or_else(|| {
            McpManagerError::ServerNotFound {
                server_id: server_id.to_string(),
            }
        })?;

        let tools = conn.list_tools().await?;

        // 更新内存缓存和文件缓存
        {
            let mut cache = self.tool_cache.write().await;
            cache.insert(server_id.to_string(), tools.clone());
        }

        self.cache_tools_to_disk(server_id, &tools);

        println!(
            "Lazy-loaded {} tools for server '{}'",
            tools.len(),
            server_id
        );
        Ok(tools)
    }
}
