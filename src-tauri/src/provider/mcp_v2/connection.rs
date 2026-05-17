use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::service::{Peer, RoleClient, RunningService};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::provider::mcp_v2::error::{McpManagerError, Result};

/// 管理与单个 MCP 服务器的连接及工具缓存
pub struct McpConnection {
    /// MCP Peer 句柄（可克隆，无需锁即可调用方法）
    peer: Peer<RoleClient>,
    /// RunningService 句柄，用于优雅关闭（需要 &mut self）
    running: RwLock<Option<RunningService<RoleClient, ()>>>,
    /// 内层缓存，避免重复请求 MCP 服务器
    cached_tools: RwLock<Option<Vec<Tool>>>,
}

impl McpConnection {
    /// 从已建立的 RunningService 创建连接管理器
    pub fn new(running: RunningService<RoleClient, ()>) -> Self {
        let peer = running.peer().clone();
        Self {
            peer,
            running: RwLock::new(Some(running)),
            cached_tools: RwLock::new(None),
        }
    }

    /// 获取工具列表，优先使用内层缓存
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        {
            let cached = self.cached_tools.read().await;
            if let Some(ref tools) = *cached {
                debug!("Returning {} cached tools", tools.len());
                return Ok(tools.clone());
            }
        }

        let tools = self
            .peer
            .list_all_tools()
            .await
            .map_err(|e| McpManagerError::McpError(format!("Failed to list tools: {}", e)))?;

        let mut cached = self.cached_tools.write().await;
        *cached = Some(tools.clone());
        debug!("Fetched and cached {} tools", tools.len());
        Ok(tools)
    }

    /// 强制刷新工具列表（绕过缓存）
    pub async fn refresh_tools(&self) -> Result<Vec<Tool>> {
        let tools =
            self.peer.list_all_tools().await.map_err(|e| {
                McpManagerError::McpError(format!("Failed to refresh tools: {}", e))
            })?;

        let mut cached = self.cached_tools.write().await;
        *cached = Some(tools.clone());
        debug!("Refreshed and cached {} tools", tools.len());
        Ok(tools)
    }

    /// 调用工具
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        let params = match arguments {
            Value::Object(map) => {
                CallToolRequestParams::new(tool_name.to_string()).with_arguments(map)
            }
            _ => CallToolRequestParams::new(tool_name.to_string()),
        };
        let result =
            self.peer
                .call_tool(params)
                .await
                .map_err(|e| McpManagerError::ToolCallFailed {
                    message: format!("Failed to call tool '{}': {}", tool_name, e),
                })?;

        // 将 CallToolResult 序列化为 serde_json::Value 返回
        let value =
            serde_json::to_value(&result).map_err(|e| McpManagerError::SerializationError(e))?;
        Ok(value)
    }

    /// 清除内层工具缓存
    pub async fn clear_cache(&self) {
        *self.cached_tools.write().await = None;
        debug!("Tool cache cleared");
    }

    /// 关闭连接
    pub async fn close(&self) {
        if let Some(mut running) = self.running.write().await.take() {
            match running.close().await {
                Ok(reason) => info!("MCP connection closed: {:?}", reason),
                Err(e) => warn!("Error closing MCP connection: {}", e),
            }
        }
    }
}
