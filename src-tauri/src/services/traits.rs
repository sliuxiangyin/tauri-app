//! Trait 接口定义 - 依赖注入核心
//!
//! 定义 Services 层使用的核心 Trait，实现解耦。
//! 所有 Service 结构体应通过这些 Trait 接口访问底层组件。

use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use sea_orm::DatabaseConnection;
use std::sync::Arc;

pub use crate::db::DbError;

/// 数据库访问接口
///
/// 抽象数据库连接获取，使 Services 层可测试化。
/// 实现者：`db::DbState`
#[async_trait]
pub trait DbAccessor: Send + Sync {
    async fn get(&self) -> Result<Arc<DatabaseConnection>, DbError>;
}

/// MCP 客户端接口
///
/// 抽象 MCP 工具调用，使 Services 层不依赖 McpManager 具体实现。
/// 实现者：`provider::mcp::McpManager`
#[async_trait]
pub trait McpClient: Send + Sync {
    /// 调用 MCP 工具
    async fn call_tool(
        &self,
        name: &str,
        params: CallToolRequestParams,
    ) -> Result<CallToolResult, crate::provider::mcp::McpError>;

    /// 获取 MCP 工具列表
    async fn get_tools(&self, name: &str) -> Result<Vec<Tool>, crate::provider::mcp::McpError>;

    /// 获取连接状态
    fn get_status(&self, name: &str) -> Option<crate::provider::mcp::McpStatus>;

    /// 获取所有连接状态
    fn list_all_status(&self) -> Vec<crate::provider::mcp::McpStatus>;

    /// 获取工具总数
    fn get_tools_count(&self) -> usize;
}