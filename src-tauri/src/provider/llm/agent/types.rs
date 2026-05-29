//! Agent 模块类型定义
//!
//! 包含 PlanExecutor 混合模式相关的类型

use serde::{Deserialize, Serialize};

/// 步骤类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    /// 确定性步骤：工具和参数在计划阶段已知，可直接执行
    Deterministic,
    /// 探索性步骤：需要在执行时根据上下文决定工具和参数
    Exploratory,
}

impl Default for StepType {
    fn default() -> Self {
        StepType::Deterministic
    }
}

/// 步骤执行动作（LLM 决策后执行的动作）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepAction {
    /// 重试当前步骤
    Retry {
        /// 新的参数（可选，如果不提供则使用原参数）
        new_parameters: Option<serde_json::Value>,
    },
    /// 跳过当前步骤，继续下一步
    Skip {
        /// 跳过原因
        reason: String,
    },
    /// 更换工具执行
    ChangeTool {
        /// 新的工具名
        tool_name: String,
        /// 新的参数
        parameters: serde_json::Value,
        /// 更换原因
        reason: String,
    },
    /// 中止计划执行
    Abort {
        /// 中止原因
        reason: String,
    },
}

/// LLM 分析决策结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmDecision {
    /// LLM 的分析说明
    pub analysis: String,
    /// 决定执行的动作
    pub action: StepAction,
}

impl LlmDecision {
    /// 创建一个重试决策
    pub fn retry(analysis: impl Into<String>, new_parameters: Option<serde_json::Value>) -> Self {
        Self {
            analysis: analysis.into(),
            action: StepAction::Retry { new_parameters },
        }
    }

    /// 创建一个跳过决策
    pub fn skip(reason: impl Into<String>) -> Self {
        let reason_str = reason.into();
        Self {
            analysis: format!("跳过当前步骤: {}", reason_str),
            action: StepAction::Skip { reason: reason_str },
        }
    }

    /// 创建一个更换工具决策
    pub fn change_tool(tool_name: impl Into<String>, parameters: serde_json::Value, reason: impl Into<String>) -> Self {
        let tool_name_str = tool_name.into();
        let reason_str = reason.into();
        Self {
            analysis: format!("更换工具: {}", reason_str),
            action: StepAction::ChangeTool {
                tool_name: tool_name_str,
                parameters,
                reason: reason_str,
            },
        }
    }

    /// 创建一个中止决策
    pub fn abort(reason: impl Into<String>) -> Self {
        let reason_str = reason.into();
        Self {
            analysis: format!("中止执行: {}", reason_str),
            action: StepAction::Abort { reason: reason_str },
        }
    }
}