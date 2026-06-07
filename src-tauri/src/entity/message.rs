//! 消息索引实体（简化版）
//! 对应 messages 表，只保留核心索引字段
//! 内容存储在 conversations 表，通过 mid 关联

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 消息角色枚举（LLM 语义角色）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

impl MessageRole {
    /// 从字符串转换为枚举
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        }
    }

    /// 转换为字符串
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        }
    }
}

/// 聊天类型枚举（消息来源渠道）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ChatType {
    Client,
    Wechat,
}

impl ChatType {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "client" => ChatType::Client,
            "wechat" => ChatType::Wechat,
            _ => ChatType::Client,
        }
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatType::Client => "client",
            ChatType::Wechat => "wechat",
        }
    }
}

/// 消息状态枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum MessageStatus {
    Pending,
    Completed,
    Failed,
}

impl MessageStatus {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => MessageStatus::Pending,
            "completed" => MessageStatus::Completed,
            "failed" => MessageStatus::Failed,
            _ => MessageStatus::Completed,
        }
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageStatus::Pending => "pending",
            MessageStatus::Completed => "completed",
            MessageStatus::Failed => "failed",
        }
    }
}

/// 消息索引实体模型（简化版）
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "messages")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// 账号 ID（多账号隔离）
    #[sea_orm(column_name = "account_id")]
    pub account_id: String,

    /// 聊天类型：client / wechat
    #[sea_orm(column_name = "chat_type")]
    pub chat_type: String,

    /// 会话 ID
    #[sea_orm(column_name = "session_id")]
    pub session_id: String,

    /// 父消息 ID（用于消息树结构）
    #[sea_orm(column_name = "parent_id", nullable)]
    pub parent_id: Option<String>,

    /// 消息角色：user / assistant / system / tool
    #[sea_orm(column_name = "role")]
    pub role: String,

    /// 消息状态
    #[sea_orm(column_name = "status")]
    pub status: String,

    /// Token 使用量 JSON
    #[sea_orm(column_name = "token_usage", nullable)]
    pub token_usage: Option<String>,

    /// 创建时间（Unix 时间戳秒数）
    #[sea_orm(column_name = "created_at")]
    pub created_at: i64,

    /// 是否删除
    #[sea_orm(column_name = "is_deleted")]
    pub is_deleted: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 查询消息的选项
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct MessageQueryOptions {
    /// 账号 ID（必填）
    pub account_id: String,
    /// 会话 ID（可选，默认 "default"）
    pub session_id: Option<String>,
    /// 聊天类型（可选）
    pub chat_type: Option<String>,
    /// 消息角色（可选）
    pub role: Option<String>,
    /// 消息状态（可选）
    pub status: Option<String>,
    /// 消息数量限制（可选，默认 50）
    pub limit: Option<u64>,
    /// 偏移量（可选，默认 0）
    pub offset: Option<u64>,
}

/// 创建消息的负载（简化版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessagePayload {
    pub account_id: String,
    pub chat_type: String,
    pub session_id: String,
    pub role: String,
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<String>,
}