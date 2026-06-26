//! React Agent - Trait 契约与默认桩实现
//!
//! 本期只放 Trait 契约和桩实现，**真正的 Thought→Action→Observe 循环
//! 留给后续 PR**。TaskPipelineExecutor 可在 `with_react_agent()` 注入
//! 自定义实现，或使用默认桩（返回 `LlmError::NotImplemented`）。
//!
//! ## 使用方式
//!
//! ```ignore
//! use std::sync::Arc;
//! use crate::provider::llm::planner::react_agent::{ReactAgent, DefaultReactAgent};
//!
//! let executor = TaskPipelineExecutor::new(provider)
//!     .with_react_agent(Arc::new(DefaultReactAgent));
//! ```

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};

use crate::provider::llm::error::LlmError;
use crate::provider::llm::planner::execution_planner_agent::types::ExecutionStep;

pub mod types;
use self::types::{StepContext, StepExecutionResult};

/// React Agent Trait
///
/// 负责执行单个 ExecutionStep，内部完成 Thought→Action→Observe 循环。
/// 一次 Step 可能对应多次工具调用，最终输出结构化结果。
///
/// ## 职责边界
///
/// - **不感知** 上层 TaskPlan / Stage DAG（上游调度由 TaskPipelineExecutor 负责）
/// - **不感知** Tauri / 事件总线（进度事件由 executor 负责 emit）
/// - **不感知** Tauri AppHandle（与 Tauri 完全解耦）
///
/// 唯一职责：给定 ExecutionStep + 上下文，返回执行结果。
#[async_trait]
pub trait ReactAgent: Send + Sync {
    /// Step 输出的结构化类型
    ///
    /// 多数场景下用 `serde_json::Value` 即可（与 stage.outputs 规约匹配）。
    /// 强类型场景可自定义（如 `BrowserStepOutput`）。
    type Output: Serialize + DeserializeOwned + Send + 'static;

    /// 执行单个 ExecutionStep
    ///
    /// - `step.goal` / `step.expected_tool` 决定执行方向
    /// - `context` 提供 inputs、available_tools、previous_step_outputs
    /// - 返回 `StepExecutionResult`（output + tool_calls 历史 + 最后 thought）
    async fn run_step(
        &self,
        step: &ExecutionStep,
        context: &StepContext,
    ) -> Result<StepExecutionResult<Self::Output>, LlmError>;
}

/// 默认桩实现
///
/// 任何 `run_step` 调用都返回 `LlmError::NotImplemented`。
/// 用于：
/// - pipeline_executor 单元测试中验证 TaskPlanner → ExecutionPlanner 串联
/// - 真实 React Agent 实现前占位
///
/// 真实实现可通过 `TaskPipelineExecutor::with_react_agent()` 注入。
pub struct DefaultReactAgent;

#[async_trait]
impl ReactAgent for DefaultReactAgent {
    type Output = serde_json::Value;

    async fn run_step(
        &self,
        _step: &ExecutionStep,
        _context: &StepContext,
    ) -> Result<StepExecutionResult<Self::Output>, LlmError> {
        Err(LlmError::NotImplemented(
            "ReactAgent not implemented yet — use TaskPipelineExecutor::with_react_agent() to inject a real implementation".into(),
        ))
    }
}
