//! 工具执行器 Trait 定义层
//!
//! 统一 LLM 工具执行的抽象接口，供 ordinary 和 agent 两种模式共用。
//!
//! ## 类型说明
//! - `ToolExecutor`：工具执行器 trait
//! - `ToolExecError`：工具执行错误
//! - `FnToolExecutor`：闭包适配器（适合简单场景）

use std::future::Future;

use async_trait::async_trait;
use serde_json::Value;

use crate::provider::llm::types::FunctionCall;

/// 工具执行器 trait
///
/// 由外部（如 MCP Manager、Skills Manager）实现，负责执行具体的工具调用。
/// 支持多种工具源（MCP、Skills 等）。
///
/// # 使用方式
/// 1. 实现 trait（适合复杂逻辑或需要依赖注入）
/// 2. 使用 `FnToolExecutor` 闭包适配器（推荐，适合简单场景）
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 执行工具调用
    ///
    /// # 参数
    /// - `call`: 函数调用信息（包含 id、name、arguments）
    ///
    /// # 返回
    /// 执行结果（JSON Value）。
    async fn execute_tool(
        &self,
        call: FunctionCall,
    ) -> Result<Value, ToolExecError>;
}

/// 工具执行错误
#[derive(Debug, Clone)]
pub struct ToolExecError {
    pub name: String,
    pub message: String,
}

impl std::fmt::Display for ToolExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tool '{}' execution failed: {}", self.name, self.message)
    }
}

impl std::error::Error for ToolExecError {}

/// 闭包适配器：允许使用 async fn 闭包作为工具执行器
///
/// 使用示例：
/// ```ignore
/// let executor = FnToolExecutor::new(|call| async move {
///     let (server, tool) = parse_mcp_tool_name(&call.name).unwrap();
///     let params = CallToolRequestParams::new(call.name.clone(), call.arguments);
///     let result = mcp_manager.call_tool(server, params).await?;
///     Ok(result.content.into())
/// });
/// ```
pub struct FnToolExecutor<F> {
    executor: F,
}

impl<F> FnToolExecutor<F> {
    /// 创建新的闭包执行器
    pub fn new(executor: F) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<F, Fut> ToolExecutor for FnToolExecutor<F>
where
    F: Fn(FunctionCall) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ToolExecError>> + Send + 'static,
{
    async fn execute_tool(&self, call: FunctionCall) -> Result<Value, ToolExecError> {
        (self.executor)(call).await
    }
}
