use crate::db::DbState;
use crate::provider::mcp_v2::config::{McpServerConfig, TransportConfig};
use crate::provider::mcp_v2::error::McpManagerError;
use sea_orm::{EntityTrait, QueryOrder};
use crate::entity::mcp_serve_config::{self as msc, McpModelConfig};
use std::sync::Arc;
use crate::provider::mcp_v2::{
    ServerManager
};
use crate::provider::cache::Cache;

impl TryFrom<msc::Model> for McpServerConfig {
    type Error = McpManagerError;

    fn try_from(model: msc::Model) -> Result<Self, Self::Error> {
        let config: McpModelConfig = serde_json::from_str(&model.config)?;

        let transport = match config.transport.as_str() {
            "http" => TransportConfig::Http {
                url: config.url.ok_or_else(|| McpManagerError::TransportError {
                    message: format!("MCP服务 '{}' HTTP传输缺少url", model.name),
                })?,
            },
            "stdio" => TransportConfig::Stdio {
                command: config.command.ok_or_else(|| McpManagerError::TransportError {
                    message: format!("MCP服务 '{}' STDIO传输缺少command", model.name),
                })?,
                args: config.args.unwrap_or_default(),
            },
            other => {
                return Err(McpManagerError::TransportError {
                    message: format!("MCP服务 '{}' 未知传输类型 '{}'", model.name, other),
                });
            }
        };

        Ok(McpServerConfig {
            id: model.id.to_string(),
            name: model.name,
            transport,
        })
    }
}


pub async fn init_mcp_v2(db_state: &DbState) -> Result<Arc<ServerManager>, Box<dyn std::error::Error + Send + Sync>> {
    // 1. 创建文件缓存
    let tool_cache = Arc::new(Cache::open("./mcp-v2-cache")?);
    // 2. 准备初始配置
    let configs = get_mcp_services(&db_state).await?;

    // 3. 创建 ServerManager（启动时建立连接，从缓存恢复工具清单）
    let manager = ServerManager::new(configs, tool_cache).await?;
    Ok(Arc::new(manager))
}

/// 批量初始化所有已保存的 MCP 服务
 async fn get_mcp_services(
    db_state: &DbState,
) -> Result<Vec<McpServerConfig>, McpManagerError> {
    //获取所有mcp 
    let db: Arc<sea_orm::prelude::DatabaseConnection> = db_state.get().await.map_err(|e| McpManagerError::Internal { message: e.to_string() })?;
    let configs = msc::Entity::find()
        .order_by_asc(msc::Column::Id)
        .all(&*db)
        .await
        .map_err(|e| McpManagerError::Internal { message: e.to_string() })?;
    // 将 configs 转换为 McpServerConfig
    let mcp_configs: Vec<McpServerConfig> = configs
        .into_iter()
        .filter_map(|model| match McpServerConfig::try_from(model) {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::warn!("跳过无效的 MCP 服务配置: {}", e);
                None
            }
        })
        .collect();
    println!("从数据库加载了 {} 个 MCP 服务配置", mcp_configs.len());
    Ok(mcp_configs)
}

