//! Agent 专用流式事件
//!
//! 扩展 LLM 流式事件，增加 Agent 循环状态信息。

use serde::{Deserialize, Serialize};

use crate::provider::llm::llm_event::LlmStreamEvent;

/// Agent 执行终止原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum StopReason {
    /// 正常完成（LLM 返回完成标记）
    Normal,
    /// 达到最大迭代次数
    MaxStepsReached,
    /// 达到总超时时间
    TimeoutReached,
    /// 空响应超过阈值
    EmptyResponseThreshold,
    /// 连续错误超过阈值
    ErrorThreshold,
    /// 用户中止
    UserAbort,
    /// LLM 返回无效响应
    InvalidResponse,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopReason::Normal => write!(f, "normal"),
            StopReason::MaxStepsReached => write!(f, "max_steps_reached"),
            StopReason::TimeoutReached => write!(f, "timeout_reached"),
            StopReason::EmptyResponseThreshold => write!(f, "empty_response_threshold"),
            StopReason::ErrorThreshold => write!(f, "error_threshold"),
            StopReason::UserAbort => write!(f, "user_abort"),
            StopReason::InvalidResponse => write!(f, "invalid_response"),
        }
    }
}

/// Agent 流式事件
///
/// 包含：
/// - LLM 层事件（透传）
/// - Agent 状态事件（新增）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentStreamEvent {
    // ========== LLM 事件（透传）==========
    /// LLM 流式事件透传
    Llm(LlmStreamEvent),

    // ========== Agent 状态事件 ==========
    /// Agent 开始执行
    AgentStart {
        /// 当前步数（从 1 开始）
        step: u32,
    },
    /// 步骤开始
    StepStart {
        /// 当前步数
        step: u32,
    },
    /// 步骤完成
    StepComplete {
        /// 当前步数
        step: u32,
        /// 是否执行了工具调用
        had_tool_call: bool,
        /// 工具调用数量
        tool_call_count: u32,
    },
    /// 工具开始执行
    ToolStart {
        /// 工具调用 ID
        call_id: String,
        /// 工具名称
        name: String,
        /// 工具参数（JSON 字符串，用于调试）
        arguments: Option<String>,
    },
    /// 工具执行完成
    ToolComplete {
        /// 工具调用 ID
        call_id: String,
        /// 工具名称
        name: String,
        /// 执行耗时（毫秒）
        duration_ms: u64,
        /// 是否成功
        success: bool,
    },
    /// 工具执行失败
    ToolError {
        /// 工具调用 ID
        call_id: String,
        /// 工具名称
        name: String,
        /// 错误信息
        error: String,
    },
    /// Agent 执行完成
    AgentComplete {
        /// 总步数
        total_steps: u32,
        /// 终止原因
        stop_reason: StopReason,
        /// 最终消息内容（摘要）
        final_content: Option<String>,
    },
    /// 进度报告（用于长时操作）
    Progress {
        /// 当前步骤
        step: u32,
        /// 最大步骤
        max_steps: u32,
        /// 进度描述
        message: String,
    },
}

impl AgentStreamEvent {
    /// 创建 LLM 事件透传
    pub fn llm(event: LlmStreamEvent) -> Self {
        Self::Llm(event)
    }

    /// 创建 Agent 开始事件
    pub fn agent_start(step: u32) -> Self {
        Self::AgentStart { step }
    }

    /// 创建步骤开始事件
    pub fn step_start(step: u32) -> Self {
        Self::StepStart { step }
    }

    /// 创建步骤完成事件
    pub fn step_complete(step: u32, had_tool_call: bool, tool_call_count: u32) -> Self {
        Self::StepComplete {
            step,
            had_tool_call,
            tool_call_count,
        }
    }

    /// 创建工具开始事件
    pub fn tool_start(call_id: impl Into<String>, name: impl Into<String>, arguments: Option<String>) -> Self {
        Self::ToolStart {
            call_id: call_id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// 创建工具完成事件
    pub fn tool_complete(call_id: impl Into<String>, name: impl Into<String>, duration_ms: u64, success: bool) -> Self {
        Self::ToolComplete {
            call_id: call_id.into(),
            name: name.into(),
            duration_ms,
            success,
        }
    }

    /// 创建工具错误事件
    pub fn tool_error(call_id: impl Into<String>, name: impl Into<String>, error: impl Into<String>) -> Self {
        Self::ToolError {
            call_id: call_id.into(),
            name: name.into(),
            error: error.into(),
        }
    }

    /// 创建 Agent 完成事件
    pub fn agent_complete(total_steps: u32, stop_reason: StopReason, final_content: Option<String>) -> Self {
        Self::AgentComplete {
            total_steps,
            stop_reason,
            final_content,
        }
    }

    /// 创建进度事件
    pub fn progress(step: u32, max_steps: u32, message: impl Into<String>) -> Self {
        Self::Progress {
            step,
            max_steps,
            message: message.into(),
        }
    }
}

/// Agent 执行结果摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct AgentResultSummary {
    /// 总执行步数
    pub total_steps: u32,
    /// 终止原因
    pub stop_reason: StopReason,
    /// 工具调用总数
    pub total_tool_calls: u32,
    /// 成功的工具调用数
    pub successful_tool_calls: u32,
    /// 失败的工具调用数
    pub failed_tool_calls: u32,
    /// 总执行时间（毫秒）
    pub total_duration_ms: u64,
    /// 最终内容摘要
    pub final_content: Option<String>,
    /// 错误信息（如果有）
    pub error: Option<String>,
}