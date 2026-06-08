#![allow(dead_code)]
//! 计划执行实体
//! 对应 plans 表，存储 Agent 模式的执行计划和结果

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::provider::llm::agent::plan_executor::StepResult;
use crate::provider::llm::types::PlanStep;

/// 计划停止原因枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum PlanStopReason {
    Completed,
    NoFinalReply,
    PartialFailure,
    UserAbort,
    DependencyFailed,
    ToolNotFound,
}

impl PlanStopReason {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "completed" => PlanStopReason::Completed,
            "nofinalreply" | "no_final_reply" => PlanStopReason::NoFinalReply,
            "partialfailure" | "partial_failure" => PlanStopReason::PartialFailure,
            "userabort" | "user_abort" => PlanStopReason::UserAbort,
            "dependencyfailed" | "dependency_failed" => PlanStopReason::DependencyFailed,
            "toolnotfound" | "tool_not_found" => PlanStopReason::ToolNotFound,
            _ => PlanStopReason::Completed,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PlanStopReason::Completed => "completed",
            PlanStopReason::NoFinalReply => "no_final_reply",
            PlanStopReason::PartialFailure => "partial_failure",
            PlanStopReason::UserAbort => "user_abort",
            PlanStopReason::DependencyFailed => "dependency_failed",
            PlanStopReason::ToolNotFound => "tool_not_found",
        }
    }
}

/// 计划实体模型
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plans")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 关联的消息 ID
    #[sea_orm(column_name = "mid")]
    pub mid: String,

    /// 是否需要 Agent 模式执行
    #[sea_orm(column_name = "need_agent")]
    pub need_agent: String,

    /// LLM 判断理由
    #[sea_orm(column_name = "reasoning", nullable)]
    pub reasoning: Option<String>,

    /// 执行步骤列表（JSON 数组，对应 Vec<PlanStep>）
    #[sea_orm(column_name = "steps", nullable)]
    pub steps: Option<String>,

    /// 步骤执行结果（JSON 数组，对应 Vec<StepResult>）
    #[sea_orm(column_name = "step_results", nullable)]
    pub step_results: Option<String>,

    /// 停止原因
    #[sea_orm(column_name = "stop_reason", nullable)]
    pub stop_reason: Option<String>,

    /// 完成时间（Unix 时间戳秒数）
    #[sea_orm(column_name = "completed_at", nullable)]
    pub completed_at: Option<i64>,

    /// 块序号（与 conversations.order_num 共享排序空间）
    #[sea_orm(column_name = "order_num")]
    pub order_num: i32,

    /// 创建时间（Unix 时间戳秒数）
    #[sea_orm(column_name = "created_at")]
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

// ──────────────────────────────────────────────────────────────
// Payload 结构
// ──────────────────────────────────────────────────────────────

/// 创建计划的负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlanPayload {
    pub mid: String,
    pub need_agent: bool,
    /// 排序位置（与 conversations.order_num 共享排序空间）
    #[serde(default)]
    pub order_num: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_results: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

impl CreatePlanPayload {
    /// 创建新的 Plan 负载
    pub fn new(mid: String, need_agent: bool, reasoning: Option<String>) -> Self {
        Self {
            mid,
            need_agent,
            order_num: 0,
            reasoning,
            steps: None,
            step_results: None,
            stop_reason: None,
            completed_at: None,
        }
    }

    /// 从 IntentPlan 创建
    pub fn from_intent_plan(mid: String, intent_plan: &crate::provider::llm::types::IntentPlan) -> Self {
        let steps_json = serde_json::to_string(&intent_plan.steps).ok();
        Self {
            mid,
            need_agent: intent_plan.need_agent,
            order_num: 0,
            reasoning: Some(intent_plan.reasoning.clone()),
            steps: steps_json,
            step_results: None,
            stop_reason: None,
            completed_at: None,
        }
    }

    /// 设置排序位置
    pub fn with_order_num(mut self, order_num: i32) -> Self {
        self.order_num = order_num;
        self
    }

    /// 设置步骤列表
    pub fn with_steps(mut self, steps: Vec<PlanStep>) -> Self {
        self.steps = serde_json::to_string(&steps).ok();
        self
    }

    /// 获取步骤列表
    pub fn get_steps(&self) -> Option<Vec<PlanStep>> {
        self.steps.as_ref().and_then(|s| serde_json::from_str(s).ok())
    }
}

/// 更新计划结果的负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePlanPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_results: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

impl UpdatePlanPayload {
    /// 创建空的更新负载
    pub fn new() -> Self {
        Self {
            step_results: None,
            stop_reason: None,
            completed_at: None,
        }
    }

    /// 设置步骤结果
    pub fn with_step_results(mut self, results: Vec<StepResult>) -> Self {
        self.step_results = serde_json::to_string(&results).ok();
        self
    }

    /// 获取步骤结果
    pub fn get_step_results(&self) -> Option<Vec<StepResult>> {
        self.step_results.as_ref().and_then(|s| serde_json::from_str(s).ok())
    }

    /// 设置停止原因
    pub fn with_stop_reason(mut self, reason: PlanStopReason) -> Self {
        self.stop_reason = Some(reason.as_str().to_string());
        self
    }

    /// 设置完成时间
    pub fn with_completed_at(mut self, timestamp: i64) -> Self {
        self.completed_at = Some(timestamp);
        self
    }

    /// 从 PlanResult 创建
    pub fn from_plan_result(result: &crate::provider::llm::agent::plan_executor::PlanResult) -> Self {
        let step_results_json = serde_json::to_string(&result.step_results).ok();
        let stop_reason_str = match &result.stop_reason {
            crate::provider::llm::agent::plan_executor::PlanStopReason::Completed => "completed",
            crate::provider::llm::agent::plan_executor::PlanStopReason::NoFinalReply => "no_final_reply",
            crate::provider::llm::agent::plan_executor::PlanStopReason::PartialFailure => "partial_failure",
            crate::provider::llm::agent::plan_executor::PlanStopReason::UserAbort => "user_abort",
            crate::provider::llm::agent::plan_executor::PlanStopReason::DependencyFailed => "dependency_failed",
            crate::provider::llm::agent::plan_executor::PlanStopReason::ToolNotFound => "tool_not_found",
        };
        Self {
            step_results: step_results_json,
            stop_reason: Some(stop_reason_str.to_string()),
            completed_at: Some(chrono::Utc::now().timestamp()),
        }
    }
}

impl Default for UpdatePlanPayload {
    fn default() -> Self {
        Self::new()
    }
}