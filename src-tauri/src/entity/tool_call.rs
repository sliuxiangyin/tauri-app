#![allow(dead_code)]
//! 工具调用实体
//! 对应 tool_calls 表，统一存储 conversation 和 plan 的工具调用记录

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 工具调用来源类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ToolCallType {
    /// 来自对话（普通工具调用）
    Conversation,
    /// 来自计划执行
    Plan,
}

impl ToolCallType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "conversation" => ToolCallType::Conversation,
            "plan" => ToolCallType::Plan,
            _ => ToolCallType::Conversation,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ToolCallType::Conversation => "conversation",
            ToolCallType::Plan => "plan",
        }
    }
}

/// 工具调用状态枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ToolCallStatus {
    Pending,
    Success,
    Failed,
}

impl ToolCallStatus {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => ToolCallStatus::Pending,
            "success" => ToolCallStatus::Success,
            "failed" => ToolCallStatus::Failed,
            _ => ToolCallStatus::Pending,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ToolCallStatus::Pending => "pending",
            ToolCallStatus::Success => "success",
            ToolCallStatus::Failed => "failed",
        }
    }
}

/// 工具调用实体模型
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tool_calls")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 调用来源类型：conversation / plan
    #[sea_orm(column_name = "type")]
    pub type_: String,

    /// 对应来源的 ID（conversations.id 或 plans.id）
    #[sea_orm(column_name = "type_id")]
    pub type_id: String,

    /// 关联的消息 ID（用于快速查询）
    #[sea_orm(column_name = "mid")]
    pub mid: String,

    /// 调用顺序号（同一消息内按顺序递增）
    #[sea_orm(column_name = "order_num", nullable)]
    pub order_num: Option<i32>,

    /// 工具名称
    #[sea_orm(column_name = "tool_name")]
    pub tool_name: String,

    /// 调用参数（JSON）
    #[sea_orm(column_name = "arguments", nullable)]
    pub arguments: Option<String>,

    /// 执行结果
    #[sea_orm(column_name = "output", nullable)]
    pub output: Option<String>,

    /// 调用状态：pending / success / failed
    #[sea_orm(column_name = "status")]
    pub status: String,

    /// 执行耗时（毫秒）
    #[sea_orm(column_name = "duration_ms", nullable)]
    pub duration_ms: Option<i64>,

    /// 错误信息（失败时）
    #[sea_orm(column_name = "error", nullable)]
    pub error: Option<String>,

    /// 创建时间（Unix 时间戳秒数）
    #[sea_orm(column_name = "created_at")]
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 查询工具调用的选项
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct ToolCallQueryOptions {
    /// 调用来源类型
    pub type_: Option<String>,
    /// 来源 ID
    pub type_id: Option<String>,
    /// 消息 ID
    pub mid: Option<String>,
    /// 调用状态
    pub status: Option<String>,
    /// 消息数量限制
    pub limit: Option<u64>,
}

/// 创建工具调用的负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateToolCallPayload {
    pub type_: String,
    pub type_id: String,
    pub mid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_num: Option<i32>,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 更新工具调用结果的负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateToolCallPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}