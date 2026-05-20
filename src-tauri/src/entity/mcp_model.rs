use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpModelConfig {
    pub transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// operating 状态枚举
pub const OPERATING_IDLE: &str = "idle";
pub const OPERATING_INSTALLING: &str = "installing";
pub const OPERATING_LOADING: &str = "loading";
pub const OPERATING_RUNNING: &str = "running";
pub const OPERATING_FAILED: &str = "failed";

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "mcp")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i32,
    pub name: String,
    pub transport: String,  // "stdio" | "http"
    pub config: String,     // JSON 配置字符串
    pub status: String,      // "disable" | "enable"
    pub operating: String,   // "idle" | "installing" | "loading" | "running" | "failed"
    pub tools: String,       // JSON 数组 ["tool1", "tool2"]
    pub error_msg: String,   // 失败时的错误信息
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}