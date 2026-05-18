//! MCP 服务初始化与运行时管理
//!
//! 职责：
//! - MCP v2 服务的批量初始化
//! - 运行时状态管理
//! - 不直接访问数据库，通过 services::db 获取数据

use crate::provider::cache::Cache;
use crate::provider::mcp_v2::error::McpManagerError;
use crate::provider::mcp_v2::{McpV2Api, ServerManager};
use crate::services::db::mcp as mcp_db;
use std::sync::Arc;

/// MCP v2 服务初始化器
pub struct McpServiceInitializer {
    #[allow(dead_code)]
    manager: Option<Arc<ServerManager>>,
    api: Option<McpV2Api>,
    #[allow(dead_code)]
    initialized: bool,
}

impl McpServiceInitializer {
    /// 创建新的初始化器
    pub fn new() -> Self {
        Self {
            manager: None,
            api: None,
            initialized: false,
        }
    }

    /// 执行完整的初始化流程
    #[allow(dead_code)]
    pub async fn initialize(
        &mut self,
        db_state: &crate::db::DbState,
        cache: Arc<Cache>,
    ) -> Result<&McpV2Api, McpManagerError> {
        if self.initialized {
            return self.api.as_ref().ok_or_else(|| McpManagerError::Internal {
                message: "MCP service initialized but API is None".into(),
            });
        }

        // 1. 通过数据服务层获取配置
        let records =
            mcp_db::get_all_configs(db_state)
                .await
                .map_err(|e| McpManagerError::Internal {
                    message: format!("failed to load MCP configs: {}", e),
                })?;

        // 2. 转换记录为服务器配置
        let configs = mcp_db::records_to_server_configs(records);
        println!("从数据库加载了 {} 个 MCP 服务配置", configs.len());

        // 3. 创建 ServerManager
        let manager = Arc::new(ServerManager::new(configs, cache).await?);

        // 4. 创建 API 包装
        let api = McpV2Api::new(manager);

        self.manager = Some(api.manager().clone());
        self.api = Some(api);
        self.initialized = true;

        Ok(self.api.as_ref().unwrap())
    }

    /// 获取 API 引用（仅在初始化后有效）
    #[allow(dead_code)]
    pub fn get_api(&self) -> Option<&McpV2Api> {
        self.api.as_ref()
    }

    /// 检查是否已初始化
    #[allow(dead_code)]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// 获取 Manager 引用
    #[allow(dead_code)]
    pub fn get_manager(&self) -> Option<Arc<ServerManager>> {
        self.manager.clone()
    }
}

impl Default for McpServiceInitializer {
    fn default() -> Self {
        Self::new()
    }
}


/// 初始化并返回 API（推荐使用）
#[allow(dead_code)]
pub async fn init_mcp_v2_with_api(
    db_state: &crate::db::DbState,
    cache: Arc<Cache>,
) -> Result<McpV2Api, McpManagerError> {
    let configs = mcp_db::records_to_server_configs(
        mcp_db::get_all_configs(db_state).await
            .map_err(|e| McpManagerError::Internal { message: e.to_string() })?
    );
    let manager = ServerManager::new(configs, cache).await
        .map_err(|e| McpManagerError::Internal { message: e.to_string() })?;
    Ok(McpV2Api::new(Arc::new(manager)))
}
