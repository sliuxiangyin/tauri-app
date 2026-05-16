use super::client::McpClient;
use super::error::McpError;
use super::types::{McpModelConfig, McpServiceInfo, McpStateResult, ToolCallResult, ToolInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 批量连接结果
#[derive(Debug, Clone)]
pub struct ConnectAllResult {
    pub success: Vec<String>,
    pub failed: Vec<(String, String)>,
}

pub struct McpManager {
    clients: Arc<RwLock<HashMap<String, McpClient>>>,
    services: Arc<RwLock<HashMap<String, McpServiceInfo>>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 连接 MCP 服务
    pub async fn connect(
        &self,
        service_id: String,
        name: Option<String>,
        config: McpModelConfig,
    ) -> Result<(), McpError> {
        let client = McpClient::new(config.clone())?;
        client.connect_with_retry().await?;

        let service_info = McpServiceInfo {
            service_id: service_id.clone(),
            name,
            config,
            connected: true,
            last_connected_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
        };

        let mut clients = self.clients.write().await;
        let mut services = self.services.write().await;
        clients.insert(service_id.clone(), client);
        services.insert(service_id, service_info);

        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&self, service_id: &str) -> Result<(), McpError> {
        if let Some(client) = self.clients.write().await.remove(service_id) {
            client.disconnect().await?;
        }

        if let Some(service) = self.services.write().await.get_mut(service_id) {
            service.connected = false;
        }

        Ok(())
    }

    /// 获取工具列表
    pub async fn list_tools(
        &self,
        service_id: &str,
        force_refresh: bool,
    ) -> Result<Vec<ToolInfo>, McpError> {
        let clients = self.clients.read().await;
        let client = clients
            .get(service_id)
            .ok_or_else(|| McpError::ServiceNotFound(service_id.to_string()))?;

        client.list_tools(force_refresh).await
    }

    /// 调用工具
    pub async fn call_tool(
        &self,
        service_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolCallResult, McpError> {
        let clients = self.clients.read().await;
        let client = clients
            .get(service_id)
            .ok_or_else(|| McpError::ServiceNotFound(service_id.to_string()))?;

        client.call_tool(tool_name, arguments).await
    }

    /// 列出所有已连接的服务
    pub async fn list_services(&self) -> Result<Vec<McpServiceInfo>, McpError> {
        let services = self.services.read().await;
        Ok(services.values().cloned().collect())
    }

    /// 获取服务信息
    pub async fn get_service_info(&self, service_id: &str) -> Result<McpServiceInfo, McpError> {
        let services = self.services.read().await;
        services
            .get(service_id)
            .cloned()
            .ok_or_else(|| McpError::ServiceNotFound(service_id.to_string()))
    }

    /// 检查服务连接状态
    pub async fn is_service_connected(&self, service_id: &str) -> bool {
        if let Some(client) = self.clients.read().await.get(service_id) {
            client.is_connected().await
        } else {
            false
        }
    }

    /// 清除指定服务的工具缓存
    pub async fn clear_tools_cache(&self, service_id: &str) -> Result<(), McpError> {
        let clients = self.clients.read().await;
        if let Some(client) = clients.get(service_id) {
            client.clear_tools_cache().await;
            Ok(())
        } else {
            Err(McpError::ServiceNotFound(service_id.to_string()))
        }
    }

    /// 动态注册并连接单个 MCP 服务
    pub async fn register_service(
        &self,
        service_id: String,
        name: Option<String>,
        config: McpModelConfig,
    ) -> Result<(), McpError> {
        // 如果已存在，先注销旧的
        if self.clients.read().await.contains_key(&service_id) {
            self.unregister_service(&service_id).await?;
        }

        self.connect(service_id, name, config).await
    }

    /// 动态注销并断开单个 MCP 服务
    pub async fn unregister_service(&self, service_id: &str) -> Result<(), McpError> {
        // 先断开连接
        if let Some(client) = self.clients.write().await.remove(service_id) {
            let _ = client.disconnect().await;
        }

        // 从服务列表中移除
        self.services.write().await.remove(service_id);

        Ok(())
    }

    /// 批量连接 MCP 服务配置列表
    /// 失败时记录但不中断整体流程，返回成功/失败列表
    pub async fn connect_all(
        &self,
        configs: Vec<(String, Option<String>, McpModelConfig)>,
    ) -> ConnectAllResult {
        let mut success = Vec::new();
        let mut failed = Vec::new();

        for (service_id, name, config) in configs {
            match self.connect(service_id.clone(), name, config).await {
                Ok(()) => success.push(service_id),
                Err(e) => failed.push((service_id, e.to_string())),
            }
        }

        ConnectAllResult { success, failed }
    }

    /// 断开所有已连接的 MCP 服务
    pub async fn disconnect_all(&self) -> Result<(), McpError> {
        let service_ids: Vec<String> = self.clients.read().await.keys().cloned().collect();

        for service_id in service_ids {
            let _ = self.disconnect(&service_id).await;
        }

        Ok(())
    }

    /// 获取单个 MCP 服务的实时状态信息
    pub async fn get_service_state(&self, service_id: &str) -> Result<McpStateResult, McpError> {
        let services: tokio::sync::RwLockReadGuard<'_, HashMap<String, McpServiceInfo>> = self.services.read().await;
        let info = services
            .get(service_id)
            .cloned()
            .ok_or_else(|| McpError::ServiceNotFound(service_id.to_string()))?;
        drop(services);

        // 实时获取连接状态
        let state = self.is_service_connected(service_id).await;

        // 实时获取工具列表（如果连接正常）
        let tools = if state {
            match self.list_tools(service_id, false).await {
                Ok(t) => t,
                Err(e) => {
                    return Ok(McpStateResult {
                        id: info.service_id,
                        name: info.name.unwrap_or_default(),
                        state,
                        tools: Vec::new(),
                        error: Some(e.to_string()),
                    });
                }
            }
        } else {
            Vec::new()
        };

        Ok(McpStateResult {
            id: info.service_id,
            name: info.name.unwrap_or_default(),
            state,
            tools,
            error: None,
        })
    }

    /// 获取所有 MCP 服务的实时状态信息
    pub async fn get_all_service_states(&self) -> Vec<McpStateResult> {
        let service_ids: Vec<String> = self.services.read().await.keys().cloned().collect();

        let mut results = Vec::new();
        for service_id in service_ids {
            match self.get_service_state(&service_id).await {
                Ok(state) => results.push(state),
                Err(e) => {
                    // 如果获取失败，构造一个错误状态
                    results.push(McpStateResult {
                        id: service_id.clone(),
                        name: service_id,
                        state: false,
                        tools: Vec::new(),
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        results
    }

}



impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
