use crate::provider::mcp::manager::McpManager;
use crate::provider::mcp::types::{McpServiceConfig, ToolInfo, ToolCallResult};
use serde::Deserialize;
use tauri::State;

#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub service_id: String,
    pub name: Option<String>,
    pub config: McpServiceConfig,
}

#[derive(Debug, Deserialize)]
pub struct ListToolsRequest {
    pub service_id: String,
    #[serde(default)]
    pub force_refresh: bool,
}

#[derive(Debug, Deserialize)]
pub struct CallToolRequest {
    pub service_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// 连接 MCP 服务
#[tauri::command]
pub async fn mcp_connect(
    manager: State<'_, McpManager>,
    req: ConnectRequest,
) -> Result<String, String> {
    manager
        .connect(req.service_id.clone(), req.name, req.config)
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("Connected to service: {}", req.service_id))
}

/// 断开连接
#[tauri::command]
pub async fn mcp_disconnect(
    manager: State<'_, McpManager>,
    service_id: String,
) -> Result<String, String> {
    manager
        .disconnect(&service_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("Disconnected from service: {}", service_id))
}

/// 获取工具列表
#[tauri::command]
pub async fn mcp_list_tools(
    manager: State<'_, McpManager>,
    req: ListToolsRequest,
) -> Result<Vec<ToolInfo>, String> {
    manager
        .list_tools(&req.service_id, req.force_refresh)
        .await
        .map_err(|e| e.to_string())
}

/// 调用工具
#[tauri::command]
pub async fn mcp_call_tool(
    manager: State<'_, McpManager>,
    req: CallToolRequest,
) -> Result<ToolCallResult, String> {
    manager
        .call_tool(&req.service_id, &req.tool_name, req.arguments)
        .await
        .map_err(|e| e.to_string())
}

/// 列出所有连接的服务
#[tauri::command]
pub async fn mcp_list_services(
    manager: State<'_, McpManager>,
) -> Result<Vec<crate::provider::mcp::McpServiceInfo>, String> {
    manager
        .list_services()
        .await
        .map_err(|e| e.to_string())
}

/// 检查服务连接状态
#[tauri::command]
pub async fn mcp_is_service_connected(
    manager: State<'_, McpManager>,
    service_id: String,
) -> Result<bool, String> {
    Ok(manager.is_service_connected(&service_id).await)
}

/// 清除工具缓存
#[tauri::command]
pub async fn mcp_clear_tools_cache(
    manager: State<'_, McpManager>,
    service_id: String,
) -> Result<String, String> {
    manager
        .clear_tools_cache(&service_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("Tools cache cleared for service: {}", service_id))
}