use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// Model.config is McpModelConfig
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

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "mcp_serve_config")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i32,
    pub name: String,
    pub config: String,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
