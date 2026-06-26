//! React Agent - 类型定义
//!
//! 定义 StepContext 和 StepExecutionResult，作为 React Agent 与
//! TaskPipelineExecutor 之间的契约类型。

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::provider::llm::planner::task_planner_agent::types::OutputSpec;
use crate::provider::llm::types::ToolDefinition;

/// 单个 Step 执行时 React Agent 可用的上下文。
///
/// 由 `TaskPipelineExecutor::execute_stage` 在调用 `ReactAgent::run_step` 之前构造，
/// 包含完成当前 Step 所需的全部信息。
#[derive(Debug, Clone)]
pub struct StepContext {
    /// 当前 Stage 的业务目标
    pub stage_goal: String,
    /// 当前 Stage 的领域
    pub stage_domain: String,
    /// 已解析的 inputs：字面量 + FromStage 注入实际值
    pub stage_inputs: HashMap<String, serde_json::Value>,
    /// 当前 Stage 期望产出规约（key → OutputSpec）
    pub stage_outputs_spec: BTreeMap<String, OutputSpec>,
    /// 运行环境信息（由调用方注入）
    pub runtime_context: String,
    /// 完整工具列表（React Agent 可自由选择，不仅限于 `expected_tool`）
    pub available_tools: Vec<ToolDefinition>,
    /// 前序 Step 实际输出（order → value）
    pub previous_step_outputs: HashMap<u32, serde_json::Value>,
}

/// React Agent 执行单个 Step 的返回结果。
///
/// 一次 Step 可能对应多次工具调用（Thought→Action→Observe 循环），
/// 全部历史记录在 `tool_calls` 中。
#[derive(Debug, Clone)]
pub struct StepExecutionResult<T> {
    /// 当前 Step 的产出（按 stage.outputs 的 key 约定类型）
    pub output: T,
    /// 完整的工具调用历史（含所有重试）
    pub tool_calls: Vec<ToolCallRecord>,
    /// 最后一次 Thought（用于可观测性 / 调试）
    pub thought: Option<String>,
}

/// 单次工具调用记录。
///
/// 由 React Agent 内部维护，最终汇总到 StepExecutionRecord 中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// 工具名称（如 `mcp__browser__click`）
    pub tool_name: String,
    /// 工具参数
    pub arguments: serde_json::Value,
    /// 工具执行结果
    pub result: serde_json::Value,
    /// 是否成功
    pub success: bool,
    /// 时间戳（毫秒）
    pub timestamp_ms: u64,
}
