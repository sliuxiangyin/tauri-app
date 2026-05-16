use serde_json::Value;
use std::sync::Arc;

use crate::provider::mcp_v2::config::McpServerConfig;
use crate::provider::mcp_v2::error::Result;
use crate::provider::mcp_v2::server_manager::{ServerManager, ToolWithSource};

/// mcp-v2 对外统一 API
///
/// 封装 [ServerManager] 的所有公开方法，提供更简洁的调用接口。
/// 通过 `Arc<ServerManager>` 共享，可安全地在多任务间并发调用。
pub struct McpV2Api {
    manager: Arc<ServerManager>,
}

impl McpV2Api {
    /// 创建 API 实例
    pub fn new(manager: Arc<ServerManager>) -> Self {
        Self { manager }
    }

    /// 获取内部 ServerManager 引用
    pub fn manager(&self) -> &Arc<ServerManager> {
        &self.manager
    }

    /// 添加 MCP 服务器
    pub async fn add_server(&self, config: McpServerConfig) -> Result<()> {
        self.manager.add_server(config).await
    }

    /// 移除 MCP 服务器
    pub async fn remove_server(&self, id: &str) -> Result<()> {
        self.manager.remove_server(id).await
    }

    /// 更新 MCP 服务器
    pub async fn update_server(&self, id: &str, config: McpServerConfig) -> Result<()> {
        self.manager.update_server(id, config).await
    }

    /// 获取工具列表
    ///
    /// - `server_id` 为 `Some(s)` 时，仅返回指定服务器的工具
    /// - `server_id` 为 `None` 时，返回所有已连接服务器的工具
    pub async fn list_tools(&self, server_id: Option<&str>) -> Result<Vec<ToolWithSource>> {
        self.manager.list_tools(server_id).await
    }

    /// 调用工具
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value> {
        self.manager.call_tool(server_id, tool_name, arguments).await
    }

    /// 列出所有已连接的服务器配置
    pub async fn list_servers(&self) -> Vec<McpServerConfig> {
        self.manager.list_servers().await
    }

    /// 检查服务器是否已连接
    pub async fn is_connected(&self, server_id: &str) -> bool {
        self.manager.is_connected(server_id).await
    }

    /// 刷新指定服务器的工具缓存
    pub async fn refresh_tools(&self, server_id: &str) -> Result<Vec<ToolWithSource>> {
        self.manager.refresh_tools(server_id).await
    }

    /// 刷新所有服务器的工具缓存
    pub async fn refresh_all(&self) {
        self.manager.refresh_all().await;
    }

  

    /// 优雅关闭
    pub async fn shutdown(&self) {
        self.manager.shutdown().await;
    }
}
