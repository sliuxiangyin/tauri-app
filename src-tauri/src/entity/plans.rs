//! 计划执行实体
//! 对应 plans 表，存储 Agent 模式的执行计划和结果

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

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

    /// 创建时间（Unix 时间戳秒数）
    #[sea_orm(column_name = "created_at")]
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 创建计划的负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlanPayload {
    pub mid: String,
    pub need_agent: bool,
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