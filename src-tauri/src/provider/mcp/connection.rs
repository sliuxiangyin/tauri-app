use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::service::{Peer, RoleClient, RunningService};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::provider::mcp::error::{McpError, Result};

/// 管理与单个 MCP 服务器的连接
pub struct McpConnection {
    /// 服务器 ID
    server_id: String,
    /// MCP Peer 句柄
    peer: Peer<RoleClient>,
    /// RunningService 句柄，用于关闭连接
    running: RwLock<Option<RunningService<RoleClient, ()>>>,
    /// 进程 ID（仅 STDIO 类型）
    process_id: RwLock<Option<u32>>,
    /// 工具缓存，避免重复请求
    cached_tools: RwLock<Option<Vec<Tool>>>,
}

impl McpConnection {
    /// 从 RunningService 创建连接管理器
    pub fn new(server_id: String, running: RunningService<RoleClient, ()>, process_id: Option<u32>) -> Self {
        let peer = running.peer().clone();
        Self {
            server_id,
            peer,
            running: RwLock::new(Some(running)),
            process_id: RwLock::new(process_id),
            cached_tools: RwLock::new(None),
        }
    }

    /// 获取服务器 ID
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// 获取进程 ID（仅 STDIO 类型）
    pub async fn process_id(&self) -> Option<u32> {
        *self.process_id.read().await
    }

    /// 获取工具列表，优先使用缓存
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        // 先检查缓存
        {
            let cached = self.cached_tools.read().await;
            if let Some(ref tools) = *cached {
                debug!("Returning {} cached tools for server '{}'", tools.len(), self.server_id);
                return Ok(tools.clone());
            }
        }

        // 从 MCP 服务器获取
        let tools = self
            .peer
            .list_all_tools()
            .await
            .map_err(|e| McpError::ConnectionFailed {
                message: format!("Failed to list tools: {}", e),
            })?;

        // 更新缓存
        let mut cached = self.cached_tools.write().await;
        *cached = Some(tools.clone());
        debug!("Fetched and cached {} tools for server '{}'", tools.len(), self.server_id);
        
        Ok(tools)
    }

    /// 刷新工具列表（清除缓存后重新获取）
    pub async fn refresh_tools(&self) -> Result<Vec<Tool>> {
        // 清除缓存
        *self.cached_tools.write().await = None;
        
        // 重新获取
        self.list_tools().await
    }

    /// 清除工具缓存
    pub async fn clear_cache(&self) {
        *self.cached_tools.write().await = None;
        debug!("Tool cache cleared for server '{}'", self.server_id);
    }

    /// 调用工具
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        let params = match arguments {
            Value::Object(map) => {
                CallToolRequestParams::new(tool_name.to_string()).with_arguments(map)
            }
            _ => CallToolRequestParams::new(tool_name.to_string()),
        };

        let result = self
            .peer
            .call_tool(params)
            .await
            .map_err(|e| McpError::ToolCallFailed {
                message: format!("Failed to call tool '{}': {}", tool_name, e),
            })?;

        // 将 CallToolResult 序列化为 serde_json::Value 返回
        let value = serde_json::to_value(&result).map_err(McpError::SerializationError)?;
        Ok(value)
    }

    /// 检查连接是否仍然有效（通过发送 ping 或调用 list_tools）
    pub async fn is_alive(&self) -> bool {
        // 尝试调用 list_tools 来检测连接
        // 如果连接已断开，这里会返回错误
        match self.list_tools().await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// 检查连接是否已关闭
    pub async fn is_closed(&self) -> bool {
        let running = self.running.read().await;
        if let Some(ref service) = *running {
            service.is_closed()
        } else {
            true
        }
    }

    /// 关闭连接（优雅关闭）
    pub async fn close(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if let Some(mut service) = running.take() {
            match service.close().await {
                Ok(reason) => {
                    info!("MCP connection closed for server '{}': {:?}", self.server_id, reason);
                }
                Err(e) => {
                    warn!("Error closing MCP connection for server '{}': {}", self.server_id, e);
                }
            }
        }
        // 清除工具缓存
        *self.cached_tools.write().await = None;
        Ok(())
    }

    /// 强制关闭连接（带超时）
    pub async fn close_with_timeout(&self, timeout: std::time::Duration) -> Result<()> {
        let mut running = self.running.write().await;
        if let Some(mut service) = running.take() {
            match service.close_with_timeout(timeout).await {
                Ok(Some(reason)) => {
                    info!("MCP connection closed for server '{}': {:?}", self.server_id, reason);
                }
                Ok(None) => {
                    warn!("MCP connection close timed out for server '{}'", self.server_id);
                }
                Err(e) => {
                    warn!("Error closing MCP connection for server '{}': {}", self.server_id, e);
                }
            }
        }
        *self.cached_tools.write().await = None;
        Ok(())
    }
}

impl Drop for McpConnection {
    fn drop(&mut self) {
        info!("McpConnection for server '{}' dropped", self.server_id);
    }
}