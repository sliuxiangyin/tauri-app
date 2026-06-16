//! 计划执行器数据类型定义
//!
//! 集中存放 `PlanExecutor` 相关的所有数据类型：
//! - [`PlanResult`]：整个计划的执行结果
//! - [`StepResult`]：单个步骤的执行结果
//! - [`PlanStopReason`]：计划停止原因枚举
//! - [`PlanError`]：计划执行错误枚举
//!
//! 所有类型通过 `mod.rs::pub(crate) use types::*;` 重新导出，
//! 外部可通过 `crate::provider::llm::agent::plan_executor::XXX` 访问。

use std::fmt;

use crate::provider::llm::llm_tool_trait::ToolExecError;
use crate::provider::llm::types::PlanStep;

// =============================================================================
// 执行结果包装类型
// =============================================================================

/// 步骤执行结果（包含执行耗时）
///
/// 由各步骤执行器返回，供 Observe 阶段和事件发射使用。
pub struct StepExecResult {
    /// 执行后的 PlanStep（包含输出）
    pub step: PlanStep,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
}

// =============================================================================
// 结果类型
// =============================================================================

/// 计划执行结果
#[derive(Debug, Clone)]
pub struct PlanResult {
    /// 成功执行的步骤数
    pub completed_steps: u8,
    /// 总步骤数
    pub total_steps: u8,
    /// 最终回复内容
    pub final_reply: String,
    /// 各步骤执行摘要
    pub step_results: Vec<StepResult>,
    /// 停止原因
    pub stop_reason: PlanStopReason,
}

/// 单个步骤执行结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepResult {
    /// 步骤序号
    pub order: u8,
    /// 工具名称
    pub tool_name: String,
    /// 是否成功
    pub success: bool,
    /// 执行输出
    pub output: String,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
}

// =============================================================================
// Observation / Replanning / Failure 类型
// =============================================================================

/// Observe 阶段产生的下一步决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserveDecision {
    /// 继续下一步
    ContinueNext,
    /// 重试当前步骤（临时失败，重试次数未耗尽）
    RetryCurrent,
    /// 跳过当前步骤（非关键步骤失败，graceful degradation）
    SkipStep { reason: String },
    /// 需要重新规划剩余步骤
    ReplanRequired { reason: String },
    /// 任务已提前完成（所有目标达成）
    TaskComplete,
}

/// 步骤执行后的观察结论
#[derive(Debug, Clone)]
pub struct StepObservation {
    /// 被观察的步骤序号
    pub step_order: u8,
    /// 步骤是否成功执行
    pub success: bool,
    /// 观察结论摘要
    pub summary: String,
    /// 下一步决策
    pub decision: ObserveDecision,
}

/// 步骤失败分类
#[derive(Debug, Clone)]
pub enum StepFailure {
    /// 临时失败（网络超时、临时性错误），可重试
    Temporary { reason: String, retry_count: u8 },
    /// 永久失败（工具不存在、参数错误），不可重试
    Permanent { reason: String },
}

// =============================================================================
// 枚举类型
// =============================================================================

/// 计划停止原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStopReason {
    /// 正常完成
    Completed,
    /// 所有步骤都完成但没有生成回复
    NoFinalReply,
    /// 执行失败（部分步骤失败）
    PartialFailure,
    /// 用户中止
    UserAbort,
    /// 步骤依赖检查失败
    DependencyFailed,
    /// 工具不存在
    ToolNotFound,
    /// 达到最大重试次数
    MaxRetriesExceeded,
}

/// 计划执行错误
#[derive(Debug)]
pub enum PlanError {
    /// 工具执行错误
    ToolError(ToolExecError),
    /// 依赖的前置步骤不存在
    DependencyNotFound(u8),
    /// 工具未找到
    ToolNotFound(String),
    /// 执行被中止
    Aborted,
}

// =============================================================================
// Trait 实现
// =============================================================================

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::ToolError(e) => write!(f, "Tool error: {}", e),
            PlanError::DependencyNotFound(step) => {
                write!(f, "Dependency step {} not found", step)
            }
            PlanError::ToolNotFound(name) => write!(f, "Tool not found: {}", name),
            PlanError::Aborted => write!(f, "Execution aborted by user"),
        }
    }
}

impl std::error::Error for PlanError {}

impl From<ToolExecError> for PlanError {
    fn from(e: ToolExecError) -> Self {
        PlanError::ToolError(e)
    }
}
