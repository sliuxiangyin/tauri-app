//! MCP 配置数据服务
//!
//! 提供 MCP 服务配置的数据库操作接口，与业务逻辑解耦。

use crate::db::DbState;
use crate::entity::mcp_serve_config::{self as msc, McpModelConfig};
use crate::provider::mcp_v2::config::{McpServerConfig, TransportConfig};
use sea_orm::{EntityTrait, QueryOrder};
use std::sync::Arc;

/// MCP 配置数据访问错误
#[derive(Debug, thiserror::Error)]
pub enum McpDataError {
    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("config parse error: {0}")]
    ConfigParse(#[from] serde_json::Error),

    #[error("transport error: {message}")]
    TransportError { message: String },
}

/// MCP 服务配置数据记录（带原始 Entity）
pub struct McpConfigRecord {
    pub id: i32,
    pub name: String,
    pub config: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 获取所有 MCP 服务配置记录
pub async fn get_all_configs(db_state: &DbState) -> Result<Vec<McpConfigRecord>, McpDataError> {
    let db: Arc<sea_orm::prelude::DatabaseConnection> = db_state
        .get()
        .await
        .map_err(|e| McpDataError::Database(sea_orm::DbErr::Custom(e.to_string())))?;

    let configs = msc::Entity::find()
        .order_by_asc(msc::Column::Id)
        .all(&*db)
        .await?;

    Ok(configs
        .into_iter()
        .map(|m| McpConfigRecord {
            id: m.id,
            name: m.name,
            config: m.config,
            updated_at: m.updated_at,
        })
        .collect())
}

/// 获取单个 MCP 服务配置
pub async fn get_config_by_id(
    db_state: &DbState,
    id: i32,
) -> Result<Option<McpConfigRecord>, McpDataError> {
    let db: Arc<sea_orm::prelude::DatabaseConnection> = db_state
        .get()
        .await
        .map_err(|e| McpDataError::Database(sea_orm::DbErr::Custom(e.to_string())))?;

    let config = msc::Entity::find_by_id(id).one(&*db).await?;

    Ok(config.map(|m| McpConfigRecord {
        id: m.id,
        name: m.name,
        config: m.config,
        updated_at: m.updated_at,
    }))
}

/// 将数据库记录转换为 MCP 服务器配置
pub fn record_to_server_config(record: McpConfigRecord) -> Result<McpServerConfig, McpDataError> {
    let config: McpModelConfig = serde_json::from_str(&record.config)?;

    let transport = match config.transport.as_str() {
        "http" => TransportConfig::Http {
            url: config.url.ok_or_else(|| McpDataError::TransportError {
                message: format!("MCP服务 '{}' HTTP传输缺少url", record.name),
            })?,
        },
        "stdio" => TransportConfig::Stdio {
            command: config.command.ok_or_else(|| McpDataError::TransportError {
                message: format!("MCP服务 '{}' STDIO传输缺少command", record.name),
            })?,
            args: config.args.unwrap_or_default(),
        },
        other => {
            return Err(McpDataError::TransportError {
                message: format!("MCP服务 '{}' 未知传输类型 '{}'", record.name, other),
            });
        }
    };

    Ok(McpServerConfig {
        id: record.id.to_string(),
        name: record.name,
        transport,
    })
}

/// 批量转换配置记录为服务器配置（跳过无效配置）
pub fn records_to_server_configs(records: Vec<McpConfigRecord>) -> Vec<McpServerConfig> {
    records
        .into_iter()
        .filter_map(|record| match record_to_server_config(record) {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::warn!("跳过无效的 MCP 服务配置: {}", e);
                None
            }
        })
        .collect()
}

/// 将实体 Model 转换为 McpConfigRecord（兼容性函数）
impl From<msc::Model> for McpConfigRecord {
    fn from(model: msc::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            config: model.config,
            updated_at: model.updated_at,
        }
    }
}
