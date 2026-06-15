//! 工具执行器模块
//!
//! 提供各种工具执行器实现，支持扩展多种工具源（MCP、Skills 等）。

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
use crate::provider::llm::ordinary::parse_tool_arguments;
use crate::provider::llm::tools::parse_mcp_tool_name;
use crate::provider::llm::types::FunctionCall;
use crate::services::traits::McpClient;
use rmcp::model::CallToolRequestParams;

/// MCP 工具执行器
///
/// 使用 McpClient Trait 执行 MCP 协议工具调用。
pub struct McpToolExecutor {
    mcp_client: Arc<dyn McpClient>,
}

impl McpToolExecutor {
    /// 创建新的 MCP 工具执行器
    pub fn new(mcp_client: Arc<dyn McpClient>) -> Self {
        Self { mcp_client }
    }
}

#[async_trait]
impl ToolExecutor for McpToolExecutor {
    async fn execute_tool(&self, call: FunctionCall) -> Result<Value, ToolExecError> {
        // 解析 MCP 工具名称（标准格式: "mcp__server__tool_name"）
        let (server_name, tool_name) = parse_mcp_tool_name(&call.name)
            .ok_or_else(|| ToolExecError {
                name: call.name.clone(),
                message: format!(
                    "Invalid tool name format: expected 'mcp__server__tool', got '{}'",
                    call.name
                ),
            })?;

        tracing::debug!(
            "[McpToolExecutor] executing: server={}, tool={}, arguments={:?}",
            server_name,
            tool_name,
            call.arguments
        );

        // 构建 CallToolRequestParams
        let args_map = parse_tool_arguments(call.arguments);

        // 使用 'static 生命周期来处理工具名
        let tool_name_static: &'static str = Box::leak(tool_name.to_string().into_boxed_str());
        let params = CallToolRequestParams::new(tool_name_static).with_arguments(args_map);

        // 执行工具调用
        let result = self
            .mcp_client
            .call_tool(server_name, params)
            .await
            .map_err(|e| ToolExecError {
                name: call.name.clone(),
                message: e.to_string(),
            })?;

        // 转换为 JSON Value
        Ok(serde_json::to_value(&result.content).unwrap_or(Value::Null))
    }
}

/// 执行 MCP 工具调用（便捷函数）
///
/// 解析工具名并调用 MCP Client。
#[allow(dead_code)]
pub async fn execute_mcp_tool(
    mcp_client: Arc<dyn McpClient>,
    call: FunctionCall,
) -> Result<Value, ToolExecError> {
    let executor = McpToolExecutor::new(mcp_client);
    executor.execute_tool(call).await
}
