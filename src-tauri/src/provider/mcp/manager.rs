use super::client::McpClient;
use super::error::McpError;
use super::types::{McpServiceConfig, McpServiceInfo, ToolCallResult, ToolInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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
        config: McpServiceConfig,
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
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
