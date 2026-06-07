//! 对话内容块实体（统一内容模型）
//! 对应 conversations 表，存储消息的所有内容块：文本、思考、工具调用、工具结果
//! 通过 block_type 区分类型，order_num 保证顺序

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 内容块类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockType {
    /// 普通文本内容
    Text,
    /// 思考过程/推理链
    Thinking,
    /// 工具调用
    ToolCall,
    /// 工具调用结果
    ToolResult,
}

impl BlockType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "text" => BlockType::Text,
            "thinking" => BlockType::Thinking,
            "tool_call" => BlockType::ToolCall,
            "tool_result" => BlockType::ToolResult,
            _ => BlockType::Text,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            BlockType::Text => "text",
            BlockType::Thinking => "thinking",
            BlockType::ToolCall => "tool_call",
            BlockType::ToolResult => "tool_result",
        }
    }
}

/// 内容来源枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockSource {
    /// 普通对话
    Chat,
    /// Plan 执行
    Plan,
}

impl BlockSource {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "plan" => BlockSource::Plan,
            _ => BlockSource::Chat,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            BlockSource::Chat => "chat",
            BlockSource::Plan => "plan",
        }
    }
}

/// 对话内容块实体模型
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "conversations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 关联的消息 ID → messages.id
    #[sea_orm(column_name = "mid")]
    pub mid: String,

    /// 内容块类型：text / thinking / tool_call / tool_result
    #[sea_orm(column_name = "block_type")]
    pub block_type: String,

    /// 块序号（同一消息内按顺序递增）
    #[sea_orm(column_name = "order_num")]
    pub order_num: i32,

    /// 来源：chat / plan
    #[sea_orm(column_name = "source")]
    pub source: String,

    /// 来源 ID（Plan 执行时关联 plans.id）
    #[sea_orm(column_name = "source_id", nullable)]
    pub source_id: Option<String>,

    /// Plan 步骤序号（Plan 执行时标识第几步）
    #[sea_orm(column_name = "step_index", nullable)]
    pub step_index: Option<i32>,

    /// 文本内容（text / tool_result 类型使用）
    #[sea_orm(column_name = "content", nullable)]
    pub content: Option<String>,

    /// 内容摘要
    #[sea_orm(column_name = "content_summary", nullable)]
    pub content_summary: Option<String>,

    /// 思考过程（thinking 类型使用）
    #[sea_orm(column_name = "thinking", nullable)]
    pub thinking: Option<String>,

    /// 工具名称（tool_call 类型使用）
    #[sea_orm(column_name = "tool_name", nullable)]
    pub tool_name: Option<String>,

    /// 工具调用参数 JSON（tool_call 类型使用）
    #[sea_orm(column_name = "tool_arguments", nullable)]
    pub tool_arguments: Option<String>,

    /// 工具执行结果（tool_result 类型使用）
    #[sea_orm(column_name = "tool_output", nullable)]
    pub tool_output: Option<String>,

    /// 工具执行状态：pending / success / failed
    #[sea_orm(column_name = "tool_status", nullable)]
    pub tool_status: Option<String>,

    /// 工具执行耗时（毫秒）
    #[sea_orm(column_name = "tool_duration_ms", nullable)]
    pub tool_duration_ms: Option<i64>,

    /// 工具错误信息
    #[sea_orm(column_name = "tool_error", nullable)]
    pub tool_error: Option<String>,

    /// 扩展字段 JSON
    #[sea_orm(column_name = "extends")]
    pub extends: String,

    /// 附件 JSON 数组
    #[sea_orm(column_name = "attachments", nullable)]
    pub attachments: Option<String>,

    /// 元数据 JSON
    #[sea_orm(column_name = "metadata")]
    pub metadata: String,

    /// 创建时间（Unix 时间戳毫秒数）
    #[sea_orm(column_name = "created_at")]
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 创建内容块的负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConversationPayload {
    pub mid: String,
    pub block_type: String,
    pub order_num: i32,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

fn default_source() -> String {
    "chat".to_string()
}

/// 查询内容块的选项
#[derive(Debug, Default)]
pub struct ConversationQueryOptions {
    /// 消息 ID（必填）
    pub mid: String,
    /// 来源过滤
    pub source: Option<String>,
    /// 块类型过滤
    pub block_type: Option<String>,
}
